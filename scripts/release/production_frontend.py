#!/usr/bin/env python3
"""Build a fresh production frontend and prove release embedding."""

from __future__ import annotations

import argparse
import os
import shutil
import subprocess
import sys
import tempfile
from contextlib import contextmanager
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
TEST_NAME = "embedded_frontend_serves_root_and_referenced_static_asset"


class ProductionFrontendError(RuntimeError):
    """A production frontend gate invariant failed."""


def _remove_path(path: Path) -> None:
    if path.is_symlink():
        path.unlink()
    elif path.is_dir():
        shutil.rmtree(path)
    else:
        path.unlink()


@contextmanager
def _fresh_frontend_dist(frontend: Path):
    """Expose an empty dist path, restoring any prior tree on exit."""

    dist = frontend / "dist"
    if dist.is_symlink() or (dist.exists() and not dist.is_dir()):
        raise ProductionFrontendError(f"frontend/dist must be a regular directory: {dist}")
    had_dist = os.path.lexists(dist)
    with tempfile.TemporaryDirectory(prefix=".driichi-frontend-", dir=frontend) as temporary:
        backup = Path(temporary) / "previous-dist"
        if had_dist:
            dist.rename(backup)
        try:
            yield dist
        finally:
            if os.path.lexists(dist):
                _remove_path(dist)
            if had_dist:
                backup.rename(dist)


def _npm_command() -> str:
    return "npm.cmd" if os.name == "nt" else "npm"


def _run(command: list[str], repo_root: Path, env: dict[str, str] | None = None) -> None:
    print("+", " ".join(command), flush=True)
    subprocess.run(command, cwd=repo_root, env=env, check=True)


def run_gate(repo_root: Path = ROOT) -> None:
    repo_root = repo_root.resolve()
    frontend = repo_root / "frontend"
    if not frontend.is_dir():
        raise ProductionFrontendError(f"frontend source directory is missing: {frontend}")
    with _fresh_frontend_dist(frontend) as dist:
        _run([_npm_command(), "ci", "--prefix", str(frontend)], repo_root)
        _run([_npm_command(), "run", "build", "--prefix", str(frontend)], repo_root)
        if not (dist / "index.html").is_file():
            raise ProductionFrontendError("frontend build did not produce frontend/dist/index.html")
        with tempfile.TemporaryDirectory(prefix="driichi-production-target-") as target:
            env = os.environ.copy()
            env["CARGO_TARGET_DIR"] = target
            _run(
                [
                    "cargo",
                    "--locked",
                    "test",
                    "--release",
                    "-p",
                    "double_riichi_server",
                    "--test",
                    "task16_contracts",
                    TEST_NAME,
                    "--",
                    "--exact",
                    "--test-threads=1",
                ],
                repo_root,
                env,
            )
    print("production frontend embedding gate passed", flush=True)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo-root", type=Path, default=ROOT)
    args = parser.parse_args()
    try:
        run_gate(args.repo_root)
    except (OSError, subprocess.CalledProcessError, ProductionFrontendError) as error:
        print(f"production frontend gate failed: {error}", file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
