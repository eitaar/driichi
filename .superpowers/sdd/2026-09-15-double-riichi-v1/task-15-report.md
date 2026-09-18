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
  enforced by the Replay reader/frame builder and the HTTP response boundary;
  list availability uses the same bounded frame reconstruction, including
  parseable-but-invalid and oversized frame payloads.
- Completed Replay rows are bounded-validated during Storage startup, so
  corruption degrades fresh-instance health and logs only match ID/error
  details; startup validation never allocates an HTTP response.
- Corrupt, missing, unsafe, and oversized files remain listable/deletable;
  view errors are stable RFC Problem responses, internal details are logged,
  and replay health is degraded.
- Delete removes the registered file first (missing is success), then deletes
  Match/Player/auxiliary rows and writes the allowlisted Admin audit record in
  one transaction, leaving metadata retryable when the database step fails.
- All Admin state-changing Room, participant, session, and token operations
  now preflight their audit insert, serialize mutations, and complete through a
  durable pending-audit outbox; failed commands cancel pending records and
  no-op commands do not emit success audits. Room deletion distinguishes an
  actual removal from an already-gone actor.
- Ranked replay cleanup retains the writing metadata/path when post-finalize
  unlink fails, so startup can retry the registered artifact cleanup.
- Replay parsing and frame reconstruction cap event counts as well as bytes;
  auxiliary and Player rows are counted/budgeted before materialization.
- Storage startup cleanup invokes the Replay crate's global `.incomplete`
  cleanup in addition to the database-backed writing-row cleanup. A
  cancellation-aware maintenance task retries pending audits and retention
  cleanup daily, probes Replay storage every 60 seconds, and feeds cached
  component status to health.
- Admin Replay View negotiates gzip through a route-scoped tower-http layer
  with only the `compression-gzip` feature enabled.
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
- `cargo check --workspace` — passed; changed server tests also pass
  `cargo check -p double_riichi_server --tests`.
- `cargo test -p double_riichi_server --test task15_replay` — 21 passed,
  including bounded frame/list parity, fresh-startup corruption health, orphan
  `.part` cleanup, prepared/applied audit recovery, gzip
  negotiation/decompression, authenticated route and Admin audit coverage,
  no-op fill/deselect suppression, logout rollback, concurrent deletion, path
  containment, oversize/corrupt handling, auxiliary persistence, file-first
  delete retryability, half-game mode reconstruction, missing-ancestor
  handling, pending-state-update resilience, and late audit recovery.
- `cargo test --workspace` — passed all workspace unit, integration, and doc
  tests, including Task 15.
- `cd frontend && npm run typecheck` — passed.
- `cd frontend && npm test -- --run` — 41 passed across 3 files, including URL
  pagination, later-page deletion, Retry, real asset-policy mocks, Room
  audio/portrait helpers, and playback controls.
- `cd frontend && npm run build` — passed.
- `cd frontend && npx playwright test tests/task15.spec.ts` — focused library /
  viewer and review-fix flows passed at 1024x600 and 1440x900; screenshots
  were written under `frontend/test-results/task-15/`.
- `cargo test -p double_riichi_replay --test task5_replay` — 18 passed,
  including incremental reconstruction expansion limits and semantic discard /
  meld validation.
- `cargo test -p double_riichi_server --lib` — 39 passed, including retained
  writing metadata when finalized-replay cleanup cannot unlink its artifact,
  credential revalidation, command-specific no-op detection, and token-target
  redaction.
- `cargo test -p double_riichi_server --test task6_config_auth_storage` — 11
  passed, including migration, durable audit storage, and Bot Token audit
  failure/reopen exact-count coverage.
- `cargo test -p double_riichi_server --test task9_http` — 10 passed,
  including Bot Token lifecycle audit-count assertions.
- `cargo test -p double_riichi_core --lib` — 11 passed.
- `cargo test -p double_riichi_core --test task8_room` — 19 passed,
  including the unselected-deselect Ready regression.
- `cargo clippy -p double_riichi_server --all-targets -- -D warnings` — blocked
  by the same five pre-existing `double_riichi_core` lints; no changed-file
  lint was reported before that baseline failure.
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

## Review-fix implementation evidence

