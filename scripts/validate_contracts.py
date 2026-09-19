#!/usr/bin/env python3
"""Validate hand-authored contracts, fixtures, and the Axum route inventory.

The validator uses exact-pinned Python distributions and the official
AsyncAPI 3.0.0 JSON Schema vendored at a pinned source revision. It never
writes generated contract source.
"""

from __future__ import annotations

import hashlib
import importlib.metadata
import json
import re
import sys
import warnings
from pathlib import Path
from typing import Any, NoReturn

YAML_VERSION = "6.0.3"
JSONSCHEMA_VERSION = "4.25.1"
OPENAPI_VALIDATOR_VERSION = "0.7.2"
ROOT = Path(__file__).resolve().parents[1]
SPEC = ROOT / "spec"
FIXTURES = SPEC / "fixtures"
ROUTER_SOURCE = ROOT / "crates" / "double_riichi_server" / "src" / "http.rs"
ASYNCAPI_SCHEMA = SPEC / "asyncapi-schema-3.0.0.json"
ASYNCAPI_SCHEMA_SOURCE = SPEC / "asyncapi-schema-3.0.0.source"
ASYNCAPI_SCHEMA_SHA256 = "abe96881dbfaad495ccbb6bd8d3fb43ca18a13b9d7564849ccb66f39d5e5b20f"


def fail(message: str) -> NoReturn:
    raise SystemExit(f"contract validation failed: {message}")


def require_distribution(name: str, expected: str) -> None:
    try:
        actual = importlib.metadata.version(name)
    except importlib.metadata.PackageNotFoundError as error:
        fail(
            f"{name}=={expected} is required; run `just contracts-install` "
            f"({error})"
        )
    if actual != expected:
        fail(f"expected {name}=={expected}, found {actual}")


def validate_requirements_lock() -> None:
    requirements = ROOT / "scripts" / "requirements-contracts.txt"
    for line in requirements.read_text(encoding="utf-8").splitlines():
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        name, separator, expected = line.partition("==")
        if not separator or not name or not expected:
            fail(f"contract dependency is not exactly pinned: {line!r}")
        require_distribution(name, expected)


def load_yaml(path: Path) -> dict[str, Any]:
    require_distribution("PyYAML", YAML_VERSION)
    try:
        import yaml
    except ImportError as error:  # pragma: no cover - distribution failure
        fail(f"PyYAML=={YAML_VERSION} cannot be imported ({error})")
    try:
        value = yaml.safe_load(path.read_text(encoding="utf-8"))
    except Exception as error:  # pragma: no cover - parser diagnostics
        fail(f"{path.relative_to(ROOT)} is not valid YAML: {error}")
    if not isinstance(value, dict):
        fail(f"{path.relative_to(ROOT)} must contain a mapping")
    return value


def load_json(path: Path) -> Any:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except FileNotFoundError as error:
        fail(f"missing {path.relative_to(ROOT)} ({error})")
    except json.JSONDecodeError as error:
        fail(f"{path.relative_to(ROOT)} is not valid JSON: {error}")


def load_fixture(name: str) -> dict[str, Any]:
    path = FIXTURES / name
    raw = path.read_bytes()
    if b"\0" in raw:
        fail(f"{path.relative_to(ROOT)} contains NUL bytes")
    value = load_json(path)
    if not isinstance(value, dict):
        fail(f"{path.relative_to(ROOT)} must contain an object")
    return value


def schema_at(document: dict[str, Any], reference: str) -> dict[str, Any]:
    if not reference.startswith("#/"):
        fail(f"unsupported local schema reference {reference}")
    value: Any = document
    for part in reference[2:].split("/"):
        value = value[part.replace("~1", "/").replace("~0", "~")]
    if not isinstance(value, dict):
        fail(f"schema reference {reference} does not resolve to an object")
    return value


def validate_instance(
    instance: Any,
    schema: dict[str, Any],
    root: dict[str, Any],
    label: str,
) -> None:
    require_distribution("jsonschema", JSONSCHEMA_VERSION)
    error = None
    try:
        with warnings.catch_warnings():
            warnings.simplefilter("ignore", DeprecationWarning)
            from jsonschema import Draft202012Validator, RefResolver

        validator = Draft202012Validator(
            schema,
            resolver=RefResolver.from_schema(root),
        )
        error = next(iter(validator.iter_errors(instance)), None)
    except Exception as validation_error:  # pragma: no cover - dependency diagnostics
        fail(f"{label} schema validation could not run ({validation_error})")
    if error is not None:
        location = ".".join(str(part) for part in error.absolute_path) or "$"
        fail(f"{label} violates schema at {location}: {error.message}")


