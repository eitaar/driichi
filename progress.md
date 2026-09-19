# Progress

## 2026-09-15 — Task 15 replay/admin review fixes

- Implemented semantic replay validation, persisted half-game reconstruction,
  safe missing-path handling, typed list-time failure logging, and bounded
  list/View/startup reconstruction.
- Implemented post-lock credential revalidation, durable prepared/applied/
  rolled-back audit outcomes with recovery, command-specific no-op detection,
  token-target redaction, and Bot Token audit-count coverage.
- Validation passed: `cargo test --workspace`, focused Task 5/6/9/15 tests,
  `cargo check --workspace`, `cargo fmt --all -- --check`, and `git diff --check`.
  Clippy remains blocked only by five pre-existing `double_riichi_core` lints.
- Frontend was unchanged in this pass; no frontend checks were rerun.

## 2026-09-15 — Task 15 audit follow-up

- Accepted Room mutations now flush prepared audit rows directly; startup and
  periodic recovery retry unresolved prepared/legacy applied rows exactly once,
  while rolled-back outcomes remain excluded.
- Deselecting an already-unselected participant is a true Room no-op and no
  longer clears Ready on unrelated selected Humans; core and HTTP regressions
  cover state and audit behavior.
- Added Bot Token create/revoke audit-insert failure atomicity and close/reopen
  exact-count coverage.
- Replay View, startup, and list corruption logs now redact token-shaped IDs
  and emit typed failure kinds.

## 2026-09-15 — Task 15 final cancellation fix

- `cancel_admin_audit` now falls back from DELETE to a durable `rolled_back`
  state, verifies cancellation, and propagates inability to guarantee it to
  Admin routes.
- Recovery excludes rolled-back rows; a true Room no-op with injected
  cancellation DELETE failure remains unaudited across close/reopen.

## 2026-09-15 — Task 15 provisional acceptance

- Independent final reviews approved Replay and Admin audit with no P0/P1
  findings; one same-request-ID retry edge remains a non-blocking P2 note.
- Fresh parent verification passed all Rust workspace tests serially, focused
  Replay/Task 6/9/15 tests, frontend typecheck/41 Vitest/build, Task 15
  Playwright 4/4 in isolation at both required viewports, changed-file LSP,
  screenshot inspection, formatting, diff checks, and clean-tree verification.
- External yamai, authenticated riichi.dev, Conditional Design Freeze, strict
  compatibility, and release acceptance remain blocked and unclaimed.

## 2026-09-19 — Task 16 provisional local acceptance

- Integrated and independently approved contracts/operations, real-server
  E2E/persistence, and Release/CI lanes.
- Fresh verification passed all Rust workspace tests, contract/release checks,
  frontend typecheck/41 Vitest/build, browser 23/23 including real 3p/4p Match
  completion, embedded frontend proof, and Windows release/native smoke.
- Final residual review returned Spec PASS, Quality APPROVED, Merge OK, and no
  P0/P1/P2 findings.
- Authenticated immutable riichi.dev evidence, foreign-platform CI execution,
  Conditional Design Freeze, strict compatibility, and final release acceptance
  remain blocked and unclaimed.

## 2026-09-19 — yamai ReplayProcessor gate

- Pinned owner-supplied `eitaar/yamai` revision
  `226cb84d917376d7513fbfdf987cc6a2294767cc` and exact ReplayProcessor hash.
- Generated a complete four-player East Match; upstream yamai accepted all 1,146
  events and emitted 8 round and 560 discard samples.
- Added fail-closed `test-yamai` and CI coverage without vendoring or
  redistributing the upstream repository, whose project license is unspecified.
