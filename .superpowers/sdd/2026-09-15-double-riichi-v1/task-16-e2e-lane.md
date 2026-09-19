# Task 16 E2E lane — real-server recovery

## RED / recovery evidence

- Recovery continued from the preserved dirty worktree without reset, checkout,
  stash, or discard. The async runner disappeared after its bash tool remained
  open with no Playwright, server, or Vite process left; preserved evidence is
  `C:/Users/eitab/AppData/Local/Temp/driichi-recovery/task16-e2e-fix1-hung-tool.diff`
  and `.txt`.
- The prior failed focus assertion was corrected to the actual DOM order: after
  focusing `Skip to main content`, Tab focuses the brand link (`Double Riichi
  home`), then the next Tab focuses `Rooms`. Both links remain asserted.
- The authorized persistence fix routes production Room effects through a
  bounded storage worker, records room source/name/roster metadata, and keeps
  gameplay running when persistence fails. The focused server regression proves
  a completed 3p Room Replay is listed and viewable through the Admin API.

## GREEN / exact verification

Commands were run from `frontend/` unless noted:

- `timeout 360s npm run test:browser:real -- --grep '3p-red-east' tests/task16-real-server.spec.ts`
  — **1 passed (1.5m)**; the 3p test itself completed in 46.5s.
- `timeout 600s npm run test:browser:real -- tests/task16-real-server.spec.ts`
  — **2 passed (3.1m)**, one worker: 3p in 1.1m and 4p in 1.6m. Axe checks,
  accepted Human actions, Post-Match controller state, concrete multi-candidate
  action coverage, bounded Replay API responses, both required viewports, and
  Replay skip-link focus order passed.
- `timeout 180s cargo test -p double_riichi_server --test task15_replay completed_room_match_is_visible_through_admin_replay_api -- --exact`
  — **1 passed**; Room Replay source/name/roster/frames are persisted and
  visible through the Admin list/View API.
- `timeout 180s cargo test -p double_riichi_core --test task8_room persistence_`
  — **3 passed**; bounded effects, gameplay-on-backpressure, and persistence
  acknowledgement failure behavior remain covered.
- `timeout 180s npm run typecheck` — passed (`tsc --noEmit`).
- `timeout 300s npm test -- --run src/app.test.tsx src/game/task12.test.ts src/replay.test.tsx`
  — **3 files passed, 41 tests passed** (Vitest 5.0.1).
- `timeout 180s npm run build` — passed; TypeScript and Vite production build
  transformed 5,409 modules.
- `cargo fmt --all -- --check` and `git diff --check` — passed.

## Review evidence and hygiene

Screenshots are retained only under
`frontend/test-results/task-16-review/` (entry, Admin, Lobby, actual Decision,
Results, Replay library, Replay viewer, and Post-Match Admin at `1024x600` and
`1440x900`; representative paths include
`frontend/test-results/task-16-review/3p-red-east-decision-1024x600.png` and
`frontend/test-results/task-16-review/4p-red-east-replay-viewer-1440x900.png`).
Generated `.vitest/`, root `node_modules/`, Playwright reports/results, and
`NUL` were removed; the ignored `frontend/node_modules/` installation remains.

The round-two implementation now awaits OpenMatch, FlushKyoku, and
FinalizeMatch acknowledgements in the Room actor; production rooms use one
ordered bounded effect worker per Room, and registry shutdown drains and joins
those workers before storage cleanup. Replay-worker failures set the shared
storage degradation signal, lifecycle auxiliary events are ordered on the Room
queue, and Room start/completion timestamps are carried into persistence.
POSIX real-server teardown now terminates detached process groups and Windows
continues to use taskkill tree termination.

Round-two focused verification:

- `timeout 360s npm run test:browser:real -- --grep '3p-red-east' tests/task16-real-server.spec.ts`
  — **1 passed (1.9m)**.
- `timeout 360s npm run test:browser:real -- --grep '4p-red-east' tests/task16-real-server.spec.ts`
  — **1 passed (1.9m)**.
- `cargo test -p double_riichi_core --no-fail-fast` — **all core tests passed**;
  persistence acknowledgement and bounded-room regressions are included.
- `cargo test -p double_riichi_server --test task15_replay --no-fail-fast` —
  **23 passed**, including Room replay persistence and cleanup recovery.
- `timeout 180s npm run typecheck` — passed; focused Vitest remains **41 passed**.
- `cargo fmt --all -- --check` and `git diff --check` — passed.

Screenshots remain only under `frontend/test-results/task-16-review/`.
Generated Vitest and Playwright report debris was removed. External
Yamai/riichi.dev, release, and Conditional Design Freeze gates remain outside
this lane and are not claimed here.

## Round 3 — persistence failure semantics and action correlation

