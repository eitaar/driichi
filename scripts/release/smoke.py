#!/usr/bin/env python3
"""Run release smoke checks or print them without foreign-platform binaries."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import subprocess
import sys
import tempfile
import time
import zipfile
from pathlib import Path

try:
    from package import HEX_COMMIT, PLATFORM_TARGETS, ReleaseError, _verify_archive
except ImportError:  # pragma: no cover - supports ``python -m scripts.release.smoke``
    from scripts.release.package import (
        HEX_COMMIT,
        PLATFORM_TARGETS,
        ReleaseError,
        _verify_archive,
    )


VERSION_LINE = re.compile(r"^driichi ([0-9]+\.[0-9]+\.[0-9]+(?:[-+][0-9A-Za-z.-]+)?) \(([0-9a-f]{7,64})\)$")
ARCHIVE_CHECKSUM = re.compile(r"^([0-9a-f]{64})  ([^\r\n]+)$")


def _binary_names(platform_name: str) -> tuple[str, str]:
    suffix = ".exe" if platform_name == "windows-x86_64" else ""
    return f"driichi{suffix}", f"driichi-mcp{suffix}"


def compose_smoke_commands(directory: Path, platform_name: str) -> dict[str, list[str]]:
    """Compose argv arrays; no shell parsing is used for portability."""

    server, mcp = _binary_names(platform_name)
    return {
        "version": [str(directory / server), "--version"],
        "start": [str(directory / server), "--config", str(directory / "config.toml")],
        "mcp": [str(directory / mcp), "--server", "http://127.0.0.1:3000/mcp"],
    }


def _run_version(directory: Path, version: str, commit: str, platform_name: str) -> None:
    command = compose_smoke_commands(directory, platform_name)["version"]
    try:
        result = subprocess.run(command, cwd=directory, check=False, capture_output=True, text=True, timeout=20)
    except (OSError, subprocess.TimeoutExpired) as error:
        raise ReleaseError(f"version smoke failed: {error}") from error
    _require(result.returncode == 0, f"version smoke exited {result.returncode}: {result.stderr.strip()}")
    match = VERSION_LINE.fullmatch(result.stdout.strip())
    _require(match is not None, f"unexpected --version output: {result.stdout.strip()!r}")
    if match is not None:
        _require(match.group(1) == version, "--version semver does not match VERSION metadata")
        _require(bool(HEX_COMMIT.fullmatch(match.group(2))), "--version commit metadata is not hexadecimal")
        expected_commit = commit.lower()[:12]
        _require(match.group(2) == expected_commit, "--version commit does not match release metadata")


def _prepare_config(directory: Path) -> None:
    source = directory / "config.toml.example"
    _require(source.is_file(), "config.toml.example is missing from the extracted archive")
    text = source.read_text(encoding="utf-8")
    text = re.sub(r"^bind\s*=.*$", 'bind = "127.0.0.1:0"', text, flags=re.MULTILINE)
    text = re.sub(r"^public_origin\s*=.*$", 'public_origin = "http://127.0.0.1:0"', text, flags=re.MULTILINE)
    (directory / "config.toml").write_text(text, encoding="utf-8", newline="\n")


def _prepare_secret(directory: Path, server: Path) -> None:
    try:
        result = subprocess.run(
            [str(server), "hash-password"],
            cwd=directory,
            input="release-smoke-password\nrelease-smoke-password\n",
            check=False,
            capture_output=True,
            text=True,
            timeout=120,
        )
    except (OSError, subprocess.TimeoutExpired) as error:
        raise ReleaseError(f"password hash smoke failed: {error}") from error
    _require(result.returncode == 0, f"password hash smoke exited {result.returncode}: {result.stderr.strip()}")
    hashes = [line.strip() for line in result.stdout.splitlines() if line.strip().startswith("$argon2")]
    _require(len(hashes) == 1, "password hash smoke did not return one PHC hash")
    (directory / ".env").write_text(
        f"ADMIN_USERNAME=smoke\nADMIN_PASSWORD_HASH={hashes[0]}\n",
        encoding="utf-8",
        newline="\n",
    )


def _run_start(directory: Path, platform_name: str) -> None:
    server_name, _ = _binary_names(platform_name)
    server = directory / server_name
    _prepare_config(directory)
    _prepare_secret(directory, server)
    command = compose_smoke_commands(directory, platform_name)["start"]
    try:
        process = subprocess.Popen(command, cwd=directory, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    except OSError as error:
        raise ReleaseError(f"start smoke could not launch server: {error}") from error
    try:
        deadline = time.monotonic() + 15
        while time.monotonic() < deadline:
            if process.poll() is not None:
                stderr = process.stderr.read() if process.stderr is not None else ""
                raise ReleaseError(f"start smoke exited {process.returncode}: {stderr.strip()}")
            time.sleep(0.1)
    finally:
        if process.poll() is None:
            process.terminate()
        try:
            process.wait(timeout=10)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait(timeout=10)


def _require(condition: bool, message: str) -> None:
    if not condition:
        raise ReleaseError(message)


def _verify_published_checksum(archive: Path) -> None:
    checksum = archive.with_name(archive.name + ".sha256")
    _require(checksum.is_file(), f"missing archive checksum: {checksum}")
    lines = checksum.read_text(encoding="ascii").splitlines()
    _require(len(lines) == 1, f"invalid archive checksum file: {checksum}")
    match = ARCHIVE_CHECKSUM.fullmatch(lines[0])
    _require(match is not None and match.group(2) == archive.name, f"invalid archive checksum entry: {checksum}")
    if match is not None:
        _require(match.group(1) == hashlib.sha256(archive.read_bytes()).hexdigest(), f"archive checksum mismatch: {archive}")


def _dry_run(archive: Path, platform_name: str, version: str, commit: str) -> None:
    _verify_published_checksum(archive)
    _verify_archive(archive, platform_name, version, commit)
    with tempfile.TemporaryDirectory(prefix="driichi-smoke-plan-") as temporary:
        directory = Path(temporary)
        with zipfile.ZipFile(archive) as bundle:
            bundle.extractall(directory)
        print(json.dumps(compose_smoke_commands(directory, platform_name), indent=2, sort_keys=True))


def _mark_binaries_executable(directory: Path, platform_name: str) -> None:
    if platform_name == "windows-x86_64":
        return
    for name in _binary_names(platform_name):
        binary = directory / name
        binary.chmod(binary.stat().st_mode | 0o111)


def _run_archive(archive: Path, platform_name: str, version: str, commit: str) -> None:
    _verify_published_checksum(archive)
    _verify_archive(archive, platform_name, version, commit)
    with tempfile.TemporaryDirectory(prefix="driichi-smoke-") as temporary:
        directory = Path(temporary)
        with zipfile.ZipFile(archive) as bundle:
            bundle.extractall(directory)
        _mark_binaries_executable(directory, platform_name)
        _run_version(directory, version, commit, platform_name)
        _run_start(directory, platform_name)
        print(f"version/start smoke passed for {archive}")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("archive", type=Path)
    parser.add_argument("--platform", choices=list(PLATFORM_TARGETS), required=True)
    parser.add_argument("--version", required=True)
    parser.add_argument("--commit", required=True)
    parser.add_argument("--dry-run", action="store_true", help="verify and print argv without executing archive binaries")
    args = parser.parse_args()
    try:
        if args.dry_run:
            _dry_run(args.archive.resolve(), args.platform, args.version, args.commit)
        else:
            _run_archive(args.archive.resolve(), args.platform, args.version, args.commit)
    except (OSError, UnicodeError, ValueError, zipfile.BadZipFile, ReleaseError) as error:
        print(f"smoke error: {error}", file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
