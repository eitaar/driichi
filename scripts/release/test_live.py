#!/usr/bin/env python3
"""Run a bounded, scheduled production-Bot compatibility check.

The deterministic release lane never invokes this script.  A live check is
claimed only when an operator supplies both local and riichi.dev endpoints,
their Bot credentials, and the unchanged production Bot command.  The same
command is run against both endpoints; missing evidence fails closed as
unclaimed instead of being reported as compatibility success.
"""

from __future__ import annotations

import argparse
import json
import math
import os
import subprocess
import sys
from collections.abc import Sequence
from urllib.error import HTTPError, URLError
from urllib.parse import urlparse, urlunparse
from urllib.request import HTTPHandler, HTTPSHandler, ProxyHandler, Request, build_opener


DEFAULT_TIMEOUT_SECONDS = 90.0
MAX_TIMEOUT_SECONDS = 120.0
MAX_COMMAND_ITEMS = 32
MAX_COMMAND_ITEM_BYTES = 4096


class LiveCheckError(RuntimeError):
    """The bounded external gate could not be completed or claimed."""


def _endpoint(base_url: str) -> str:
    try:
        parsed = urlparse(base_url)
        has_userinfo = parsed.username is not None or parsed.password is not None or "@" in parsed.netloc
        hostname = parsed.hostname
        port = parsed.port
    except ValueError as error:
        raise LiveCheckError("compatibility endpoint URL is malformed") from error
    if (
        parsed.scheme not in {"http", "https"}
        or not parsed.netloc
        or hostname is None
        or has_userinfo
        or parsed.query
        or parsed.fragment
        or (port is not None and not 1 <= port <= 65535)
    ):
        raise LiveCheckError("compatibility endpoint must be an absolute HTTP(S) URL without userinfo, query, or fragment")
    path = parsed.path.rstrip("/") + "/status"
    return urlunparse((parsed.scheme, parsed.netloc, path, "", "", ""))


def _credential(value: str, label: str) -> str:
    if not value or len(value) > MAX_COMMAND_ITEM_BYTES or any(char.isspace() for char in value):
        raise LiveCheckError(f"{label} is required and must be a bounded opaque credential")
    return value


def _bot_command(value: str) -> list[str]:
    try:
        command = json.loads(value)
    except json.JSONDecodeError as error:
        raise LiveCheckError("RIICHI_PRODUCTION_BOT_COMMAND must be a JSON argv array") from error
    if (
        not isinstance(command, list)
        or not command
        or len(command) > MAX_COMMAND_ITEMS
        or any(not isinstance(item, str) or not item or len(item.encode()) > MAX_COMMAND_ITEM_BYTES for item in command)
    ):
        raise LiveCheckError("RIICHI_PRODUCTION_BOT_COMMAND must be a bounded non-empty JSON argv array")
    return command


def _timeout(value: str | None) -> float:
    if value is None or not value:
        return DEFAULT_TIMEOUT_SECONDS
    try:
        seconds = float(value)
    except ValueError as error:
        raise LiveCheckError("RIICHI_LIVE_TIMEOUT_SECONDS must be a number") from error
    if not math.isfinite(seconds) or not 0 < seconds <= MAX_TIMEOUT_SECONDS:
        raise LiveCheckError(f"RIICHI_LIVE_TIMEOUT_SECONDS must be greater than zero and at most {MAX_TIMEOUT_SECONDS:g}")
    return seconds


def _status_request(endpoint: str, auth_key: str, timeout: float) -> int:
    try:
        parsed = urlparse(endpoint)
        port = parsed.port
    except ValueError as error:
        raise LiveCheckError("compatibility request URL is malformed") from error
    if (
        parsed.scheme not in {"http", "https"}
        or not parsed.netloc
        or parsed.username is not None
        or parsed.password is not None
        or "@" in parsed.netloc
        or (port is not None and not 1 <= port <= 65535)
    ):
        raise LiveCheckError("compatibility request endpoint must use HTTP(S) without userinfo")
    request = Request(  # noqa: S310 - scheme and netloc are restricted above
        endpoint,
        headers={
            "Accept": "application/json",
            "Authorization": f"Bearer {auth_key}",
            "User-Agent": "double-riichi-scheduled-compat/2",
        },
    )
    try:
        opener = build_opener(ProxyHandler({}), HTTPHandler(), HTTPSHandler())
        with opener.open(request, timeout=timeout) as response:
            body = response.read(1024 * 1024)
            status = response.status
    except HTTPError as error:
        raise LiveCheckError(f"compatibility status probe returned HTTP {error.code}") from error
    except URLError as error:
        raise LiveCheckError(f"compatibility status probe failed: {error.reason}") from error
    try:
        value = json.loads(body.decode("utf-8"))
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        raise LiveCheckError("compatibility status response was not JSON") from error
    if not 200 <= status < 300 or not isinstance(value, dict) or value.get("status") != "ok":
        raise LiveCheckError("compatibility status response was not a bounded healthy object")
    return status


