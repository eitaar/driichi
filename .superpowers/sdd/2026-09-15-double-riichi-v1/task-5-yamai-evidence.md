# Task 5 yamai evidence

## Authoritative source

Owner-supplied repository: <https://github.com/eitaar/yamai>

Pinned revision: `226cb84d917376d7513fbfdf987cc6a2294767cc`
(`main`/`HEAD` when supplied).

Primary-source findings:

- `src/yamai/game_state/replay_processor.py` defines
  `ReplayProcessor(RiichiEnv).process(events: bytes)` and applies every decoded
  MJAI event to `RiichiEnv`: <https://github.com/eitaar/yamai/blob/226cb84d917376d7513fbfdf987cc6a2294767cc/src/yamai/game_state/replay_processor.py>.
- `pyproject.toml` requires Python `>=3.14`, NumPy `>=2.5.1`, and riichienv
  `>=0.4.8`: <https://github.com/eitaar/yamai/blob/226cb84d917376d7513fbfdf987cc6a2294767cc/pyproject.toml>.
- The upstream lock resolves NumPy `2.5.3` and riichienv `0.4.10`:
  <https://github.com/eitaar/yamai/blob/226cb84d917376d7513fbfdf987cc6a2294767cc/uv.lock>.
- The pinned `ReplayProcessor` SHA-256 is
  `b8b99cc8fcc17f09330d640801e7575bea7686a4d3d5f553b50d708402d184e3`.
- The pinned repository has no project-level license file or package license
  metadata. Its only license-named file is `misc/mnist/LICENCE.md`, which applies
  to that third-party artifact, not yamai. The gate therefore does not vendor,
  modify, package, or redistribute yamai.

The immutable values are recorded in `spec/external-contracts.toml`.

## Executable gate

`python scripts/test_yamai.py`:

1. Generates a complete four-player East Match from `MatchMachine` through the
   existing Task 5 integration test.
2. Fetches the pinned yamai commit into `.cache/yamai/<sha>` and verifies the
   checked-out SHA and `ReplayProcessor` SHA-256.
3. Creates an isolated temporary Python environment with the upstream-locked
   NumPy and riichienv versions.
4. Loads the exact pinned `ReplayProcessor` module without yamai's unrelated GPU
   model package initializer.
5. Processes the complete generated MJSON and fails unless yamai reaches
   `end_game` input with both round and discard samples.

Observed result:

```text
generated_four_player_match_events_are_valid_canonical_mjson_and_reconstructable ... ok
yamai accepted 1146 events: rounds=8, discards=560
yamai ReplayProcessor accepted the generated replay at 226cb84d917376d7513fbfdf987cc6a2294767cc
```

The command passed twice: once after the initial fetch and once from the pinned
cache. This closes the executable Task 5 yamai ReplayProcessor gate. It does not
supply the still-missing authenticated/immutable riichi.dev evidence.
