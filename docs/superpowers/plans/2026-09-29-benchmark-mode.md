# Benchmark Mode Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Run 1–1,000 consecutive, persisted bot-only Matches with fixed identities, rotating seats, private Admin live observation, and per-Run statistics.

**Architecture:** Extend the existing Room actor for benchmark-only lifecycle and enforce bot admission at its command boundary; route Run writes through the existing acknowledged Room persistence worker. Store Runs in SQLite separately from Room lifetime, project live all-hands state exclusively behind an Admin-only endpoint, and derive statistics from committed Match results.

**Tech Stack:** Rust `double_riichi_core` + `double_riichi_server` (Tokio, Axum, sqlx/SQLite), React/TypeScript/Vitest.

**Spec:** `docs/superpowers/specs/2026-09-29-benchmark-mode-design.md`

## Global Constraints

- Only selected MJAI, MCP, and explicitly selected Built-in Bot Participants play; Humans and ordinary Spectators are rejected; no fill-on-start; three-player MJAI remains disallowed.
- Fixed Participant-ID roster, target count 1–1,000 inclusive, one-seat rotation per Match, independent dealing; count only successfully persisted completed Matches.
- Admin stop is after the current successfully persisted Match; external-bot disconnect, revocation, Match abort, and persistence failure stop immediately without counting that attempt or including auto-play results.
- Interrupted Runs after process restart are read-only; no automatic resume, replacement, token-level aggregation, cross-Run aggregation, or seed input.
- Only authenticated Admin receives live current all-hands state; no future wall order or other Players' private legal actions; all existing ordinary/compat privacy contracts hold.
- Preserve unrelated uncommitted ChatGPT gateway, OAuth, MCP and test changes. No new product dependencies without need.

## Review Focus

1. Built-in-only roster and an empty external connection list must not trigger automatic Room expiration during a running Run; test in Task 3.
2. Admin or transport-level disconnect of an *observer* must not stop a Run; test in Task 4.
3. A one-token/multiple-Participant roster must produce separate statistics; test in Tasks 5 and 6.
4. Replay write succeeds but Run association fails: never advance or report the Match as counted; test in Task 3.
5. Revoked Admin while streaming must no longer receive all-hands snapshots; test in Task 4.

## File map and interfaces

- `crates/double_riichi_server/migrations/0006_benchmark.sql`: durable Run header, immutable roster, counted Match association with per-player rank/score/seat, optional incomplete attempt; Match FK is restrictive so deleting a referenced replay cannot silently corrupt history.
- `crates/double_riichi_server/src/storage.rs`: Run create/finish/read and atomic Match+Run completion; keep all SQL confined here. Use `BenchmarkRunId = String` at the server edge; `ParticipantId` and `MatchId` are the existing core types.
- `crates/double_riichi_core/src/room.rs`: `RoomConfig.benchmark: bool` defaults false; `RoomCommand::AddBenchmarkBot`, `StartBenchmark { run_id: String, target: u16 }`, `StopBenchmark`, `GetBenchmarkAdminProjection`, plus `RoomSnapshot.benchmark: Option<BenchmarkProgress>`; `BenchmarkProgress { run_id: String, target: u16, completed: u16, stop_requested: bool, status: BenchmarkStatus }`, with status `Running | Completed | Stopped | Failed`. The Room owns only live progress; SQLite owns persisted history. Do not expose these commands to bots.
- `crates/double_riichi_core/src/projection.rs`: `Audience::BenchmarkAdmin` and `AudienceProjection::BenchmarkAdmin` use all-hands visibility and *public* decision visibility; existing `ReplayAdmin` stays post-Match and unchanged.
- `crates/double_riichi_server/src/http.rs`: Admin-only create/start/stop/list/detail/live-view endpoints; existing Room bot transports continue to join only an authorized Benchmark Room, while public Human paths reject it.
- `frontend/src/api.ts`, `frontend/src/app.tsx` and a focused `frontend/src/benchmark.tsx`: typed Admin Run APIs, Run controls and live observer/statistics UI. Reuse existing table renderer rather than build another board.