The round-three fix replaces the initial and Rematch open-failure paths with
transactional rollback: the initial Room remains in Lobby and Rematch remains
Post-Match, both return `persistence` without starting gameplay. Per-Room
append and auxiliary saturation now marks only the owning Room unavailable,
queues ordered incomplete cleanup, and uses a dedicated worker-to-actor
failure channel; production storage marks failed metadata, cleans Failed and
Writing artifacts on DeleteIncomplete/shutdown/startup, and retains failed
rows when metadata deletion itself fails so retry remains possible. Admin kick
now has its own Room command/controller reason and persists `kicked`, while
voluntary leave remains `left`. Finalization waits for the bounded SQLite
operation to reach one terminal acknowledgement before changing Room health, so
a late worker cannot commit after the Room becomes unavailable. Human
`action_result` echoes the concrete submitted `action_id`; the browser lane
asserts that correlation for both 3p and 4p.

Round-three focused verification:

- `cargo test -p double_riichi_core --test task8_room --no-fail-fast` — **24 passed**;
  rollback, Rematch rollback, per-Room isolation, append/auxiliary saturation,
  kick ordering, and delayed-finalize failure behavior are covered.
- `cargo test -p double_riichi_server --lib storage::tests --no-fail-fast` —
  **2 passed**; Kicked auxiliary mapping and frame ordering pass.
- `cargo test -p double_riichi_server --test task15_replay --no-fail-fast` —
  **25 passed**; worker failure notification, Failed cleanup/retryability,
  startup cleanup, and Room Replay persistence pass.
- `cargo test -p double_riichi_server --test task9_http --no-fail-fast` —
  **10 passed**; Human accepted/rejected action results echo action IDs.
- `python scripts/validate_contracts.py` — passed; checksum validation now
  normalizes Windows CRLF checkout bytes without changing the pinned schema.
- `cd frontend && npm test -- --run src/app.test.tsx src/game/task12.test.ts src/replay.test.tsx`
  — **41 passed**; `npm run typecheck` and `npm run build` passed.
- `cd frontend && timeout 600s npm run test:browser:real -- tests/task16-real-server.spec.ts`
  — **2 passed** (3p 41.2s, 4p 1.7m), including concrete accepted Human
  action-result correlation.
- `cargo check --workspace`, `cargo fmt --all -- --check`, and `git diff --check`
  — passed.

Ignored screenshots remain only under `frontend/test-results/task-16-review/`;
Playwright report, Vitest, root node_modules, and other generated debris were
removed after verification.

## Round 4 — cleanup isolation, terminal finalization, and contract drift

The Room shutdown/abort path now sends an acknowledged cleanup-only effect;
it does not reuse the failure path or degrade a healthy Room. Worker cleanup
failures still mark storage degraded, while unrelated Rooms retain healthy
replay state. Startup cleanup processes each unfinished row independently,
repairs safe metadata beside unsafe registered paths, never touches an external
path, and exposes replay-only degradation through the health contract. Finalize
workers no longer use a cancellable atomic flag; the Room waits beyond SQLite's
busy timeout for the authoritative result.

Round-four focused verification:

- `cargo test -p double_riichi_core --test task8_room` — **24 passed**.
- `cargo test -p double_riichi_server --test task15_replay` — **30 passed**;
  acknowledged shutdown cleanup, two-terminal SQLite delayed finalization,
  replay health degradation, and mixed safe/unsafe startup cleanup pass.
- `cargo test -p double_riichi_server --test task16_contracts` — **12 passed**.
- `python scripts/validate_contracts.py` — passed; OpenAPI start/rematch 503
  responses and replay-degraded health fixture are validated.
- `cargo fmt --all -- --check` and `git diff --check` — passed.

## Round 5 — authoritative finalization and contract drift closure

Room finalization now waits for the ordered worker acknowledgement without a
Room-side cancellation deadline. Storage explicitly uses SQLite's five-second
busy timeout and a ten-second pool acquisition timeout, so a starved pool has
one bounded worker-owned outcome. Incomplete cleanup claims its writing/failed
row before removing files and leaves completed replay artifacts untouched. The
RoomDetail OpenAPI schema and validator now require runtime
`persistence_degraded` and `replay_available` booleans; runtime contract tests
assert both fields.

Round-five focused verification:

- `cargo test -p double_riichi_core --test task8_room --no-fail-fast` — **24
  passed**.
- `cargo test -p double_riichi_server --lib storage::tests --no-fail-fast` —
  **3 passed**, including completed-replay cleanup protection.
- `cargo test -p double_riichi_server --test task15_replay --no-fail-fast` —
  **30 passed**, including pool starvation held beyond the former six-second
  Room deadline.
- `cargo test -p double_riichi_server --test task16_contracts --no-fail-fast` —
  **12 passed**.
- `cargo test -p double_riichi_server --test task9_http --no-fail-fast` — **10
  passed**.
- `python scripts/validate_contracts.py` — passed; RoomDetail booleans and
  requiredness are checked alongside the existing OpenAPI/AsyncAPI contracts.
- `cargo check --workspace`, `cargo fmt --all -- --check`, and `git diff
  --check` — passed.
- `cargo test --workspace --no-fail-fast -- --test-threads=1` — failed only
  at the pre-existing Windows CRLF-sensitive
  `task7_characters::starter_generator_is_deterministic_and_covers_every_required_pack_asset`
  fixture test; all Task 16-focused binaries passed in that serial run.