def validate_official_documents(openapi: dict[str, Any], asyncapi: dict[str, Any]) -> None:
    require_distribution("openapi-spec-validator", OPENAPI_VALIDATOR_VERSION)
    require_distribution("jsonschema", JSONSCHEMA_VERSION)
    try:
        validate_spec = importlib.import_module("openapi_spec_validator").validate_spec
    except ImportError as error:  # pragma: no cover - distribution failure
        fail(f"openapi-spec-validator=={OPENAPI_VALIDATOR_VERSION} cannot import ({error})")
    try:
        validate_spec(openapi)
    except Exception as error:
        fail(f"OpenAPI official schema validation failed: {error}")

    source = ASYNCAPI_SCHEMA_SOURCE.read_text(encoding="utf-8")
    if "source_package = @asyncapi/specs" not in source or "source_version = 6.11.1" not in source:
        fail("AsyncAPI schema source metadata is not pinned")
    schema_bytes = ASYNCAPI_SCHEMA.read_bytes().replace(b"\r\n", b"\n")
    if hashlib.sha256(schema_bytes).hexdigest() != ASYNCAPI_SCHEMA_SHA256:
        fail("vendored AsyncAPI schema checksum does not match its pinned source")
    schema = load_json(ASYNCAPI_SCHEMA)
    if not isinstance(schema, dict):
        fail("vendored AsyncAPI schema must contain an object")
    try:
        from jsonschema import Draft7Validator

        Draft7Validator.check_schema(schema)
        errors = sorted(Draft7Validator(schema).iter_errors(asyncapi), key=lambda item: list(item.path))
    except Exception as error:  # pragma: no cover - dependency diagnostics
        fail(f"AsyncAPI official schema validation could not run ({error})")
    if errors:
        error = errors[0]
        location = ".".join(str(part) for part in error.absolute_path) or "$"
        fail(f"AsyncAPI official schema validation failed at {location}: {error.message}")


def route_operations(source: str) -> dict[str, set[str]]:
    """Extract route methods without relying on generated source or line layout."""

    operations: dict[str, set[str]] = {}
    methods = {"get", "post", "put", "patch", "delete", "head", "options", "trace", "any"}
    cursor = 0
    while True:
        start = source.find(".route(", cursor)
        if start < 0:
            return operations
        path_match = re.match(r'\.route\(\s*"([^"]+)"\s*,', source[start:])
        if path_match is None:
            fail(f"could not parse router route near byte {start}")
        path = path_match.group(1)
        body_start = start + path_match.end()
        depth = 1
        index = body_start
        quote = False
        escaped = False
        while index < len(source) and depth:
            character = source[index]
            if quote:
                if escaped:
                    escaped = False
                elif character == "\\":
                    escaped = True
                elif character == '"':
                    quote = False
            elif character == '"':
                quote = True
            elif character == "(":
                depth += 1
            elif character == ")":
                depth -= 1
            index += 1
        if depth:
            fail(f"unterminated router route for {path}")
        body = source[body_start : index - 1]
        found = set(re.findall(r"\b(" + "|".join(methods) + r")\s*\(", body))
        if not found:
            fail(f"router route {path} has no HTTP method")
        operations.setdefault(path, set()).update(found)
        cursor = index


def validate_router_mapping(openapi: dict[str, Any]) -> None:
    source_operations = route_operations(ROUTER_SOURCE.read_text(encoding="utf-8"))
    local_operations = {
        path: methods
        for path, methods in source_operations.items()
        if path.startswith(("/api/v1/", "/assets/")) or path == "/status"
    }
    contract_operations = {
        path: {
            method.lower()
            for method in item
            if method.lower()
            in {"get", "post", "put", "patch", "delete", "head", "options", "trace"}
        }
        for path, item in openapi.get("paths", {}).items()
        if isinstance(item, dict)
    }
    if local_operations != contract_operations:
        missing = sorted(
            (path, method)
            for path, methods in local_operations.items()
            for method in methods - contract_operations.get(path, set())
        )
        extra = sorted(
            (path, method)
            for path, methods in contract_operations.items()
            for method in methods - local_operations.get(path, set())
        )
        fail(f"router/OpenAPI method mismatch; missing={missing}, extra={extra}")