## Task 1: Durable Run ledger and read model

**Files:** Create `crates/double_riichi_server/migrations/0006_benchmark.sql`; modify `crates/double_riichi_server/src/storage.rs`; test `crates/double_riichi_server/tests/benchmark_storage.rs`.

**Interfaces:** Produce `Storage::create_benchmark_run(run_id: &str, room_code: &str, mode: GameMode, target: u16, roster: &[MatchPlayerSnapshot]) -> Result<(), StorageError>`, `Storage::record_benchmark_completion(run_id: &str, sequence: u16, match_id: &str, result: &MatchResult, artifact: &ReplayArtifact, completed_at: i64) -> Result<(), StorageError>`, `Storage::stop_benchmark_run(run_id: &str, status: BenchmarkRunStatus, reason: Option<&str>) -> Result<(), StorageError>`, and `Storage::load_benchmark_run(run_id: &str) -> Result<BenchmarkRunRecord, StorageError>`; `BenchmarkRunStatus = Running | Completed | Stopped | Failed | Interrupted` lives in storage. Existing `complete_room_match` delegates to shared transaction logic; benchmark completion commits replay result and Run association in one SQLite transaction. `BenchmarkRunRecord` includes roster (IDs/names/kinds), ordered counted results (Match ID, sequence, per-player rank/score/seat), status/reason, and optional failed attempt.

- [ ] Write failing migration/storage tests: identical repeated `(run_id, sequence, match_id)` completion is idempotent but conflicting repeats fail, preserve distinct Participant IDs independently of authentication token, persist per-seat score/rank keyed to Participant ID, reject deleting a referenced Match, and assert `completed == 0` after injected association failure.
- [ ] Run `cargo test -p double_riichi_server --test benchmark_storage` and observe the expected migration/API failures.
- [ ] Add minimal schema, transactional writes, Run read methods and startup `Running -> Interrupted` transition. Do not retain a live Room as the source of historical data; avoid Run writes when replay saving is disabled.
- [ ] Re-run `cargo test -p double_riichi_server --test benchmark_storage`; expect PASS. Commit only this task's files.

## Task 2: Benchmark Room admission and deterministic seat rotation

**Files:** Modify `crates/double_riichi_core/src/room.rs`; tests in its existing `#[cfg(test)]` module.

**Interfaces:** Produce `RoomConfig.benchmark: bool`, `RoomCommand::AddBenchmarkBot` (creates exactly one *unselected* Built-in Bot in a Benchmark Lobby), `RoomState::validate_benchmark_start(target: u16) -> Result<Vec<ParticipantId>, RoomError>`, `RoomState::benchmark_roster(sequence: u16) -> Result<Vec<Participant>, RoomError>`, and `BenchmarkProgress` as in File map. Task 3 owns the `StartBenchmark` command and progress transition; here validate fixed selected roster, `replay_save == true`, `target in 1..=1000`, complete seat count and connected external bots. A Benchmark Room denies Human join, late admission/selection changes, `FillWithBots`, manual `Rematch`/`BackToLobby`, and disabling replay saving. Seat for participant at sequence k is `(initial_seat + k - 1) % seat_count`; retain participant identity and selected character.

- [ ] Write failing core tests for Human join/late Spectator rejection, creating one unselected Built-in Bot then selecting it explicitly (existing `FillWithBots` creates *selected* bots and must not be reused), no auto-fill, MJAI 3-player rejection, target 0/1001, missing player/disconnected bot, and 4 Match seat cycles (plus three-seat case). Confirm normal Room's Rematch remains unchanged.
- [ ] Run `cargo test -p double_riichi_core room::tests` and observe failures.
- [ ] Implement only benchmark-specific admission/configuration guards and roster rotation; keep ordinary Room behavior intact. Expose progress in the Room snapshot without exposing concealed information.
- [ ] Re-run core tests; expect PASS. Commit this task's files.

## Task 3: Acknowledged series lifecycle and failure/stop semantics

**Files:** Modify `crates/double_riichi_core/src/room.rs`, `crates/double_riichi_server/src/storage.rs`; test `crates/double_riichi_server/tests/benchmark_run.rs` and core Room tests.