def probe(base_url: str, auth_key: str, timeout: float = 20) -> int:
    """Probe one endpoint after validating its URL and credential."""

    endpoint = _endpoint(base_url)
    return _status_request(endpoint, _credential(auth_key, "Bot credential"), timeout)


def _run_production_bot(command: Sequence[str], base_url: str, auth_key: str, timeout: float, label: str) -> None:
    environment = os.environ.copy()
    environment["RIICHI_BOT_BASE_URL"] = base_url.rstrip("/")
    environment["RIICHI_BOT_AUTH_KEY"] = auth_key
    environment["RIICHI_BOT_ENDPOINT"] = _endpoint(base_url)
    # Keep the production Bot's interface to base URL/key only; the same
    # argv is reused, with these values changed between runs.
    environment["RIICHI_BASE_URL"] = base_url.rstrip("/")
    environment["RIICHI_AUTH_KEY"] = auth_key
    environment["RIICHI_DEV_BASE_URL"] = base_url.rstrip("/")
    environment["RIICHI_DEV_AUTH_KEY"] = auth_key
    try:
        result = subprocess.run(
            list(command),
            check=False,
            env=environment,
            stdin=subprocess.DEVNULL,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            timeout=timeout,
        )
    except (OSError, subprocess.TimeoutExpired) as error:
        raise LiveCheckError(f"production Bot {label} run failed or exceeded its bound") from error
    if result.returncode != 0:
        raise LiveCheckError(f"production Bot {label} run exited {result.returncode}")


def run_live(
    local_base_url: str,
    local_auth_key: str,
    riichi_dev_base_url: str,
    riichi_dev_auth_key: str,
    command: Sequence[str],
    timeout: float,
) -> None:
    """Run the unchanged production Bot against local and riichi.dev."""

    local_endpoint = _endpoint(local_base_url)
    external_endpoint = _endpoint(riichi_dev_base_url)
    local_key = _credential(local_auth_key, "RIICHI_LOCAL_AUTH_KEY")
    external_key = _credential(riichi_dev_auth_key, "RIICHI_DEV_AUTH_KEY")
    # Status is a cheap bounded health check; the Bot runs are the evidence.
    _status_request(local_endpoint, local_key, timeout)
    _status_request(external_endpoint, external_key, timeout)
    _run_production_bot(command, local_base_url, local_key, timeout, "local")
    _run_production_bot(command, riichi_dev_base_url, external_key, timeout, "riichi.dev")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--dry-run", action="store_true", help="show the validated external status endpoint without making a request")
    args = parser.parse_args()
    try:
        external_base_url = os.environ.get("RIICHI_DEV_BASE_URL", "")
        if not external_base_url:
            raise LiveCheckError("unclaimed: RIICHI_DEV_BASE_URL is required; external evidence is unavailable")
        external_endpoint = _endpoint(external_base_url)
        if args.dry_run:
            print(json.dumps({"method": "GET", "url": external_endpoint, "authorization": "Bearer <redacted>"}, sort_keys=True))
            return 0

        local_base_url = os.environ.get("RIICHI_LOCAL_BASE_URL", "")
        local_auth_key = os.environ.get("RIICHI_LOCAL_AUTH_KEY", "")
        external_auth_key = os.environ.get("RIICHI_DEV_AUTH_KEY", "")
        command_text = os.environ.get("RIICHI_PRODUCTION_BOT_COMMAND", "")
        if not local_base_url or not local_auth_key or not external_auth_key or not command_text:
            raise LiveCheckError(
                "unclaimed: local/external Bot credentials and RIICHI_PRODUCTION_BOT_COMMAND are required; external evidence is unavailable"
            )
        command = _bot_command(command_text)
        timeout = _timeout(os.environ.get("RIICHI_LIVE_TIMEOUT_SECONDS"))
        print("live compatibility gate: bounded production Bot local-vs-riichi.dev")
        run_live(local_base_url, local_auth_key, external_base_url, external_auth_key, command, timeout)
        print("live compatibility gate passed: production Bot completed against both endpoints")
    except LiveCheckError as error:
        print(f"live check error: {error}", file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
