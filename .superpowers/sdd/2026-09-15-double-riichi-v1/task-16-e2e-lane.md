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

The commit contains the coherent owned frontend/E2E changes, necessary Room
persistence/backend changes and regression proof, plus this report. External
Yamai/riichi.dev, release, and Conditional Design Freeze gates remain outside
this lane and are not claimed here.
