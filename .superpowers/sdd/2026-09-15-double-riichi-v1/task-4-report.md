# Task 4 report — Decisions, timing, and projection

Status: `DONE`
Commit: `feat(core): enforce decisions timing and visibility` (final commit hash: see `git rev-parse HEAD`)

## Implemented

- Added protocol-neutral `Decision`, `DecisionId`, `ActionId`, complete legal `GameAction` entries, per-seat deterministic defaults, and a single `MatchMachine` decision boundary.
- Added simultaneous response collection: a Pon/Ron/other response does not resolve until every eligible seat responds; compound actions remain complete and canonical.
- Added stale decision, foreign action, already-consumed action, expired, and no-safe-default errors.
- Added Tokio monotonic timing with paused-time coverage, exact Casual turn/response durations (30/10 seconds), Unlimited connected/disconnected Human behavior, five-minute watchdog defaults, Built-in Bot immediate timing, and Temporary Auto reconnect behavior.
- Added canonical `Audience::{Player, Public, ReplayAdmin}` projection types. `TableState` intentionally is not serializable; only audience projections expose JSON serialization. Player views contain only the viewer's hand/legal actions, Public views contain no concealed hands or private legal actions, and ReplayAdmin views contain complete player hands/actions.
- Added engine-backed table snapshots and simultaneous engine application while leaving Room, persistence, HTTP, WebSocket, MJAI, MCP, and frontend behavior unchanged.
- Pinned Tokio at `=1.53.1` and updated `Cargo.lock`.

## TDD evidence

### RED

1. Before Decision/projection implementation:

```text
cargo test -p double_riichi_core --test task4_decisions
# failed to compile: unresolved Decision/DecisionId/DecisionError/DecisionKind/TimeControl imports and missing tokio dependency

cargo test -p double_riichi_core --test task4_projection
# failed to compile: unresolved projection API imports and missing tokio dependency
```

2. Before expiration handling:

```text
cargo test -p double_riichi_core --test task4_decisions expired_decision_rejects_late_action
# FAILED: expected Err(DecisionError::Expired), but late action was accepted
```

3. Before disconnect retiming:

```text
cargo test -p double_riichi_core --test task4_machine unlimited_disconnect_uses_watchdog_then_temporary_auto_until_reconnect
# FAILED: duration was None instead of Some(300s) for a pending Unlimited decision after disconnect
```

### GREEN

```text
cargo test -p double_riichi_core --test task4_decisions
# 5 passed; 0 failed

cargo test -p double_riichi_core --test task4_machine
# 4 passed; 0 failed

cargo test -p double_riichi_core --test task4_projection
# 3 passed; 0 failed

cargo test -p double_riichi_core
# 6 unit + 3 Task 3 integration + 5 Decision + 4 timing + 3 projection tests passed; 0 failed; 0 doc tests
```

## Verification

```text
cargo fmt --all -- --check
# passed

cargo check --workspace
# Finished `dev` profile

cargo test --workspace
# all workspace unit, integration, and doc tests passed

git diff --check
# passed
```

## Files changed

- `Cargo.lock`
- `crates/double_riichi_core/Cargo.toml`
- `crates/double_riichi_core/src/decision.rs`
- `crates/double_riichi_core/src/engine.rs`
- `crates/double_riichi_core/src/lib.rs`
- `crates/double_riichi_core/src/match_machine.rs`
- `crates/double_riichi_core/src/projection.rs`
- `crates/double_riichi_core/tests/task4_decisions.rs`
- `crates/double_riichi_core/tests/task4_machine.rs`
- `crates/double_riichi_core/tests/task4_projection.rs`

## Invariants checked

- One open Decision owns all action IDs and complete actions; IDs from another/closed Decision cannot mutate state.
- Response windows resolve only after every eligible response or deterministic timeout default.
- Timeout defaults are Pass, then tsumogiri, then canonical discard; risky actions are never selected by timeout.
- Casual timing is 30 seconds for turn Decisions and 10 seconds for response Decisions.
- Unlimited timing is no deadline for a connected Human, five-minute watchdog for disconnected Human/MJAI/MCP, and immediate for Built-in Bot/Temporary Auto.
- Temporary Auto begins after a disconnected seat times out and reconnect restores Interactive control.
- Canonical table state has no `Serialize` implementation; JSON is available only through Player/Public/ReplayAdmin projections.
- Public JSON has no concealed `hand` fields, private action lists, or dummy fourth three-player seat; Player JSON exposes only its own hand/actions; ReplayAdmin exposes complete hands/actions.

## Concerns

Task 2's yamai and authenticated riichi.dev evidence remains deferred as previously ruled. No yamai or authenticated riichi.dev adapter was touched, and this report does not claim Design Freeze. No Room/persistence/protocol/frontend behavior was added.

## Review package

After the required commit:

```text
C:/Users/eitab/.pi/agent/git/github.com/obra/superpowers/skills/subagent-driven-development/scripts/review-package docs/superpowers/plans/2026-09-15-double-riichi-v1.md b678a3e58c51f4530df3d3386a3f9ab4cf6458a HEAD
# wrote the workspace review package for the Task 4 commit range
```