**Interfaces:** `RoomCommand::StartBenchmark { run_id: String, target: u16 }` and `RoomCommand::StopBenchmark`; `RoomEffect::CreateBenchmarkRun { run_id, target, roster, completion }`, `FinalizeMatch { ..., benchmark: Option<(run_id, sequence)> }`, `StopBenchmarkRun { run_id, status, reason, completion }` travel through the existing effect channel. `RoomCommand::StopBenchmark` marks stop-after-current and persists terminal state after successful finalization; `RoomCommand::GetBenchmarkAdminProjection` is handled in Task 4. The Run's first Match opens only after its header is acknowledged; completion awaits atomic persistence before increment/start. Terminal failure takes precedence over a previously requested graceful stop.

- [ ] Write failing tests for exactly `n` counted Matches at n=1 and n=2, immediately launching Match 2 only after Match 1 durable acknowledgment, all-Built-in run remaining alive, graceful stop after finishing one Match, replay/association failures not advancing count, duplicate acknowledgment not double-starting, and disconnect/leave/kick/token revocation/abort stopping before auto-play. Test a bot reconnect does not revive the Run; observe unrelated ordinary Room behavior unchanged.
- [ ] Run `cargo test -p double_riichi_server --test benchmark_run` and the focused core tests; confirm failures.
- [ ] Wire effects, ack handling, progress events and terminal state through Room actor and existing storage worker. Check benchmark disconnect *before* `sync_machine_controllers` or decision advancement; clean incomplete replay and persist stop reason when possible. Avoid recursively chaining thousands of fully auto Matches in one task loop: yield between starts while retaining actor-serialized ordering. Keep Room cleanup from deleting active benchmark progress.
- [ ] Re-run focused tests; expect PASS. Commit this task's files.

## Task 4: Authenticated live Admin view without Player leakage

**Files:** Modify `crates/double_riichi_core/src/projection.rs`, `crates/double_riichi_core/src/room.rs`, `crates/double_riichi_server/src/http.rs`; test core projection tests and `crates/double_riichi_server/tests/benchmark_visibility.rs`.

**Interfaces:** `RoomHandle::benchmark_admin_projection() -> Result<Option<AudienceProjection>, RoomError>` sends `GetBenchmarkAdminProjection`; the Room rejects ordinary Rooms. Admin-only `GET /api/v1/admin/benchmark/rooms/{join_code}/live` returns current projection plus revision; `GET /api/v1/admin/benchmark/rooms/{join_code}/live/events` streams SSE under the existing `/api/v1/admin` session-cookie path, sends an authoritative projection on connect and each Room revision, and revalidates Admin permission before forwarding each private update. Neither route supplies an action command or participant ID.

- [ ] Write failing tests: all hands present for authenticated Admin and no other Player's private legal options, wall order absent, ordinary Rooms denied, public/bot/unauthenticated routes do not leak hands, observer disconnect leaves progress unchanged, and expired/revoked Admin sessions stop receiving private SSE updates.
- [ ] Run `cargo test -p double_riichi_server --test benchmark_visibility` and focused core projection tests; confirm failures.
- [ ] Implement `Audience::BenchmarkAdmin` with all-hands + public decision, dedicated Room command and authenticated HTTP/SSE routes. Reuse existing admin authentication/Origin policy and bounded Room subscription; do not reuse `ReplayAdmin` for live visibility.
- [ ] Re-run tests; expect PASS. Commit this task's files.

## Task 5: Admin Run lifecycle endpoints and read-side stats

**Files:** Modify `crates/double_riichi_server/src/http.rs`, `crates/double_riichi_server/src/storage.rs`; test `crates/double_riichi_server/tests/benchmark_http.rs`.

