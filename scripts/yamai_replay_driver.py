#!/usr/bin/env python3
"""Execute a replay through the pinned yamai ReplayProcessor source tree."""

import argparse
import json
import sys
import types
from pathlib import Path


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--yamai-src", type=Path, required=True)
    parser.add_argument("--replay", type=Path, required=True)
    args = parser.parse_args()

    package_root = (args.yamai_src / "yamai").resolve()
    yamai = types.ModuleType("yamai")
    yamai.__path__ = [str(package_root)]
    yamai.__package__ = "yamai"
    sys.modules["yamai"] = yamai

    from riichienv import RiichiEnv
    from yamai.dataclass import DiscardSample, NewRoundSignal
    from yamai.game_state.replay_processor import ReplayProcessor

    raw = args.replay.read_bytes()
    lines = [json.loads(line) for line in raw.splitlines()]
    if not lines or lines[-1].get("type") != "end_game":
        raise SystemExit("generated replay is not a complete Match")

    outputs = list(ReplayProcessor(RiichiEnv(game_mode="4p-red-east")).process(raw))
    rounds = sum(isinstance(item, NewRoundSignal) for item in outputs)
    discards = sum(isinstance(item, DiscardSample) for item in outputs)
    if rounds == 0 or discards == 0:
        raise SystemExit(f"yamai produced insufficient samples: rounds={rounds}, discards={discards}")

    print(f"yamai accepted {len(lines)} events: rounds={rounds}, discards={discards}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