def validate_dto_contracts(openapi: dict[str, Any]) -> None:
    schemas = openapi["components"]["schemas"]
    login = openapi["paths"]["/api/v1/admin/login"]["post"]["responses"]["200"]["content"]["application/json"]["schema"]
    if set(login.get("required", [])) != {"expires_at"} or set(login.get("properties", {})) != {"expires_at"}:
        fail("Admin login response must be exactly expires_at")
    for name in ("BotToken", "CreatedBotToken"):
        schema = schemas[name]
        required = set(schema.get("required", []))
        if not {"token_id", "name", "state", "created_at", "revoked_at"}.issubset(required):
            fail(f"{name} is missing durable state fields")
        if schema["properties"]["state"].get("enum") != ["active", "revoked"]:
            fail(f"{name}.state does not match TokenState")
        if schema["properties"]["created_at"].get("format") != "date-time":
            fail(f"{name}.created_at must be RFC3339")
        if schema["properties"]["revoked_at"].get("type") != ["string", "null"]:
            fail(f"{name}.revoked_at must be nullable RFC3339")
    if "token" not in schemas["CreatedBotToken"]["required"]:
        fail("CreatedBotToken.token is missing")

    for path in ("/api/v1/admin/rooms/{join_code}/start", "/api/v1/admin/rooms/{join_code}/rematch"):
        responses = openapi["paths"][path]["post"]["responses"]
        if "503" not in responses:
            fail(f"{path} must declare persistence failure status 503")
        if responses["503"].get("$ref") != "#/components/responses/ServiceUnavailable":
            fail(f"{path} must use the bounded ServiceUnavailable response")

    room_detail = schemas["RoomDetail"]
    room_detail_required = set(room_detail.get("required", []))
    room_detail_properties = room_detail.get("properties", {})
    for field in ("persistence_degraded", "replay_available"):
        if field not in room_detail_required:
            fail(f"RoomDetail.{field} must be required")
        if room_detail_properties.get(field, {}).get("type") != "boolean":
            fail(f"RoomDetail.{field} must be a boolean")

    for parameter in ("TokenId", "ParticipantId", "MatchId"):
        if openapi["components"]["parameters"][parameter]["schema"].get("maxLength") != 128:
            fail(f"{parameter} must declare the runtime identifier bound")

    for name in ("CreateRoomRequest", "PatchRoomRequest"):
        properties = schemas[name]["properties"]
        if set(properties) != {
            "room_name",
            "game_mode",
            "time_control",
            "replay_save",
            "participant_limit",
        }:
            fail(f"{name} does not enumerate the complete accepted field set")
        if properties["time_control"].get("enum") != [
            "casual",
            "riichi_dev",
            "riichi-dev",
            "unlimited",
        ]:
            fail(f"{name}.time_control enum does not match runtime")
        if properties["participant_limit"].get("minimum") != 3 or properties["participant_limit"].get("maximum") != 32:
            fail(f"{name}.participant_limit bounds do not match runtime")


def validate_asyncapi_events(asyncapi: dict[str, Any]) -> None:
    schemas = asyncapi["components"]["schemas"]
    room_event = schemas["roomEvent"]
    expected_events = {
        "snapshotEvent",
        "participantJoinedEvent",
        "participantLeftEvent",
        "selectionChangedEvent",
        "phaseChangedEvent",
        "matchStartedEvent",
        "matchCompletedEvent",
        "matchAbortedEvent",
        "storageDegradedEvent",
        "serverShutdownEvent",
        "roomDeletedEvent",
    }
    refs = {item.get("$ref", "").removeprefix("#/components/schemas/") for item in room_event.get("oneOf", [])}
    if refs != expected_events:
        fail("Human roomEvent does not enumerate the exact runtime event set")
    for name in expected_events:
        if schemas[name].get("additionalProperties") is not False:
            fail(f"{name} must reject unknown fields")
    phase = schemas["phaseChangedEvent"]
    if phase["properties"]["phase"].get("enum") != ["lobby", "playing", "post_match"]:
        fail("phase_changed schema has the wrong phase enum")
    close_codes = asyncapi["channels"]["humanRoom"].get("x-websocket-close-codes")
    if close_codes != {
        4001: "connected_elsewhere",
        4002: "room_deleted",
        4003: "server_shutdown",
        4005: "slow_consumer",
        4006: "session_expired",
    }:
        fail("Human WebSocket close codes do not match runtime")


def expected_fixture_metadata() -> dict[str, dict[str, Any]]:
    return {
        "health-200.json": {
            "file": "health-200.json",
            "method": "GET",
            "path": "/api/v1/health",
            "status": 200,
            "auth": "admin-cookie",
        },
        "health-503.json": {
            "file": "health-503.json",
            "method": "GET",
            "path": "/api/v1/health",
            "status": 503,
            "auth": "admin-cookie",
        },
        "health-503-replay.json": {
            "file": "health-503-replay.json",
            "method": "GET",
            "path": "/api/v1/health",
            "status": 503,
            "auth": "admin-cookie",
        },
        "public-status.json": {
            "file": "public-status.json",
            "method": "GET",
            "path": "/status",
            "status": 200,
            "auth": "none",
        },
        "public-room-lookup.json": {
            "file": "public-room-lookup.json",
            "method": "GET",
            "path": "/api/v1/rooms/{join_code}",
            "status": 200,
            "auth": "none",
        },
        "human-snapshot.json": {
            "file": "human-snapshot.json",
            "transport": "websocket",
            "path": "/ws/v1/rooms/{join_code}/human",
            "message": "snapshot",
        },
    }


