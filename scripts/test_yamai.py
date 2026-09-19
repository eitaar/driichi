#!/usr/bin/env python3
"""Generate a driichi replay and run it through pinned yamai."""

import hashlib
import os
import subprocess
import sys
import tempfile
import tomllib
from pathlib import Path


def run(command: list[str], *, cwd: Path, env: dict[str, str] | None = None) -> None:
    subprocess.run(command, cwd=cwd, env=env, check=True)


def output(command: list[str], *, cwd: Path) -> str:
    return subprocess.check_output(command, cwd=cwd, text=True).strip()


def yamai_checkout(root: Path, contract: dict[str, str]) -> Path:
    revision = contract["revision"]
    checkout = root / ".cache/yamai" / revision
    if not checkout.exists():
        checkout.parent.mkdir(parents=True, exist_ok=True)
        run(["git", "init", "--quiet", str(checkout)], cwd=root)
        run(["git", "remote", "add", "origin", contract["repository"]], cwd=checkout)
        run(["git", "fetch", "--quiet", "--depth", "1", "origin", revision], cwd=checkout)
        run(["git", "checkout", "--quiet", "--detach", "FETCH_HEAD"], cwd=checkout)
    actual_revision = output(["git", "rev-parse", "HEAD"], cwd=checkout)
    if actual_revision != revision:
        raise SystemExit(f"yamai cache revision mismatch: {actual_revision}")
    if output(["git", "status", "--porcelain"], cwd=checkout):
        raise SystemExit("yamai cache has local modifications")
    return checkout


def main() -> int:
    root = Path(__file__).resolve().parents[1]
    if sys.version_info < (3, 14):
        raise SystemExit("yamai requires Python 3.14 or newer")
    with (root / "spec/external-contracts.toml").open("rb") as contracts:
        contract = tomllib.load(contracts)["yamai"]
    checkout = yamai_checkout(root, contract)
    with tempfile.TemporaryDirectory(prefix="driichi-yamai-replay-") as temporary:
        temp = Path(temporary)
        replay = temp / "generated-4p.mjson"
        env = os.environ.copy()
        env["DRIICHI_YAMAI_REPLAY_OUTPUT"] = str(replay)
        run(
            [
                "cargo",
                "test",
                "--locked",
                "-p",
                "double_riichi_replay",
                "--test",
                "task5_replay",
                "generated_four_player_match_events_are_valid_canonical_mjson_and_reconstructable",
                "--",
                "--exact",
            ],
            cwd=root,
            env=env,
        )
        if not replay.is_file():
            raise SystemExit("replay generator did not produce MJSON")

        processor = checkout / contract["replay_processor"]
        digest = hashlib.sha256(processor.read_bytes()).hexdigest()
        if digest != contract["replay_processor_sha256"]:
            raise SystemExit(f"ReplayProcessor checksum mismatch: {digest}")

        environment = temp / "venv"
        run([sys.executable, "-m", "venv", str(environment)], cwd=root)
        python = environment / ("Scripts/python.exe" if os.name == "nt" else "bin/python")
        run(
            [
                str(python),
                "-m",
                "pip",
                "install",
                "--disable-pip-version-check",
                "--no-input",
                f"numpy=={contract['numpy']}",
                f"riichienv=={contract['riichienv']}",
            ],
            cwd=root,
        )
        run(
            [
                str(python),
                str(root / "scripts/yamai_replay_driver.py"),
                "--yamai-src",
                str(checkout / "src"),
                "--replay",
                str(replay),
            ],
            cwd=root,
        )

    print(f"yamai ReplayProcessor accepted the generated replay at {contract['revision']}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
