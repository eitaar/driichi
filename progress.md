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