def validate_fixtures(openapi: dict[str, Any], asyncapi: dict[str, Any]) -> None:
    index = load_fixture("fixture-index.json")
    if index.get("format") != "double-riichi-contract-fixtures-v1":
        fail("fixture index format is not pinned")
    validator_metadata = index.get("validator", {})
    if validator_metadata != {
        "script": "scripts/validate_contracts.py",
        "python_requirements": "scripts/requirements-contracts.txt",
        "yaml_dependency": "PyYAML==6.0.3",
        "jsonschema_dependency": "jsonschema==4.25.1",
        "openapi_dependency": "openapi-spec-validator==0.7.2",
        "asyncapi_schema": "spec/asyncapi-schema-3.0.0.json",
        "asyncapi_schema_source": "spec/asyncapi-schema-3.0.0.source",
    }:
        fail("fixture index validator metadata is not exact")
    entries = index.get("fixtures")
    expected = expected_fixture_metadata()
    if not isinstance(entries, list) or {entry.get("file") for entry in entries} != set(expected):
        fail("fixture index entries do not match the retained fixture set")
    for entry in entries:
        if entry != expected.get(entry.get("file")):
            fail(f"fixture metadata is not exact for {entry.get('file')}")

    health_schema = schema_at(openapi, "#/components/schemas/Health")
    status_schema = schema_at(openapi, "#/components/schemas/PublicStatus")
    room_schema = schema_at(openapi, "#/components/schemas/PublicRoomLookup")
    snapshot_schema = schema_at(asyncapi, "#/components/schemas/snapshotMessage")
    for name, schema, document in [
        ("health-200.json", health_schema, openapi),
        ("health-503.json", health_schema, openapi),
        ("health-503-replay.json", health_schema, openapi),
        ("public-status.json", status_schema, openapi),
        ("public-room-lookup.json", room_schema, openapi),
        ("human-snapshot.json", snapshot_schema, asyncapi),
    ]:
        validate_instance(load_fixture(name), schema, document, name)

    if load_fixture("health-200.json")["database"] != "ok":
        fail("health-200 fixture does not represent database ok")
    if load_fixture("health-503.json")["database"] != "degraded":
        fail("health-503 fixture does not represent database degraded")
    replay_health = load_fixture("health-503-replay.json")
    if replay_health["database"] != "ok" or replay_health["replay_storage"] != "degraded":
        fail("health-503-replay fixture does not represent replay storage degraded")
    human_message = asyncapi["components"]["messages"]["humanServerMessage"]["payload"]
    if not any(
        reference.get("$ref") == "#/components/schemas/snapshotMessage"
        for reference in human_message.get("oneOf", [])
    ):
        fail("Human server messages do not include snapshot")

    for entry in entries:
        if "method" not in entry:
            continue
        response = openapi["paths"][entry["path"]][entry["method"].lower()]["responses"]
        if str(entry["status"]) not in response:
            fail(f"fixture status {entry['status']} is absent from {entry['path']}")


def validate_contracts() -> None:
    validate_requirements_lock()
    openapi = load_yaml(SPEC / "openapi.yaml")
    asyncapi = load_yaml(SPEC / "asyncapi.yaml")
    if openapi.get("openapi") != "3.1.0":
        fail("OpenAPI version is not 3.1.0")
    if asyncapi.get("asyncapi") != "3.0.0":
        fail("AsyncAPI version is not 3.0.0")
    paths = openapi.get("paths")
    if not isinstance(paths, dict) or "/api/v1/health" not in paths:
        fail("OpenAPI health path is missing")
    if any(path.startswith("/ws/") for path in paths) or "/mcp" in paths:
        fail("OpenAPI must not duplicate Human WebSocket or MCP schemas")
    if "adminCookie" not in openapi.get("components", {}).get("securitySchemes", {}):
        fail("OpenAPI admin cookie scheme is missing")
    if "/api/v1/admin/openapi.yaml" not in paths:
        fail("Admin-gated OpenAPI path is missing")
    validate_dto_contracts(openapi)
    validate_asyncapi_events(asyncapi)
    validate_official_documents(openapi, asyncapi)
    validate_router_mapping(openapi)
    validate_fixtures(openapi, asyncapi)


def main() -> int:
    validate_contracts()
    print("contract validation passed: official OpenAPI/AsyncAPI schemas, fixtures, and router methods")
    return 0


if __name__ == "__main__":
    sys.exit(main())
