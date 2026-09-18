#!/usr/bin/env python3
"""Validate hand-authored contracts, fixtures, and the Axum route inventory.

The validator intentionally has one exact YAML parser pin so contract checks do
not silently vary between developer and CI environments.
"""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path

YAML_VERSION = "6.0.3"
ROOT = Path(__file__).resolve().parents[1]
SPEC = ROOT / "spec"
FIXTURES = SPEC / "fixtures"
ROUTER_SOURCE = ROOT / "crates" / "double_riichi_server" / "src" / "http.rs"


def fail(message: str) -> "NoReturn":
    raise SystemExit(f"contract validation failed: {message}")


def load_yaml(path: Path) -> dict:
    try:
        import yaml
    except ImportError as error:  # pragma: no cover - environment failure
        fail(f"PyYAML=={YAML_VERSION} is required ({error})")
    if getattr(yaml, "__version__", None) != YAML_VERSION:
        fail(
            f"expected PyYAML=={YAML_VERSION}, found "
            f"{getattr(yaml, '__version__', 'unknown')}"
        )
    try:
        value = yaml.safe_load(path.read_text(encoding="utf-8"))
    except Exception as error:  # pragma: no cover - parser diagnostics
        fail(f"{path.relative_to(ROOT)} is not valid YAML: {error}")
    if not isinstance(value, dict):
        fail(f"{path.relative_to(ROOT)} must contain a mapping")
    return value


def load_json(name: str) -> dict:
    path = FIXTURES / name
    raw = path.read_bytes()
    if b"\0" in raw:
        fail(f"{path.relative_to(ROOT)} contains NUL bytes")
    try:
        value = json.loads(raw)
    except json.JSONDecodeError as error:
        fail(f"{path.relative_to(ROOT)} is not valid JSON: {error}")
    if not isinstance(value, dict):
        fail(f"{path.relative_to(ROOT)} must contain an object")
    return value


def validate_fixtures() -> None:
    index = load_json("fixture-index.json")
    if index.get("format") != "double-riichi-contract-fixtures-v1":
        fail("fixture index format is not pinned")
    if index.get("validator", {}).get("yaml_dependency") != "PyYAML==6.0.3":
        fail("fixture index does not pin the YAML validator")
    entries = index.get("fixtures")
    if not isinstance(entries, list) or len(entries) != 5:
        fail("fixture index must list exactly five representative fixtures")
    expected = {
        "health-200.json",
        "health-503.json",
        "public-status.json",
        "public-room-lookup.json",
        "human-snapshot.json",
    }
    if {entry.get("file") for entry in entries} != expected:
        fail("fixture index entries do not match the retained fixture set")

    health_fields = {
        "version",
        "commit",
        "uptime_seconds",
        "database",
        "replay_storage",
        "active_rooms",
        "active_room_matches",
        "active_compat_matches",
    }
    for name in ("health-200.json", "health-503.json"):
        value = load_json(name)
        if set(value) != health_fields:
            fail(f"{name} has an uncontracted health field")
        if value["database"] not in {"ok", "degraded", "not_configured"}:
            fail(f"{name} has an invalid database state")
        if value["replay_storage"] not in {"ok", "degraded", "not_configured"}:
            fail(f"{name} has an invalid replay state")
        if not isinstance(value["commit"], str) or len(value["commit"]) != 12:
            fail(f"{name} must use a short commit placeholder")
        for key in health_fields - {"version", "commit", "database", "replay_storage"}:
            if not isinstance(value[key], int) or value[key] < 0:
                fail(f"{name} has an unbounded {key}")

    if load_json("public-status.json") != {"status": "ok", "active_matches": 0}:
        fail("public-status fixture is not the minimum pinned response")

    room = load_json("public-room-lookup.json")
    if set(room) != {
        "room_name",
        "game_mode",
        "phase",
        "join_allowed",
        "participant_count",
        "participant_limit",
    }:
        fail("public-room-lookup fixture contains an uncontracted field")
    if room["phase"] not in {"lobby", "playing", "post_match"}:
        fail("public-room-lookup fixture has an invalid phase")

    snapshot = load_json("human-snapshot.json")
    if snapshot.get("type") != "snapshot" or set(snapshot) != {"type", "room", "state"}:
        fail("human snapshot fixture does not match AsyncAPI")
    if set(snapshot["room"]) != {
        "join_code",
        "room_name",
        "game_mode",
        "phase",
        "revision",
        "participants",
        "match_players",
        "roster",
        "result",
    }:
        fail("human snapshot room contains an uncontracted field")


def validate_contracts() -> None:
    openapi = load_yaml(SPEC / "openapi.yaml")
    asyncapi = load_yaml(SPEC / "asyncapi.yaml")
    if openapi.get("openapi") != "3.1.0":
        fail("OpenAPI version is not 3.1.0")
    if asyncapi.get("asyncapi") != "3.0.0":
        fail("AsyncAPI version is not 3.0.0")
    paths = openapi.get("paths")
    if not isinstance(paths, dict) or "/api/v1/health" not in paths:
        fail("OpenAPI health path is missing")
    if "/mcp" in paths or any(path.startswith("/ws/") for path in paths):
        fail("OpenAPI must not duplicate MCP or Human WebSocket schemas")
    if "adminCookie" not in openapi.get("components", {}).get("securitySchemes", {}):
        fail("OpenAPI admin cookie scheme is missing")
    health = openapi["components"]["schemas"]["Health"]
    if health["properties"]["commit"]["pattern"] != r"^(?:[0-9a-f]{12}|dev[0-9]{9})$":
        fail("OpenAPI commit schema is not short-SHA bounded")

    channels = asyncapi.get("channels", {})
    if "humanRoom" not in channels or "roomMjaiWrapper" not in channels:
        fail("AsyncAPI Human or Room MJAI wrapper channel is missing")
    wrapper = channels["roomMjaiWrapper"]
    if "externalDocs" not in wrapper or "x-upstream-contract" not in wrapper:
        fail("Room MJAI does not retain its upstream pointer")
    if "mcp" in str(asyncapi).lower():
        fail("AsyncAPI must not duplicate MCP schema")

    source = ROUTER_SOURCE.read_text(encoding="utf-8")
    source_paths = set(re.findall(r"\.route\(\s*\"([^\"]+)\"", source))
    contract_paths = set(paths)
    local_paths = {
        path
        for path in source_paths
        if path.startswith("/api/v1/") or path.startswith("/assets/") or path == "/status"
    }
    if local_paths != contract_paths:
        missing = sorted(local_paths - contract_paths)
        extra = sorted(contract_paths - local_paths)
        fail(f"router/OpenAPI path mismatch; missing={missing}, extra={extra}")


def main() -> int:
    validate_contracts()
    validate_fixtures()
    print("contract validation passed: OpenAPI, AsyncAPI, fixtures, and router paths")
    return 0


if __name__ == "__main__":
    sys.exit(main())
