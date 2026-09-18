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