**Interfaces:** `POST /api/v1/admin/benchmark/rooms` creates a Benchmark Room (same room configuration validation, forced `replay_save=true`); `POST /api/v1/admin/benchmark/rooms/{join_code}/bots` adds exactly one unselected Built-in Bot (Task 2); `POST /api/v1/admin/benchmark/rooms/{join_code}/runs` body `{ "target": 1 }` starts selected roster; `POST /api/v1/admin/benchmark/rooms/{join_code}/stop` requests graceful stop; `GET /api/v1/admin/benchmark/runs` lists durable Runs; `GET /api/v1/admin/benchmark/runs/{run_id}` returns Run record with per-Participant aggregates. The HTTP start handler generates a Run ID once and delegates to `StartBenchmark`; no separate second Run creation race. Endpoints require existing Admin auth and unsafe-method Origin/audit conventions.

- [ ] Write failing API tests for auth/Origin including the add-bot action, invalid target 0/1001, start without full explicit selection, same-token distinct Participants, stop idempotency and a read-only interrupted Run after simulated restart/Room deletion. Assert zero completed Matches yields null average/rate and an empty series.
- [ ] Run `cargo test -p double_riichi_server --test benchmark_http`; confirm failures.
- [ ] Implement endpoints and stats read model. Average rank = sum(rank)/completed count; first-place rate = rank-1 count/completed count. For each Match graph increment = player's final score minus the mean of all final scores for that Match, plotted in sequence order; don't derive rank from score when stored rank exists. Keep failed attempts outside every denominator.
- [ ] Re-run endpoint tests; expect PASS. Commit this task's files.

## Task 6: Admin controls, observer and results UI

**Files:** Modify `frontend/src/api.ts`, `frontend/src/app.tsx`, `frontend/src/app.test.tsx`; create `frontend/src/benchmark.tsx`, `frontend/src/benchmark.test.tsx`; add focused styles to `frontend/src/styles.css` or existing game stylesheet.

**Interfaces:** `api.createBenchmarkRoom`, `api.addBenchmarkBot`, `api.startBenchmarkRun`, `api.stopBenchmarkRun`, `api.listBenchmarkRuns`, `api.getBenchmarkRun` mirror Task 5 routes and typed Run record; live observer uses Task 4 Admin SSE stream and the existing table renderer's projection input. Admin navigation exposes Benchmark list/create and per-Run page without exposing an ordinary human join link.

- [ ] Write failing Vitest tests for adding and selecting one Built-in Bot at a time with no auto-fill, target 1/1000 input, loading/failed/stop/interrupted/completed states, empty statistics, separate same-token Participants, mean rank and first-place rate, centered cumulative series (including n not divisible by seats), no fake points for aborted attempts, and live-observer reconnect. Assert ordinary Room UI still works.
- [ ] Run `cd frontend && npm test -- --run src/benchmark.test.tsx src/app.test.tsx`; confirm failures.
- [ ] Implement narrow Admin UI using existing room participant selection and table controls where possible; draw one simple accessible line chart with text/table equivalents and zero-dependency SVG/CSS. Label the graph as cumulative score relative to table average.
- [ ] Re-run focused tests and `cd frontend && npm run build`; expect PASS. Commit this task's files.

## Task 7: Contract and regression gate

**Files:** Test-only additions to `crates/double_riichi_server/tests/benchmark_run.rs` and `benchmark_visibility.rs` as needed; update existing contract tests only if additive response fields require it.

**Interfaces:** End-to-end test with real Room actor and server storage: Admin creates Room, explicitly selects MCP/MJAI/Built-in participants, runs two Matches, confirms persisted Match IDs and seat rotation, checks Admin all-hands vs bot/private/public projection, stops or finishes, reloads historical stats. Use bounded decisions and actual auth, not mocked success signals.

- [ ] Add the focused regression scenario above and negative three-player-MJAI, incomplete replay, and restart cases not covered by earlier integration tests.
- [ ] Run `cargo fmt --all -- --check`, `cargo test --workspace`, `cd frontend && npm test`, and `cd frontend && npm run build`; expect all PASS. If pre-existing dirty OAuth/MCP tests fail, report the exact unrelated failure without altering those files.
- [ ] Run `git diff --check`, inspect changed-file diagnostics, and inspect `git status --short` to ensure unrelated user edits remain untouched. Commit only benchmark files/tests after verification.
