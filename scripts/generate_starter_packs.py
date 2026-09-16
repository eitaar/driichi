#!/usr/bin/env python3
"""Generate the release-only, project-owned CC0 Character Starter Packs.

The output is deliberately not checked in. Python creates deterministic pixel
sources and ffmpeg creates the WebP/Ogg files used by a release archive.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import shutil
import subprocess
import tempfile
import zipfile
from pathlib import Path

STARTER_VERSION = "1.0.0"

PACKS = (
    ("player-red", "Player Red", "human", (220, 70, 70)),
    ("player-blue", "Player Blue", "human", (70, 120, 220)),
    ("mjai-bot", "MJAI Bot", "mjai", (75, 175, 95)),
    ("tsumogiri-bot", "Tsumogiri Bot", "builtin", (145, 145, 145)),
    ("mcp-agent", "MCP Agent", "mcp", (160, 90, 205)),
)
VOICES = ("chi", "pon", "kan", "riichi", "ron", "tsumo")

CC0_LICENSE = Path(__file__).with_name("CC0-1.0.txt").read_text(encoding="utf-8")


def run(command: list[str]) -> None:
    subprocess.run(command, check=True, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)


def write_ppm(path: Path, width: int, height: int, color: tuple[int, int, int]) -> None:
    row = bytes(color) * width
    with path.open("wb") as output:
        output.write(f"P6\n{width} {height}\n255\n".encode("ascii"))
        for _ in range(height):
            output.write(row)


def make_image(output: Path, color: tuple[int, int, int], width: int, height: int) -> None:
    with tempfile.TemporaryDirectory(prefix="driichi-starter-") as temporary:
        source = Path(temporary) / "source.ppm"
        write_ppm(source, width, height, color)
        run(
            [
                "ffmpeg",
                "-hide_banner",
                "-loglevel",
                "error",
                "-y",
                "-fflags",
                "+bitexact",
                "-i",
                str(source),
                "-frames:v",
                "1",
                "-an",
                "-c:v",
                "libwebp",
                "-lossless",
                "1",
                "-map_metadata",
                "-1",
                str(output),
            ]
        )


def ogg_crc(page: bytes) -> int:
    crc = 0
    for byte in page:
        crc ^= byte << 24
        for _ in range(8):
            crc = ((crc << 1) ^ 0x04C11DB7) & 0xFFFFFFFF if crc & 0x80000000 else (crc << 1) & 0xFFFFFFFF
    return crc


def canonicalize_ogg(path: Path) -> None:
    data = bytearray(path.read_bytes())
    offset = 0
    while offset + 27 <= len(data) and data[offset : offset + 4] == b"OggS":
        segment_count = data[offset + 26]
        page_size = 27 + segment_count + sum(data[offset + 27 : offset + 27 + segment_count])
        end = offset + page_size
        if end > len(data):
            raise RuntimeError("ffmpeg produced a truncated Ogg page")
        data[offset + 14 : offset + 18] = b"\0\0\0\0"
        data[offset + 22 : offset + 26] = b"\0\0\0\0"
        checksum = ogg_crc(data[offset:end])
        data[offset + 22 : offset + 26] = checksum.to_bytes(4, "little")
        offset = end
    if offset != len(data):
        raise RuntimeError("ffmpeg produced an invalid Ogg stream")
    path.write_bytes(data)


def make_voice(output: Path) -> None:
    run(
        [
            "ffmpeg",
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-fflags",
            "+bitexact",
            "-f",
            "lavfi",
            "-i",
            "anullsrc=r=8000:cl=mono",
            "-t",
            "0.25",
            "-vn",
            "-c:a",
            "libvorbis",
            "-q:a",
            "1",
            "-map_metadata",
            "-1",
            "-serial_offset",
            "1",
            str(output),
        ]
    )
    canonicalize_ogg(output)


def write_checksums(output: Path) -> None:
    entries: list[str] = []
    for path in sorted(path for path in output.rglob("*") if path.is_file() and path.name != "SHA256SUMS"):
        digest = hashlib.sha256(path.read_bytes()).hexdigest()
        entries.append(f"{digest}  {path.relative_to(output).as_posix()}")
    (output / "SHA256SUMS").write_text("\n".join(entries) + "\n", encoding="utf-8", newline="\n")


def write_zip(output: Path, archive: Path) -> None:
    with zipfile.ZipFile(archive, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9) as bundle:
        for path in sorted(path for path in output.rglob("*") if path.is_file()):
            relative = path.relative_to(output).as_posix()
            info = zipfile.ZipInfo(relative, date_time=(1980, 1, 1, 0, 0, 0))
            info.create_system = 3
            info.compress_type = zipfile.ZIP_DEFLATED
            info.external_attr = 0o644 << 16
            bundle.writestr(info, path.read_bytes())


def write_archive_checksum(archive: Path) -> None:
    digest = hashlib.sha256(archive.read_bytes()).hexdigest()
    archive.with_name(archive.name + ".sha256").write_text(
        f"{digest}  {archive.name}\n", encoding="ascii", newline="\n"
    )


def generate(output: Path, archive: Path | None) -> None:
    output.mkdir(parents=True, exist_ok=True)
    for child in output.iterdir():
        if child.is_dir():
            shutil.rmtree(child)
        else:
            child.unlink()

    (output / "STARTER_VERSION").write_text(STARTER_VERSION + "\n", encoding="utf-8", newline="\n")
    for pack_id, name, usage, color in PACKS:
        pack = output / pack_id
        voices = pack / "voices"
        voices.mkdir(parents=True)
        (pack / "manifest.json").write_text(
            json.dumps({"id": pack_id, "name": name, "usage": usage}, ensure_ascii=False, separators=(",", ":")) + "\n",
            encoding="utf-8",
            newline="\n",
        )
        (pack / "LICENSE").write_text(CC0_LICENSE, encoding="utf-8", newline="\n")
        make_image(pack / "portrait.webp", color, 512, 512)
        make_image(pack / "icon.webp", color, 128, 128)
        for voice in VOICES:
            make_voice(voices / f"{voice}.ogg")

    covered = [f"{pack_id}/{asset}" for pack_id, _, _, _ in PACKS for asset in ("portrait.webp", "icon.webp", *(f"voices/{voice}.ogg" for voice in VOICES))]
    notice = (
        "CC0-1.0 coverage for generated Starter assets\n\n"
        "The following project-owned generated binary files are dedicated to the public domain under CC0 1.0 Universal:\n"
        + "\n".join(f"character-packs/{path}" for path in covered)
        + "\n\nReal Characters, trademarks, tile art, and third-party assets are not included in this dedication.\n"
    )
    (output / "CC0-NOTICE.txt").write_text(notice, encoding="utf-8", newline="\n")
    write_checksums(output)
    if archive is not None:
        archive.parent.mkdir(parents=True, exist_ok=True)
        if archive.exists():
            archive.unlink()
        write_zip(output, archive)
        write_archive_checksum(archive)


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--zip", type=Path)
    args = parser.parse_args()
    generate(args.output, args.zip)


if __name__ == "__main__":
    main()
