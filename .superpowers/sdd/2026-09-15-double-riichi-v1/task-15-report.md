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
- Added TanStack Query Replay list/view/delete data flow, URL-backed
  pagination with Back/Forward support and later-page delete clamping,
  working list retry, accessible playback controls (Play, Pause, Previous
  Event, Next Event, 0.5x/1x/2x/4x, Kyoku jump), ordered auxiliary
  status/event-log entries, generic silent fallback based on actual Character
  asset loading, Room audio/portrait presentation through the live helpers,
  delete confirmation, loading/error/empty states, and responsive dark
  broadcast-noir styling using the existing Pixi renderer.

## Verification

- `cargo fmt --all -- --check` — passed.
- `cargo check --workspace` — passed before final focused reruns; changed server
  tests also pass `cargo check -p double_riichi_server --tests`.
- `cargo test -p double_riichi_server --test task15_replay` — 4 passed,
  including authenticated route coverage, path containment, oversize/corrupt
  handling, auxiliary persistence, and file-first delete retryability.
- `cargo test --workspace` — parent verification passed all workspace unit,
  integration, and doc tests, including Task 15.
- `cd frontend && npm run typecheck` — passed.
- `cd frontend && npm test -- --run src/replay.test.tsx` — 9 passed, including
  URL pagination, later-page deletion, Retry, real asset-policy mocks, Room
  audio/portrait helpers, and playback controls.
- `cd frontend && npm test -- --run` — parent verification passed the full
  frontend suite.
- `cd frontend && npm run build` — passed.
- `cd frontend && npx playwright test tests/task15.spec.ts` — focused library /
  viewer and review-fix flows passed at 1024x600 and 1440x900; screenshots
  were written under `frontend/test-results/task-15/`.
- `git diff --check` — passed.

## Review-fix recovery evidence

Two runner attempts crashed with EPERM during the Task 15 review-fix pass.
The preserved dirty patch was audited in place rather than restarted. The
finisher retained the production fixes for modifier-safe links, skip/main
semantics, localized time and announced loading, URL/back-forward pagination,
later-page delete clamping, list Retry, actual Character asset availability,
and Room audio/portrait presentation; it added the missing browser assertions
and completed replay-route security/failure evidence without changing the
renderer or API design.
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
