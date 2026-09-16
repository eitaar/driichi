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

## Review round 1/5 follow-up

Follow-up commit: `d28d3b7` — `fix(core): close task 4 visibility and timing gaps` (final amended hash: see `git rev-parse HEAD`)

Addressed all findings:

- Public and opponent Player projections now redact closed meld tile identities while retaining full contents for the owning Player and ReplayAdmin. New JSON fixtures cover both three-player and four-player modes.
- Unlimited Decisions now keep timing per eligible seat. Connected Humans retain no deadlines even when mixed with Built-in Bot or watchdog seats; automatic seats still timeout independently. Mixed roster coverage is included.
- Reconnecting Temporary Auto preserves the open Decision, other seats' accepted responses, and the original ephemeral IDs while retiming only the reconnected seat.
- Added a three-player Player JSON assertion and an actual paused-time ten-second response expiry test.

### Follow-up RED

```text
cargo test -p double_riichi_core --test task4_projection closed_meld_tiles_are_redacted_from_public_and_opponent_players_in_both_modes
# FAILED: public meld tiles were [100, 101, 102, 103] instead of []

cargo test -p double_riichi_core --test task4_decisions mixed_unlimited_decision_keeps_connected_human_open_while_auto_times_out
# failed to compile: Decision::new_with_timings was not implemented

cargo test -p double_riichi_core --lib reconnect_preserves_pending_decision_responses -- --nocapture
# FAILED: current decision was d1 instead of pending after reconnect
```

### Follow-up GREEN

```text
cargo test -p double_riichi_core --test task4_projection closed_meld_tiles_are_redacted_from_public_and_opponent_players_in_both_modes
# 1 passed; 0 failed

cargo test -p double_riichi_core --test task4_decisions mixed_unlimited_decision_keeps_connected_human_open_while_auto_times_out
# 1 passed; 0 failed

cargo test -p double_riichi_core --test task4_decisions response_decision_expires_after_exactly_ten_seconds
# 1 passed; 0 failed

cargo test -p double_riichi_core --lib reconnect_preserves_pending_decision_responses -- --nocapture
# 1 passed; 0 failed

cargo fmt --all -- --check
# passed

cargo check --workspace
# Finished `dev` profile

cargo test --workspace
# all workspace unit, integration, and doc tests passed

git diff --check
# passed
```

## Review round 2/5 follow-up

Follow-up commit: `fix(core): preserve partial timeout state` (final hash: see `git rev-parse HEAD`).

The partial per-seat timeout state is now stored on each Decision entry. A watchdog/zero-duration entry that defaults while a connected Human remains pending stays marked `timed_out` through the final resolution. `MatchMachine::resolve_expired` also promotes a disconnected Interactive seat to TemporaryAuto as soon as its partial timeout is recorded, before the rest of the response window resolves.

### Follow-up RED

```text
cargo test -p double_riichi_core --test task4_decisions mixed_unlimited_decision_keeps_connected_human_open_while_auto_times_out
# FAILED: final bot ResolvedAction.timed_out was false
```

### Follow-up GREEN

```text
cargo test -p double_riichi_core --test task4_decisions mixed_unlimited_decision_keeps_connected_human_open_while_auto_times_out
# 1 passed; 0 failed

cargo test -p double_riichi_core --lib partial_timeout_promotes_disconnected_seat_and_preserves_timeout_marker -- --nocapture
# 1 passed; 0 failed

cargo fmt --all -- --check
# passed

cargo test -p double_riichi_core
# 9 unit + 3 Task 3 integration + 7 Decision + 4 timing + 4 projection tests passed; 0 failed; 0 doc tests
```

## Review round 3/5 follow-up

Follow-up commit: `fix(core): preserve staged timeout markers` (final hash: see `git rev-parse HEAD`).

Removed the late-resolution overwrite that replaced persisted timeout markers with only the current call's local timeout list. Staged zero/ten-second expirations now report every timed-out entry in the final `DecisionResolution`.

### Follow-up RED

```text
cargo test -p double_riichi_core --test task4_decisions staged_deadlines_preserve_all_timeout_markers
# FAILED: the earlier zero-duration entry was reported with timed_out=false
```

### Follow-up GREEN

```text
cargo test -p double_riichi_core --test task4_decisions staged_deadlines_preserve_all_timeout_markers
# 1 passed; 0 failed

cargo fmt --all -- --check
# passed

cargo check --workspace
# passed

cargo test --workspace
# all workspace unit, integration, and doc tests passed

git diff --check
# passed
```

## Review round 4/5 follow-up

The round 4 review independently verified all prior Critical/Important/Minor findings as addressed. No additional source change or new failing behavior slice was required; the existing RED/GREEN tests cover each finding.

### Verification

```text
cargo test -p double_riichi_core --test task4_decisions
# 8 passed; 0 failed

cargo test -p double_riichi_core --test task4_machine
# 4 passed; 0 failed

cargo test -p double_riichi_core --test task4_projection
# 4 passed; 0 failed

cargo fmt --all -- --check
# passed

cargo check --workspace
# passed

cargo test --workspace
# all workspace unit, integration, and doc tests passed

git diff --check
# passed
```
