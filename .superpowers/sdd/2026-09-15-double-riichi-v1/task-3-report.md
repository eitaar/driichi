# Task 3 report — protocol-neutral MatchMachine

Status: `DONE`
Commit: `feat(core): add protocol-neutral match machine`

## Implemented

- Added protocol-neutral `GameMode`, `Seat`, `Wind`, physical `Tile`/`TileType` (including red fives), `Participant`, `GameAction`, `GameEvent`, `MatchResult`, `MatchStatus`, and `MatchAbort` types.
- Added the sole `double_riichi_core::engine` adapter for pinned `riichienv-core = 0.4.10`.
- Added `MatchMachine` with randomized production seat assignment, test-only deterministic seed construction, legal-action conversion, completion results, and adapter-divergence aborts.
- Kept every `riichienv_core` import and conversion in `src/engine.rs`; no engine-specific type appears in the public neutral API.

## TDD evidence

### RED

Command:

```text
cargo test -p double_riichi_core --test task3_domain
```

Relevant failure before implementation:

```text
error[E0432]: unresolved imports `double_riichi_core::GameAction`, `double_riichi_core::GameEvent`, `double_riichi_core::GameMode`, `double_riichi_core::Participant`, `double_riichi_core::ParticipantKind`, `double_riichi_core::Seat`, `double_riichi_core::Tile`
```

The focused public-domain test failed because the requested neutral API did not exist.

### GREEN

Command:

```text
cargo test -p double_riichi_core
```

Output: `2` unit tests, `3` integration tests, and `0` doc tests passed.

The unit completion test uses the internal `new_with_seed` constructor and completes all four modes: `4p-red-east`, `4p-red-half`, `3p-red-east`, and `3p-red-half`. The divergence test injects an unsupported engine event, verifies adapter failure, and verifies `MatchStatus::Aborted` with no result.

## Observed pinned-engine mapping

Verified source/probe inputs:

- Crate: `riichienv-core 0.4.10`
- Upstream repository: `https://github.com/smly/RiichiEnv`
- VCS revision: `479c1faeb33d082965eef8198f63261a79c0fce3`
- Crate archive SHA-256 from the preserved contract probe: `02e75331a82f9ee0a3c1401f5fe75f210e905063b095d0748fd13e41ce3cd56d`
- Source API: `GameStateVariant::new(game_mode, skip_mjai_logging, seed: Option<u64>, round_wind, GameRule)`, `Observation::legal_actions_method`, `Observation3P::legal_actions_method`, `GameState::step`, and `GameState3P::step`.
- Engine red-five IDs: `16` (`5m`), `52` (`5p`), and `88` (`5s`).
- Engine modes: 4-player East `1`, 4-player Half `2`, 3-player East `4`, and 3-player Half `5`.

Preserved deterministic probe output for fixed seed `0xD0`:

```text
4p-red-east digest=d91ca957d08a5e41f1bb0d8f902bf2fe4b99c66575894e27892b06d40ad02faa steps=700 actions=702 red_candidates=85 events={"chi", "dahai", "end_game", "end_kyoku", "pon", "ryukyoku", "start_game", "start_kyoku", "tsumo"} action_types={"Ankan", "Chi", "Discard", "Kakan", "Pass", "Pon"}
4p-red-half digest=b7f6b76d758d000a85ec50a6c85b3ced2d3a4b8f928314f61a24ad2971f7645f steps=882 actions=884 red_candidates=108 events={"chi", "dahai", "end_game", "end_kyoku", "pon", "ryukyoku", "start_game", "start_kyoku", "tsumo"} action_types={"Ankan", "Chi", "Discard", "Kakan", "Pass", "Pon"}
3p-red-east digest=dff0935d2655ea7fe50cbb8661d9a7842de56654e9f62aed401380db9a3692aa steps=364 actions=364 red_candidates=79 events={"dahai", "end_game", "end_kyoku", "pon", "ryukyoku", "start_game", "start_kyoku", "tsumo"} action_types={"Daiminkan", "Discard", "Kakan", "Kita", "KyushuKyuhai", "Pass", "Pon", "Riichi"}
3p-red-half digest=c88019abb046995ad01643938d7f110abf645be450b1dfd60d2773673c8d33b9 steps=551 actions=551 red_candidates=113 events={"dahai", "end_game", "end_kyoku", "pon", "ryukyoku", "start_game", "start_kyoku", "tsumo"} action_types={"Daiminkan", "Discard", "Kakan", "Kita", "KyushuKyuhai", "Pass", "Pon", "Riichi"}
```

The repeat run produced the same four digests. The adapter also maps the pinned source's complete `ActionType` union (`Discard`, `Chi`, `Pon`, `Daiminkan`, `Ron`, `Riichi`, `Tsumo`, `Pass`, `Ankan`, `Kakan`, `KyushuKyuhai`, `Kita`) to neutral complete actions, expanding the engine's two-step Riichi marker into `RiichiDiscard` and mapping `Kita` to neutral `Nuki`. The source `MjaiEvent` union is mapped to neutral `GameEvent` variants for `start_game`, `start_kyoku`, `tsumo`, `dahai`, `pon`, `chi`, `kan`/`Daiminkan`, `kakan`, `ankan`, `dora`, `reach`, `reach_accepted`, `hora`, `ryukyoku`, `kita`, `end_kyoku`, and `end_game`; an unknown event aborts the Match.

## Files changed

- `Cargo.lock`
- `crates/double_riichi_core/Cargo.toml`
- `crates/double_riichi_core/src/lib.rs`
- `crates/double_riichi_core/src/domain.rs`
- `crates/double_riichi_core/src/engine.rs`
- `crates/double_riichi_core/src/match_machine.rs`
- `crates/double_riichi_core/tests/task3_domain.rs`
- `.superpowers/sdd/2026-09-15-double-riichi-v1/task-3-report.md`

## Seed visibility proof

The only production constructor is `MatchMachine::new(mode, participants)`, which obtains its seed internally from `rand::random`. The deterministic `MatchMachine::new_with_seed` exists only under `#[cfg(test)]` and is private. No public seed field, constructor, or setter exists; the engine adapter and seed-bearing constructors are crate-private/private.

## Validation commands

```text
cargo fmt --all -- --check
# passed

cargo check --workspace
# Finished `dev` profile [unoptimized + debuginfo]

cargo test -p double_riichi_core --test task3_domain
# 3 passed; 0 failed

cargo test -p double_riichi_core match_machine::tests::all_four_presets_complete_with_a_test_only_seed -- --nocapture
# 1 passed; 0 failed

cargo test -p double_riichi_core match_machine::tests::adapter_divergence_aborts_without_a_result -- --nocapture
# 1 passed; 0 failed

cargo test --workspace
# all workspace unit, integration, and doc tests passed

git diff --check
# passed
```

`cargo clippy` was not run because the pinned Rust toolchain does not have the `clippy` component installed; it is not required by the Task 3 command list.

## Concerns

- The pinned engine exposes final scores rather than a separate rank field. `MatchResult` derives rank from descending final scores with seat-order tie breaking while leaving scoring and rule resolution to the engine.
- No MJAI, Replay, Room, MCP, HTTP, or production Frontend code was changed.
