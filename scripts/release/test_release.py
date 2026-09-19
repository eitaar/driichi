#!/usr/bin/env python3
"""Native, dependency-free checks for the release staging scripts."""

from __future__ import annotations

import hashlib
import json
import os
import stat
import sys
import tempfile
import unittest
import zipfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import package  # noqa: E402
import production_frontend  # type: ignore[import-not-found]  # noqa: E402
import smoke  # noqa: E402
import test_live  # noqa: E402


class ReleaseScriptTests(unittest.TestCase):
    def _starter(self, root: Path) -> tuple[Path, Path]:
        starter = root / "starter"
        starter.mkdir()
        (starter / "STARTER_VERSION").write_text("1.0.0\n", encoding="ascii")
        covered: list[str] = []
        for pack_id, usage in package.STARTER_PACKS:
            pack = starter / pack_id / "voices"
            pack.mkdir(parents=True)
            (pack.parent / "manifest.json").write_text(
                json.dumps({"id": pack_id, "name": pack_id, "usage": usage}) + "\n",
                encoding="utf-8",
            )
            (pack.parent / "LICENSE").write_text("CC0\n", encoding="ascii")
            for name, data in (("portrait.webp", b"RIFF0000WEBP"), ("icon.webp", b"RIFF0000WEBP")):
                (pack.parent / name).write_bytes(data)
                covered.append(f"{pack_id}/{name}")
            for voice in package.STARTER_VOICES:
                relative = f"{pack_id}/voices/{voice}.ogg"
                (starter / relative).write_bytes(b"OggS\x00")
                covered.append(relative)
        (starter / "CC0-NOTICE.txt").write_text("\n".join(covered) + "\n", encoding="utf-8")
        package._write_checksums(starter)
        archive = root / "character-packs-1.0.0.zip"
        package._create_zip(starter, archive)
        return starter, archive

    def _repo(self, root: Path) -> Path:
        repo = root / "repo"
        (repo / "release").mkdir(parents=True)
        for name, content in (
            ("README.md", "Double Riichi\n"),
            ("LICENSE-MIT", "MIT\n"),
            ("LICENSE-APACHE", "Apache\n"),
            ("THIRD_PARTY_NOTICES", "Notices\n"),
        ):
            (repo / "release" / name).write_text(content, encoding="utf-8")
        (repo / "config.toml.example").write_text('bind = "127.0.0.1:3000"\n', encoding="utf-8")
        (repo / ".env.example").write_text("ADMIN_USERNAME=admin\n", encoding="ascii")
        (repo / "frontend" / "dist").mkdir(parents=True)
        (repo / "frontend" / "dist" / "index.html").write_text("<!doctype html>\n", encoding="utf-8")
        return repo

    def test_stage_is_deterministic_and_self_verifying(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            repo = self._repo(root)
            starter, starter_archive = self._starter(root)
            server = root / "driichi"
            mcp = root / "driichi-mcp"
            server.write_bytes(b"server")
            mcp.write_bytes(b"mcp")
            output = root / "out"
            def stage() -> Path:
                return package.stage_release(
                    repo_root=repo,
                    output=output,
                    platform_name="linux-x86_64",
                    target=package.PLATFORM_TARGETS["linux-x86_64"],
                    version="1.2.3",
                    commit="abcdef123456",
                    server=server,
                    mcp=mcp,
                    frontend_dist=repo / "frontend" / "dist",
                    starter_dir=starter,
                    starter_archive=starter_archive,
                )
            archive = stage()
            first = archive.read_bytes()
            package._verify_archive(archive, "linux-x86_64", "1.2.3", "abcdef123456")
            smoke._verify_published_checksum(archive)
            stage()
            self.assertEqual(first, archive.read_bytes())
            checksum = archive.with_name(archive.name + ".sha256").read_text(encoding="ascii")
            self.assertEqual(checksum, f"{hashlib.sha256(first).hexdigest()}  {archive.name}\n")
            with zipfile.ZipFile(archive) as bundle:
                self.assertIn("driichi", bundle.namelist())
                self.assertNotIn("config.toml", bundle.namelist())
                info = bundle.getinfo("driichi")
                self.assertEqual(info.date_time, (1980, 1, 1, 0, 0, 0))
                self.assertEqual(info.create_system, 3)
                self.assertEqual(stat.S_IFMT(info.external_attr >> 16), stat.S_IFREG)
                self.assertEqual(info.external_attr >> 16 & 0o777, 0o755)
            with zipfile.ZipFile(starter_archive) as bundle:
                info = bundle.getinfo("STARTER_VERSION")
                self.assertEqual(info.date_time, (1980, 1, 1, 0, 0, 0))
                self.assertEqual(stat.S_IFMT(info.external_attr >> 16), stat.S_IFREG)
                self.assertEqual(info.external_attr >> 16 & 0o777, 0o644)
            with tempfile.TemporaryDirectory() as extracted:
                with zipfile.ZipFile(archive) as bundle:
                    smoke._extract_archive(bundle, Path(extracted))
                if os.name != "nt":
                    self.assertEqual(stat.S_IMODE((Path(extracted) / "driichi").stat().st_mode), 0o755)

    def test_smoke_commands_and_live_endpoint_are_portable(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            commands = smoke.compose_smoke_commands(Path(temporary), "windows-x86_64")
        self.assertTrue(commands["version"][0].endswith("driichi.exe"))
        self.assertEqual(commands["version"][-1], "--version")
        self.assertEqual(test_live._endpoint("https://example.test/base/"), "https://example.test/base/status")
        with self.assertRaises(test_live.LiveCheckError):
            test_live._endpoint("https://example.test/base?secret=1")
        with self.assertRaises(test_live.LiveCheckError):
            test_live._endpoint("https://secret@example.test/base")
        self.assertEqual(test_live._bot_command('["production-bot", "--once"]'), ["production-bot", "--once"])
        with self.assertRaises(test_live.LiveCheckError):
            test_live._bot_command("production-bot")
        with self.assertRaises(test_live.LiveCheckError):
            test_live._timeout("121")

    def test_supported_platforms_match_the_release_contract(self) -> None:
        self.assertEqual(
            package.PLATFORM_TARGETS,
            {
                "linux-x86_64": "x86_64-unknown-linux-gnu",
                "windows-x86_64": "x86_64-pc-windows-msvc",
                "linux-arm64": "aarch64-unknown-linux-gnu",
            },
        )
        self.assertEqual(package._platform_name("auto", "aarch64-unknown-linux-gnu"), "linux-arm64")

    @unittest.skipUnless(hasattr(os, "symlink"), "symlinks are unavailable on this host")
    def test_symlink_inputs_are_rejected_before_read(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            target = root / "target"
            target.write_bytes(b"input")
            link = root / "link"
            link.symlink_to(target)
            with self.assertRaises(package.ReleaseError):
                package._read_required_file(link, "linked input")
            with self.assertRaises(package.ReleaseError):
                package._binary_path(root, str(link), None, "driichi", "linux-x86_64")

    def test_generated_media_is_rejected_from_repository(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "character-packs" / "player-red").mkdir(parents=True)
            (root / "character-packs" / "player-red" / "portrait.webp").write_bytes(b"x")
            with self.assertRaises(package.ReleaseError):
                package._assert_no_generated_media(root)

    def test_production_frontend_gate_restores_and_cleans_dist_safely(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            frontend = Path(temporary) / "frontend"
            frontend.mkdir()
            dist = frontend / "dist"
            dist.mkdir()
            (dist / "old.txt").write_text("old\n", encoding="ascii")
            with production_frontend._fresh_frontend_dist(frontend) as fresh:
                self.assertFalse(fresh.exists())
                fresh.mkdir()
                (fresh / "index.html").write_text("fresh\n", encoding="ascii")
            self.assertEqual((dist / "old.txt").read_text(encoding="ascii"), "old\n")
            dist.rename(frontend / "saved-dist")
            with production_frontend._fresh_frontend_dist(frontend) as fresh:
                fresh.mkdir()
                (fresh / "index.html").write_text("fresh\n", encoding="ascii")
            self.assertFalse((frontend / "dist").exists())
            (frontend / "saved-dist").rename(dist)

    def test_tagged_release_publishes_matching_versioned_artifacts(self) -> None:
        workflow = (package.ROOT / ".github" / "workflows" / "release.yml").read_text(
            encoding="utf-8"
        )
        self.assertIn('if: github.ref_type == \'tag\'', workflow)
        self.assertIn('contents: write', workflow)
        self.assertIn(
            'uses: actions/download-artifact@d3f86a106a0bac45b974a628896c90dbdf5c8093',
            workflow,
        )
        self.assertIn('gh release create "$GITHUB_REF_NAME"', workflow)
        self.assertIn('actual = os.environ["GITHUB_REF_NAME"]', workflow)
        self.assertIn('expected = f"v{version}"', workflow)


if __name__ == "__main__":
    unittest.main()