The Task 15 replay/admin review fixes are implemented without frontend or API
architecture changes:

- Replay reconstruction now validates state-dependent discards, meld shapes,
  consumed tiles, active-kyoku ordering, and persisted East/Half mode. List,
  View, and startup validation use the same bounded reconstruction path.
- Registered replay paths tolerate missing ancestors as unavailable while
  preserving traversal and symlink defenses; View reports the failure and
  Delete remains idempotent. List-time failures log only `match_id` and a
  typed failure kind.
- Admin mutations revalidate credentials after waiting for the mutation lock.
  Durable pending-audit rows now distinguish `prepared`, `applied`, and
  `rolled_back`; accepted `prepared`/`applied` rows recover on restart while
  rolled-back outcomes remain excluded. Command-specific no-op comparisons
  ignore unrelated public joins.
- Audit-failure diagnostics redact token-shaped target IDs while retaining safe
  IDs. Task 9 Bot Token lifecycle coverage asserts exactly one audit per
  successful state-changing operation.

Validation for this pass also included `cargo check --workspace`,
`cargo fmt --all -- --check`, `git diff --check`, and the full
`cargo test --workspace` suite. Frontend checks were not rerun because this
pass changed no frontend files.

## Audit follow-up evidence

The follow-up closes the remaining Admin-audit findings without changing the
approved Replay behavior:

- Accepted Room mutations flush their durable prepared outbox row directly;
  there is no fragile prepared-to-applied state update between the actor
  mutation and audit insertion. Startup and the maintenance retry process
  recover both unresolved `prepared` and legacy `applied` rows exactly once.
  Explicit `rolled_back` rows remain excluded. Regression coverage exercises a
  pending-state-update failure trigger and a post-mutation flush failure across
  close/reopen.
- Room deselect now returns before clearing unrelated selected Human Ready state
  when the target is already unselected. Core and HTTP regressions prove the
  state and audit no-op behavior.
- Bot Token create/revoke audit-insert failures remain atomic, and successful
  create/revoke rows retain exactly one audit each after close/reopen.
- Replay View, startup, and list corruption logs now use the shared token-safe
  identifier redaction and typed failure classification. The helper regression
  covers token-shaped replay IDs.

Follow-up validation passed focused Task 6/8/9/15 tests, `cargo fmt --all
-- --check`, `cargo check --workspace`, `cargo test --workspace --
--test-threads=1`, and `git diff --check`. Frontend checks were not rerun
because this follow-up changed no frontend files.

## Final cancellation follow-up evidence

The final narrow fix preserves prepared-row recovery while making cancellation
itself durable:

- `cancel_admin_audit` deletes prepared rows when possible, falls back to a
  durable `rolled_back` transition when deletion fails, and verifies the row
  is absent or rolled back. Any inability to guarantee cancellation is
  propagated to the Admin route instead of returning a false success/no-op.
- Recovery continues to exclude rolled-back rows and request-ID deduplication
  remains unchanged.
- A Task 15 regression injects cancellation DELETE failure during a true
  `fill_with_bots` no-op, verifies the fallback rolled-back outcome, closes and
  reopens storage, and proves no audit row is created for the canceled request.

Final validation passed focused Task 15 and storage/audit tests, `cargo fmt
--all -- --check`, `cargo check --workspace`, the serial full workspace suite,
and `git diff --check`. Frontend checks were not rerun because this fix changed
no frontend files.

## Scope and residual risks

- Internal test-only `FailureInjection`/`ServerState::for_tests` and the
  pre-existing `Storage::pool` remain non-blocking API-surface notes; they are
  not network-reachable production capabilities and this is not a security
  claim.
- Pending audit recovery is durable across process interruption after an
  accepted mutation's prepared marker; the actor mutation and database audit
  cannot be committed in one SQL transaction, so the serialized outbox is the
  recovery boundary. Explicitly canceled/rolled-back rows remain excluded.
- Room Replay metadata remains dependent on the existing persistence producer;
  this slice does not add a new Room persistence pipeline.
- External yamai/riichi.dev/Conditional Design Freeze/release gates remain
  deferred as recorded in the SDD ledger.
- Generated frontend build/test artifacts remain ignored and are not committed.
