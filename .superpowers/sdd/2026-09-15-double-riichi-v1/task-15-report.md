# Task 15 report — Admin Replay API and Viewer

## Status

`SUPPORTED_PROVISIONALLY` for the local authenticated Admin Replay surface. The
local API, storage path checks, server-built frames, corruption/size handling,
delete ordering, audit record, and Replay Viewer are implemented and verified.
The previously parked Task 2/yamai and upstream compatibility gates remain
unclaimed.

## Implemented

- Added Admin-only newest-first completed Replay listing with strict offset
  pagination (default 50, maximum 100), metadata, availability state, and no
  raw MJSON route.
- Added Admin Replay view and delete routes. View resolves the registered path
  below the configured Replay root, loads auxiliary metadata, and returns
  server-built `ReplayFrame` values. The 64 MiB decompressed payload limit is
  enforced by the Replay reader/frame builder and the HTTP response boundary.
- Corrupt, missing, unsafe, and oversized files remain listable/deletable;
  view errors are stable RFC Problem responses, internal details are logged,
  and replay health is degraded.
- Delete removes the registered file first (missing is success), then deletes
  Match/Player/auxiliary rows and writes the allowlisted Admin audit record in
  one transaction, leaving metadata retryable when the database step fails.
- Persisted ranked auxiliary Replay events during completion.
- Added TanStack Query Replay list/view/delete data flow, accessible playback
  controls (Play, Pause, Previous Event, Next Event, 0.5x/1x/2x/4x, Kyoku
  jump), ordered auxiliary status/event-log entries, generic silent fallback,
  delete confirmation, loading/error/empty states, and responsive dark
  broadcast-noir styling using the existing Pixi renderer.

## Verification

- `cargo fmt --all -- --check` — passed.
- `cargo check --workspace` — passed before final focused reruns; changed server
  tests also pass `cargo check -p double_riichi_server --tests`.
- `cargo test -p double_riichi_server --test task15_replay` — 2 passed.
- `cargo test --workspace` — all workspace unit, integration, and doc tests
  passed, including Task 15 (2 tests).
- `cd frontend && npm run typecheck` — passed.
- `cd frontend && npm test -- --run` — 36 tests passed in 3 files.
- `cd frontend && npm run build` — passed.
- `cd frontend && npx playwright test tests/task15.spec.ts` — 2 passed at
  1024x600 and 1440x900; screenshots were written under
  `frontend/test-results/task-15/`.
- `git diff --check` — passed.
- Focused `cargo clippy -p double_riichi_server --all-targets -- -D warnings`
  remains blocked by five documented pre-existing Clippy lints in
  `double_riichi_core`; no changed-file lint was reported before that baseline
  failure.

## Scope and residual risks

- Room Replay metadata remains dependent on the existing persistence producer;
  this slice does not add a new Room persistence pipeline.
- External yamai/riichi.dev/Conditional Design Freeze/release gates remain
  deferred as recorded in the SDD ledger.
- Generated frontend build/test artifacts remain ignored and are not committed.
