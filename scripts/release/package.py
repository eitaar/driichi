#!/usr/bin/env python3
"""Build and verify deterministic Double Riichi release archives.

The package command consumes already-built binaries and a production frontend
build.  The build command runs the production build in the required order and
keeps generated Starter Pack media in a temporary directory; no generated
binary media is ever written to the repository's ``character-packs/`` path.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import platform as host_platform
import re
import shutil
import subprocess
import sys
import tempfile
import zipfile
from collections.abc import Callable, Iterable
from pathlib import Path, PurePosixPath

ROOT = Path(__file__).resolve().parents[2]
STARTER_PACKS = (
    ("player-red", "human"),
    ("player-blue", "human"),
    ("mjai-bot", "mjai"),
    ("tsumogiri-bot", "builtin"),
    ("mcp-agent", "mcp"),
)
STARTER_VOICES = ("chi", "pon", "kan", "riichi", "ron", "tsumo")
PLATFORM_TARGETS = {
    "linux-x86_64": "x86_64-unknown-linux-gnu",
    "windows-x86_64": "x86_64-pc-windows-msvc",
    "macos-arm64": "aarch64-apple-darwin",
}
SEMVER = re.compile(r"^(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$")
HEX_COMMIT = re.compile(r"^[0-9a-f]{7,64}$")
CHECKSUM = re.compile(r"^([0-9a-f]{64})  ([^\r\n]+)$")


class ReleaseError(RuntimeError):
    """An input or release invariant failed."""


def _require(condition: bool, message: str) -> None:
    if not condition:
        raise ReleaseError(message)


def _safe_relative_name(name: str, label: str = "archive entry") -> str:
    """Return a portable relative POSIX path or fail closed."""

    _require(bool(name) and "\\" not in name and "\x00" not in name, f"{label} uses a non-portable path: {name!r}")
    path = PurePosixPath(name)
    _require(not path.is_absolute() and not name.startswith("/"), f"{label} is absolute: {name!r}")
    _require(not name.partition("/")[0].endswith(":"), f"{label} has a drive prefix: {name!r}")
    _require(all(part not in ("", ".", "..") for part in path.parts), f"{label} escapes its root: {name!r}")
    _require(path.as_posix() == name, f"{label} is not canonical: {name!r}")
    return name


def _read_required_file(path: Path, label: str) -> bytes:
    _require(path.is_file() and not path.is_symlink(), f"missing {label}: {path}")
    return path.read_bytes()


def _hash_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def _hash_file(path: Path) -> str:
    return _hash_bytes(path.read_bytes())


def _parse_checksum_text(
    text: str,
    names: Iterable[str],
    read: Callable[[str], bytes],
    label: str,
) -> None:
    expected_names = {_safe_relative_name(name, f"{label} entry") for name in names}
    seen: set[str] = set()
    for line_number, line in enumerate(text.splitlines(), 1):
        match = CHECKSUM.fullmatch(line)
        _require(match is not None, f"invalid {label} checksum line {line_number}: {line!r}")
        if match is None:
            raise ReleaseError(f"invalid {label} checksum line {line_number}")
        digest, name = match.groups()
        name = _safe_relative_name(name, f"{label} checksum path")
        _require(name != "SHA256SUMS", f"{label} checksum lists itself")
        _require(name in expected_names, f"{label} checksum lists unexpected path: {name}")
        _require(name not in seen, f"duplicate {label} checksum path: {name}")
        _require(_hash_bytes(read(name)) == digest, f"checksum mismatch in {label}: {name}")
        seen.add(name)
    _require(seen == expected_names, f"{label} checksum does not cover the complete file set")


def _manifest(path: Path, expected_id: str, expected_usage: str) -> None:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as error:
        raise ReleaseError(f"invalid Starter manifest: {path}: {error}") from error
    _require(isinstance(value, dict) and set(value) == {"id", "name", "usage"}, f"invalid Starter manifest fields: {path}")
    _require(value["id"] == expected_id and value["usage"] == expected_usage, f"Starter manifest identity mismatch: {path}")
    _require(isinstance(value["name"], str) and value["name"], f"Starter manifest name is empty: {path}")


def _verify_starter_directory(starter: Path) -> str:
    """Validate generated Starter Pack files and return their version."""

    _require(starter.is_dir() and not starter.is_symlink(), f"missing Starter Pack directory: {starter}")
    version_path = starter / "STARTER_VERSION"
    version = _read_required_file(version_path, "Starter version").decode("utf-8").strip()
    _require(bool(SEMVER.fullmatch(version)), f"invalid Starter version: {version!r}")
    notice = _read_required_file(starter / "CC0-NOTICE.txt", "Starter CC0 notice").decode("utf-8")
    checksums = _read_required_file(starter / "SHA256SUMS", "Starter checksums").decode("ascii")

    required: set[str] = {"STARTER_VERSION", "CC0-NOTICE.txt"}
    binary_paths: list[str] = []
    for pack_id, usage in STARTER_PACKS:
        pack = starter / pack_id
        _require(pack.is_dir() and not pack.is_symlink(), f"missing Starter Pack: {pack_id}")
        _manifest(pack / "manifest.json", pack_id, usage)
        _require(bool(_read_required_file(pack / "LICENSE", f"Starter license {pack_id}").strip()), f"empty Starter license: {pack_id}")
        required.update(f"{pack_id}/{name}" for name in ("manifest.json", "LICENSE", "portrait.webp", "icon.webp"))
        for asset in ("portrait.webp", "icon.webp"):
            data = _read_required_file(pack / asset, f"Starter asset {pack_id}/{asset}")
            _require(data[:4] == b"RIFF" and data[8:12] == b"WEBP", f"invalid WebP header: {pack_id}/{asset}")
            binary_paths.append(f"{pack_id}/{asset}")
        for voice in STARTER_VOICES:
            relative = f"{pack_id}/voices/{voice}.ogg"
            data = _read_required_file(starter / relative, f"Starter asset {relative}")
            _require(data[:4] == b"OggS", f"invalid Ogg header: {relative}")
            required.add(relative)
            binary_paths.append(relative)

    files = {
        _safe_relative_name(path.relative_to(starter).as_posix(), "Starter file")
        for path in starter.rglob("*")
        if path.is_file()
    }
    _require("SHA256SUMS" in files, "Starter checksum file is missing")
    _require(files - {"SHA256SUMS"} == required, "Starter Pack contains unexpected or missing files")
    _parse_checksum_text(
        checksums,
        files - {"SHA256SUMS"},
        lambda name: (starter / Path(*name.split("/"))).read_bytes(),
        "Starter Pack",
    )
    for relative in binary_paths:
        _require(relative in notice, f"Starter CC0 notice omits {relative}")
    return version


def _verify_starter_zip(archive: Path, expected_version: str) -> None:
    _require(archive.is_file(), f"missing standalone Starter archive: {archive}")
    with zipfile.ZipFile(archive) as bundle:
        raw_names = bundle.namelist()
        names = [_safe_relative_name(name, "Starter archive entry") for name in raw_names]
        _require(names == raw_names, "Starter archive contains a non-portable path")
        _require(names == sorted(names), "Starter archive entries are not deterministic")
        _require(len(names) == len(set(names)), "Starter archive contains duplicate entries")
        members = {name: bundle.read(name) for name in names}
    _require(members.get("STARTER_VERSION", b"").decode("utf-8").strip() == expected_version, "Starter archive version mismatch")
    required: set[str] = {"STARTER_VERSION", "CC0-NOTICE.txt"}
    binary_paths: list[str] = []
    for pack_id, usage in STARTER_PACKS:
        prefix = f"{pack_id}/"
        manifest_name = prefix + "manifest.json"
        license_name = prefix + "LICENSE"
        _require(manifest_name in members and license_name in members, f"Starter archive omits {pack_id} metadata")
        try:
            manifest = json.loads(members[manifest_name].decode("utf-8"))
        except (UnicodeDecodeError, json.JSONDecodeError) as error:
            raise ReleaseError(f"invalid Starter archive manifest: {manifest_name}") from error
        _require(
            isinstance(manifest, dict)
            and set(manifest) == {"id", "name", "usage"}
            and manifest["id"] == pack_id
            and manifest["usage"] == usage
            and isinstance(manifest["name"], str)
            and bool(manifest["name"]),
            f"invalid Starter archive manifest: {manifest_name}",
        )
        _require(bool(members[license_name].strip()), f"empty Starter archive license: {pack_id}")
        required.update({manifest_name, license_name})
        for asset in ("portrait.webp", "icon.webp"):
            name = prefix + asset
            data = members.get(name, b"")
            _require(data[:4] == b"RIFF" and data[8:12] == b"WEBP", f"invalid Starter archive WebP: {name}")
            required.add(name)
            binary_paths.append(name)
        for voice in STARTER_VOICES:
            name = f"{prefix}voices/{voice}.ogg"
            _require(members.get(name, b"")[:4] == b"OggS", f"invalid Starter archive Ogg: {name}")
            required.add(name)
            binary_paths.append(name)
    _require(set(members) == required | {"SHA256SUMS"}, "Starter archive contains unexpected or missing files")
    _parse_checksum_text(
        members["SHA256SUMS"].decode("ascii"),
        required,
        members.__getitem__,
        "Starter archive",
    )
    notice = members["CC0-NOTICE.txt"].decode("utf-8")
    for name in binary_paths:
        _require(name in notice, f"Starter archive CC0 notice omits {name}")


def _workspace_version(repo_root: Path) -> str:
    text = _read_required_file(repo_root / "Cargo.toml", "workspace manifest").decode("utf-8")
    section = False
    for line in text.splitlines():
        if line.startswith("["):
            section = line.strip() == "[workspace.package]"
        elif section:
            match = re.match(r'\s*version\s*=\s*"([^"]+)"\s*$', line)
            if match:
                version = match.group(1)
                _require(bool(SEMVER.fullmatch(version)), f"invalid workspace version: {version}")
                return version
    raise ReleaseError("workspace version is missing from [workspace.package]")


def _git_commit(repo_root: Path) -> str:
    candidates = [os.environ.get("GIT_COMMIT"), os.environ.get("GITHUB_SHA")]
    for candidate in candidates:
        if candidate and HEX_COMMIT.fullmatch(candidate[:64]):
            return candidate[:64].lower()
    try:
        result = subprocess.run(
            ["git", "rev-parse", "--short=12", "HEAD"],
            cwd=repo_root,
            check=True,
            capture_output=True,
            text=True,
        )
    except (OSError, subprocess.CalledProcessError) as error:
        raise ReleaseError("could not determine the release commit; pass --commit") from error
    commit = result.stdout.strip().lower()
    _require(bool(HEX_COMMIT.fullmatch(commit)), f"git returned an invalid release commit: {commit!r}")
    return commit


def _platform_name(value: str, target: str | None) -> str:
    if value != "auto":
        _require(value in PLATFORM_TARGETS, f"unsupported release platform: {value}")
        return value
    if target in PLATFORM_TARGETS.values():
        return next(name for name, candidate in PLATFORM_TARGETS.items() if candidate == target)
    system = host_platform.system().lower()
    machine = host_platform.machine().lower()
    if system == "linux" and machine in {"x86_64", "amd64"}:
        return "linux-x86_64"
    if system == "windows" and machine in {"x86_64", "amd64"}:
        return "windows-x86_64"
    if system == "darwin" and machine in {"arm64", "aarch64"}:
        return "macos-arm64"
    raise ReleaseError("cannot infer a supported platform; pass --platform")


def _binary_path(repo_root: Path, explicit: str | None, target: str | None, filename: str, platform_name: str) -> Path:
    if explicit:
        return Path(explicit).resolve()
    release_dir = repo_root / "target"
    if target:
        release_dir /= target
    release_dir /= "release"
    suffix = ".exe" if platform_name == "windows-x86_64" else ""
    return (release_dir / f"{filename}{suffix}").resolve()


def _assert_no_generated_media(repo_root: Path) -> None:
    generated_root = repo_root / "character-packs"
    if generated_root.is_dir() and any(path.is_file() for path in generated_root.rglob("*")):
        raise ReleaseError("generated Starter Pack media exists in repository character-packs/; use a temporary staging directory")
    for path in repo_root.glob("character-packs-*.zip"):
        if path.is_file():
            raise ReleaseError(f"generated Starter archive exists in repository root: {path.name}")
    git_dir = repo_root / ".git"
    if not git_dir.exists():
        return
    try:
        tracked = subprocess.run(
            ["git", "ls-files", "-z", "--", "character-packs", "character-packs-*.zip"],
            cwd=repo_root,
            check=True,
            capture_output=True,
        ).stdout
    except (OSError, subprocess.CalledProcessError) as error:
        raise ReleaseError("could not verify that generated Starter media is uncommitted") from error
    _require(not tracked, "generated Starter Pack media is tracked by git")


def _copy_tree(source: Path, destination: Path) -> None:
    for path in sorted(source.rglob("*")):
        relative = path.relative_to(source)
        _safe_relative_name(relative.as_posix(), "staged path")
        target = destination / relative
        if path.is_symlink():
            raise ReleaseError(f"symlink is not allowed in release input: {path}")
        if path.is_dir():
            target.mkdir(parents=True, exist_ok=True)
        else:
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(path, target)


def _write_checksums(root: Path) -> None:
    paths = sorted(
        (path for path in root.rglob("*") if path.is_file() and path != root / "SHA256SUMS"),
        key=lambda path: path.relative_to(root).as_posix(),
    )
    lines = [f"{_hash_file(path)}  {path.relative_to(root).as_posix()}" for path in paths]
    (root / "SHA256SUMS").write_text("\n".join(lines) + "\n", encoding="ascii", newline="\n")


def _create_zip(source: Path, archive: Path) -> None:
    if archive.exists():
        archive.unlink()
    archive.parent.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(archive, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9) as bundle:
        for path in sorted(
            (path for path in source.rglob("*") if path.is_file()),
            key=lambda path: path.relative_to(source).as_posix(),
        ):
            name = _safe_relative_name(path.relative_to(source).as_posix(), "release archive entry")
            info = zipfile.ZipInfo(name, date_time=(1980, 1, 1, 0, 0, 0))
            info.create_system = 3
            mode = 0o755 if name in {"driichi", "driichi-mcp"} else 0o644
            info.external_attr = mode << 16
            info.compress_type = zipfile.ZIP_DEFLATED
            bundle.writestr(info, path.read_bytes())


def _write_archive_checksum(archive: Path) -> Path:
    checksum = archive.with_name(archive.name + ".sha256")
    checksum.write_text(f"{_hash_file(archive)}  {archive.name}\n", encoding="ascii", newline="\n")
    return checksum


def _archive_members(archive: Path) -> dict[str, bytes]:
    with zipfile.ZipFile(archive) as bundle:
        raw_names = bundle.namelist()
        names = [_safe_relative_name(name, "release archive entry") for name in raw_names]
        _require(names == raw_names, "release archive contains a non-portable path")
        _require(names == sorted(names), "release archive entries are not deterministic")
        _require(len(names) == len(set(names)), "release archive contains duplicate entries")
        return {name: bundle.read(name) for name in names}


def _verify_archive(archive: Path, platform_name: str, version: str, commit: str) -> None:
    members = _archive_members(archive)
    suffix = ".exe" if platform_name == "windows-x86_64" else ""
    required = {
        f"driichi{suffix}",
        f"driichi-mcp{suffix}",
        "README",
        "config.toml.example",
        ".env.example",
        "LICENSE-MIT",
        "LICENSE-APACHE",
        "THIRD_PARTY_NOTICES",
        "VERSION",
        "RELEASE-METADATA.json",
        "SHA256SUMS",
        "character-packs/STARTER_VERSION",
        "character-packs/CC0-NOTICE.txt",
        "character-packs/SHA256SUMS",
    }
    _require(required <= members.keys(), f"release archive is missing: {sorted(required - members.keys())}")
    starter_required = {
        "character-packs/STARTER_VERSION",
        "character-packs/CC0-NOTICE.txt",
        "character-packs/SHA256SUMS",
    }
    for pack_id, _ in STARTER_PACKS:
        prefix = f"character-packs/{pack_id}/"
        starter_required.update({prefix + name for name in ("manifest.json", "LICENSE", "portrait.webp", "icon.webp")})
        starter_required.update(f"{prefix}voices/{voice}.ogg" for voice in STARTER_VOICES)
    _require(starter_required <= members.keys(), f"release archive is missing Starter files: {sorted(starter_required - members.keys())}")
    for pack_id, usage in STARTER_PACKS:
        prefix = f"character-packs/{pack_id}/"
        manifest_name = prefix + "manifest.json"
        try:
            manifest = json.loads(members[manifest_name].decode("utf-8"))
        except (UnicodeDecodeError, json.JSONDecodeError) as error:
            raise ReleaseError(f"invalid release Starter manifest: {manifest_name}") from error
        _require(
            isinstance(manifest, dict)
            and set(manifest) == {"id", "name", "usage"}
            and manifest["id"] == pack_id
            and manifest["usage"] == usage
            and isinstance(manifest["name"], str)
            and bool(manifest["name"]),
            f"invalid release Starter manifest: {manifest_name}",
        )
        _require(bool(members[prefix + "LICENSE"].strip()), f"empty release Starter license: {pack_id}")
        for asset in ("portrait.webp", "icon.webp"):
            data = members[prefix + asset]
            _require(data[:4] == b"RIFF" and data[8:12] == b"WEBP", f"invalid release Starter WebP: {prefix + asset}")
        for voice in STARTER_VOICES:
            name = f"{prefix}voices/{voice}.ogg"
            _require(members[name][:4] == b"OggS", f"invalid release Starter Ogg: {name}")
    forbidden = ("config.toml", ".env", "double-riichi.db", "replays/")
    _require(
        not any(
            name == item or (item.endswith("/") and name.startswith(item))
            for name in members
            for item in forbidden
        ),
        "release archive contains runtime data or secrets",
    )
    _require(members["VERSION"].decode("utf-8").strip() == version, "release VERSION metadata mismatch")
    metadata = json.loads(members["RELEASE-METADATA.json"].decode("utf-8"))
    _require(metadata == {
        "application": "double-riichi",
        "commit": commit,
        "platform": platform_name,
        "starter_version": members["character-packs/STARTER_VERSION"].decode("utf-8").strip(),
        "target": PLATFORM_TARGETS[platform_name],
        "version": version,
    }, "release metadata is incomplete or inconsistent")
    _parse_checksum_text(
        members["SHA256SUMS"].decode("ascii"),
        set(members) - {"SHA256SUMS"},
        members.__getitem__,
        "release archive",
    )
    starter_files = {name.removeprefix("character-packs/") for name in members if name.startswith("character-packs/")}
    _require("SHA256SUMS" in starter_files, "archive Starter checksums are missing")
    _parse_checksum_text(
        members["character-packs/SHA256SUMS"].decode("ascii"),
        starter_files - {"SHA256SUMS"},
        lambda name: members[f"character-packs/{name}"],
        "archive Starter Pack",
    )


def _release_archive_name(version: str, platform_name: str) -> str:
    return f"double-riichi-{version}-{platform_name}.zip"


def stage_release(
    *,
    repo_root: Path,
    output: Path,
    platform_name: str,
    target: str,
    version: str,
    commit: str,
    server: Path,
    mcp: Path,
    frontend_dist: Path,
    starter_dir: Path,
    starter_archive: Path | None = None,
) -> Path:
    _assert_no_generated_media(repo_root)
    _require(target == PLATFORM_TARGETS[platform_name], f"target {target!r} does not match {platform_name}")
    _require(bool(SEMVER.fullmatch(version)), f"invalid release version: {version!r}")
    _require(bool(HEX_COMMIT.fullmatch(commit)), f"invalid release commit: {commit!r}")
    for path, label in ((server, "driichi binary"), (mcp, "driichi-mcp binary")):
        _require(path.is_file() and not path.is_symlink() and path.stat().st_size > 0, f"missing {label}: {path}")
    _require((frontend_dist / "index.html").is_file(), f"built frontend is missing index.html: {frontend_dist}")
    starter_version = _verify_starter_directory(starter_dir)
    if starter_archive is not None:
        _verify_starter_zip(starter_archive, starter_version)

    output = output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    archive = output / _release_archive_name(version, platform_name)
    with tempfile.TemporaryDirectory(prefix="driichi-release-", dir=output) as temporary:
        staged = Path(temporary)
        suffix = ".exe" if platform_name == "windows-x86_64" else ""
        shutil.copyfile(server, staged / f"driichi{suffix}")
        shutil.copyfile(mcp, staged / f"driichi-mcp{suffix}")
        for source_name, archive_name in (
            ("README.md", "README"),
            ("LICENSE-MIT", "LICENSE-MIT"),
            ("LICENSE-APACHE", "LICENSE-APACHE"),
            ("THIRD_PARTY_NOTICES", "THIRD_PARTY_NOTICES"),
        ):
            shutil.copyfile(repo_root / "release" / source_name, staged / archive_name)
        shutil.copyfile(repo_root / "config.toml.example", staged / "config.toml.example")
        shutil.copyfile(repo_root / ".env.example", staged / ".env.example")
        (staged / "VERSION").write_text(version + "\n", encoding="ascii", newline="\n")
        (staged / "RELEASE-METADATA.json").write_text(
            json.dumps(
                {
                    "application": "double-riichi",
                    "commit": commit,
                    "platform": platform_name,
                    "starter_version": starter_version,
                    "target": target,
                    "version": version,
                },
                indent=2,
                sort_keys=True,
            )
            + "\n",
            encoding="utf-8",
            newline="\n",
        )
        _copy_tree(starter_dir, staged / "character-packs")
        _write_checksums(staged)
        _create_zip(staged, archive)

    _verify_archive(archive, platform_name, version, commit)
    _write_archive_checksum(archive)
    if starter_archive is not None:
        destination = output / starter_archive.name
        if starter_archive.resolve() != destination.resolve():
            shutil.copyfile(starter_archive, destination)
        _write_archive_checksum(destination)
    return archive


def _run(command: list[str], repo_root: Path) -> None:
    try:
        subprocess.run(command, cwd=repo_root, check=True)
    except OSError as error:
        raise ReleaseError(f"could not run {' '.join(command)}: {error}") from error
    except subprocess.CalledProcessError as error:
        raise ReleaseError(f"release command failed ({error.returncode}): {' '.join(command)}") from error


def _npm_command() -> str:
    return "npm.cmd" if os.name == "nt" else "npm"


def build_release(args: argparse.Namespace) -> Path:
    repo_root = Path(args.repo_root).resolve()
    platform_name = _platform_name(args.platform, args.target)
    target = args.target or PLATFORM_TARGETS[platform_name]
    version = args.version or _workspace_version(repo_root)
    commit = (args.commit or _git_commit(repo_root)).lower()
    _assert_no_generated_media(repo_root)
    frontend = repo_root / "frontend"
    _run([_npm_command(), "ci", "--prefix", str(frontend)], repo_root)
    _run([_npm_command(), "run", "build", "--prefix", str(frontend)], repo_root)
    cargo = ["cargo", "build", "--release", "--locked"]
    if args.target:
        cargo.extend(["--target", args.target])
    _run(cargo, repo_root)
    binary_dir = repo_root / "target"
    if args.target:
        binary_dir /= args.target
    binary_dir /= "release"
    with tempfile.TemporaryDirectory(prefix="driichi-starter-build-") as temporary:
        starter_dir = Path(temporary) / "character-packs"
        starter_archive = Path(temporary) / "driichi-starter-packs.zip"
        starter_generator = repo_root / "scripts" / "generate_starter_packs.py"
        _require(starter_generator.is_file(), f"missing Starter generator: {starter_generator}")
        _run([sys.executable, str(starter_generator), "--output", str(starter_dir)], repo_root)
        # The generator's own ZIP uses host Path ordering; rewrite it with
        # POSIX-relative sorting so Windows and Unix archives are identical.
        _write_checksums(starter_dir)
        _create_zip(starter_dir, starter_archive)
        _write_archive_checksum(starter_archive)
        return stage_release(
            repo_root=repo_root,
            output=Path(args.output),
            platform_name=platform_name,
            target=target,
            version=version,
            commit=commit,
            server=_binary_path(repo_root, args.server, args.target, "driichi", platform_name) if args.server else binary_dir / ("driichi.exe" if platform_name == "windows-x86_64" else "driichi"),
            mcp=_binary_path(repo_root, args.mcp, args.target, "driichi-mcp", platform_name) if args.mcp else binary_dir / ("driichi-mcp.exe" if platform_name == "windows-x86_64" else "driichi-mcp"),
            frontend_dist=repo_root / "frontend" / "dist",
            starter_dir=starter_dir,
            starter_archive=starter_archive,
        )


def _add_common_arguments(parser: argparse.ArgumentParser) -> None:
    parser.add_argument("--repo-root", type=Path, default=ROOT)
    parser.add_argument("--platform", choices=["auto", *PLATFORM_TARGETS], default="auto")
    parser.add_argument("--target")
    parser.add_argument("--version")
    parser.add_argument("--commit")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)

    stage = commands.add_parser("stage", help="stage binaries and create a verified archive")
    _add_common_arguments(stage)
    stage.add_argument("--output", type=Path, required=True)
    stage.add_argument("--server", type=Path, required=True)
    stage.add_argument("--mcp", type=Path, required=True)
    stage.add_argument("--frontend-dist", type=Path, required=True)
    stage.add_argument("--starter-dir", type=Path, required=True)
    stage.add_argument("--starter-archive", type=Path)
    stage.set_defaults(handler="stage")

    build = commands.add_parser("build", help="build frontend/Rust artifacts, generate Starter media, and package")
    _add_common_arguments(build)
    build.add_argument("--output", type=Path, default=ROOT / "target" / "release-artifacts")
    build.add_argument("--server", type=Path)
    build.add_argument("--mcp", type=Path)
    build.set_defaults(handler="build")

    verify = commands.add_parser("verify", help="verify an archive listing and all checksums")
    verify.add_argument("archive", type=Path)
    verify.add_argument("--platform", choices=list(PLATFORM_TARGETS), required=True)
    verify.add_argument("--version", required=True)
    verify.add_argument("--commit", required=True)
    verify.set_defaults(handler="verify")

    args = parser.parse_args()
    try:
        if args.handler == "stage":
            platform_name = _platform_name(args.platform, args.target)
            target = args.target or PLATFORM_TARGETS[platform_name]
            version = args.version or _workspace_version(Path(args.repo_root).resolve())
            commit = (args.commit or _git_commit(Path(args.repo_root).resolve())).lower()
            archive = stage_release(
                repo_root=Path(args.repo_root).resolve(),
                output=args.output,
                platform_name=platform_name,
                target=target,
                version=version,
                commit=commit,
                server=args.server.resolve(),
                mcp=args.mcp.resolve(),
                frontend_dist=args.frontend_dist.resolve(),
                starter_dir=args.starter_dir.resolve(),
                starter_archive=args.starter_archive.resolve() if args.starter_archive else None,
            )
            print(archive)
        elif args.handler == "build":
            print(build_release(args))
        else:
            _verify_archive(args.archive.resolve(), args.platform, args.version, args.commit)
            print(f"verified {args.archive}")
    except (OSError, UnicodeError, ValueError, json.JSONDecodeError, zipfile.BadZipFile, ReleaseError) as error:
        print(f"release error: {error}", file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
