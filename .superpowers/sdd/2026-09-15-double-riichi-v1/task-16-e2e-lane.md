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
voluntary leave remains `left`. Finalization has a cancellation control so a
late worker cannot commit after the Room becomes unavailable. Human
`action_result` echoes the concrete submitted `action_id`; the browser lane
asserts that correlation for both 3p and 4p.

Round-three focused verification:

- `cargo test -p double_riichi_core --test task8_room --no-fail-fast` — **24 passed**;
  rollback, Rematch rollback, per-Room isolation, append/auxiliary saturation,
  kick ordering, and delayed-finalize cancellation are covered.
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
