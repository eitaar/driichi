#!/usr/bin/env python3
"""Run the explicitly scheduled-only provisional riichi.dev status probe."""

from __future__ import annotations

import argparse
import json
import os
import sys
from urllib.error import HTTPError, URLError
from urllib.parse import urlparse, urlunparse
from urllib.request import HTTPHandler, HTTPSHandler, Request, build_opener


class LiveCheckError(RuntimeError):
    """The provisional external gate could not be completed."""


def _endpoint(base_url: str) -> str:
    parsed = urlparse(base_url)
    if parsed.scheme not in {"http", "https"} or not parsed.netloc or parsed.query or parsed.fragment:
        raise LiveCheckError("RIICHI_DEV_BASE_URL must be an absolute HTTP(S) URL without query or fragment")
    path = parsed.path.rstrip("/") + "/status"
    return urlunparse((parsed.scheme, parsed.netloc, path, "", "", ""))


def probe(base_url: str, auth_key: str, timeout: float = 20) -> int:
    endpoint = _endpoint(base_url)
    if urlparse(endpoint).scheme not in {"http", "https"}:
        raise LiveCheckError("provisional probe endpoint must use HTTP(S)")
    request = Request(  # noqa: S310 - endpoint is restricted to HTTP(S) above
        endpoint,
        headers={
            "Accept": "application/json",
            "Authorization": f"Bearer {auth_key}",
            "User-Agent": "double-riichi-scheduled-compat/1",
        },
    )
    try:
        opener = build_opener(HTTPHandler(), HTTPSHandler())
        with opener.open(request, timeout=timeout) as response:
            body = response.read(1024 * 1024)
            status = response.status
    except HTTPError as error:
        raise LiveCheckError(f"provisional riichi.dev status probe returned HTTP {error.code}") from error
    except URLError as error:
        raise LiveCheckError(f"provisional riichi.dev status probe failed: {error.reason}") from error
    try:
        value = json.loads(body.decode("utf-8"))
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        raise LiveCheckError("provisional riichi.dev status response was not JSON") from error
    if not 200 <= status < 300:
        raise LiveCheckError(f"provisional riichi.dev status probe returned HTTP {status}")
    if not isinstance(value, dict):
        raise LiveCheckError("provisional riichi.dev status response was not an object")
    return status


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--dry-run", action="store_true", help="show the endpoint without making a request")
    args = parser.parse_args()
    try:
        base_url = os.environ.get("RIICHI_DEV_BASE_URL", "")
        auth_key = os.environ.get("RIICHI_DEV_AUTH_KEY", "")
        if not base_url:
            raise LiveCheckError("RIICHI_DEV_BASE_URL is required")
        endpoint = _endpoint(base_url)
        if args.dry_run:
            print(json.dumps({"method": "GET", "url": endpoint, "authorization": "Bearer <redacted>"}, sort_keys=True))
        else:
            if not auth_key:
                raise LiveCheckError("RIICHI_DEV_AUTH_KEY is required")
            print(f"provisional external gate: GET {endpoint}")
            print(f"provisional external gate: HTTP {probe(base_url, auth_key)}")
    except LiveCheckError as error:
        print(f"live check error: {error}", file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
