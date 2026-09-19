# Task 16 report — observability, contracts, E2E, and release

## Status

`SUPPORTED_PROVISIONALLY` for the complete local Task 16 implementation through
`5a33092`. Independent final review returned **Spec PASS**, **Quality APPROVED**,
and **Merge OK**, with no remaining P0/P1/P2 findings.

This is not Design Frozen and is not final compatibility/release acceptance.
Authoritative yamai `ReplayProcessor` evidence, authenticated immutable
riichi.dev parity evidence, and successful foreign-platform CI executions remain
external gates.

## Implemented

- Hand-authored OpenAPI 3.1 and AsyncAPI 3.0 contracts with exact-pinned official
  schema validation, complete local route inventory checks, representative
  fixtures, runtime field checks, strict Human Room event schemas, documented
  WebSocket close codes, and bounded identifiers.
- Admin-authenticated, default-off raw OpenAPI at
  `/api/v1/admin/openapi.yaml`; no Swagger UI.
- Allowlisted text/JSON tracing, bounded path identifiers, Health/status/build
  metadata, graceful shutdown ordering, final storage cleanup, and explicit
  SQLite pool close.
- Per-Room bounded Replay workers with acknowledged open/Kyoku flush/finalize,
  isolated backpressure, authoritative finalization, safe failure cleanup,
  storage-health degradation, auxiliary lifecycle events, and completed-Replay
  protection under pool starvation.
- Actual Rust-server Playwright coverage for complete 3p and 4p Human Matches,
  concrete action-result correlation, Post-Match, persisted Replay Library and
  Viewer, axe checks, reduced motion, keyboard/focus behavior, and retained
  screenshots at 1024x600 and 1440x900.
- Compile-time embedded production frontend with API/WS precedence, allowlisted
  SPA routes, traversal rejection, security/cache headers, and fresh release-mode
  embedding proof.
- Deterministic self-contained Windows x86_64, Linux x86_64, and Linux ARM64
  packaging; separate versioned Starter Pack ZIP; SHA-256 files; notices and
  licenses; native and dry-run smoke tooling; no committed generated media.
- Immutable-action CI, exact three-platform release matrix, tag/Cargo version
  validation, GitHub Release publication, scheduled/manual credential-gated
  `test-live`, and fail-closed live evidence handling.
- Completed Room effect-worker handles are reaped during registry activity so
  room churn does not retain finished task metadata.
- Windows CRLF-sensitive Starter license comparison was made platform-stable.

## Verification

Fresh integrated parent verification passed:

- `python scripts/validate_contracts.py`.
- `python scripts/release/test_release.py` — 7/7.
- `cargo fmt --all -- --check`.
- `cargo check --workspace`.
- `cargo test --workspace -- --test-threads=1` — all unit, integration, and doc
  tests passed, including Core Room 24/24, Replay/Storage 30/30, Task 16
  contracts 17/17, and the worker-reaping regression.
- Frontend typecheck, Vitest 41/41, and production build.
- Real-server Playwright 2/2 for complete 3p/4p flows; complete browser suite
  23/23.
- Fresh production frontend embedding gate.
- Fresh Windows x86_64 release archive build at `5a33092`, followed by native
  version/start/root/static-content smoke.
- Release checksums/archive metadata and representative retained screenshots
  were inspected.
- `git diff --check` and clean-tree checks.

The first redundant final archive rerun hit a transient Windows `npm ci` EPERM
on `tsc.exe`; a clean retry completed successfully without source changes.

`cargo clippy --workspace --all-targets -- -D warnings` remains blocked by four
pre-existing `double_riichi_core` lints in `decision.rs`/`engine.rs`
(type-complexity, two needless-range-loop, and too-many-arguments). No Task 16
lint was reached before that unchanged baseline failure.

## Review disposition

Initial integrated review found two release P1s and one worker-retention P2;
all were fixed in `38c43c4`. The Spec-axis review then identified shutdown,
identifier, and Human WebSocket contract gaps; those were fixed in `5a33092`.

Final residual review found no remaining issues. It classified a Room-side
Replay-finalization timeout as not applicable: spec §19.1 requires one
authoritative flush → sync → rename → SQLite → Post-Match outcome, while a
cancellable Room timeout could allow a late commit followed by cleanup. The
configurable server shutdown deadline remains the lifecycle bound, and database
acquisition/operations are bounded internally.

## Parked external gates

- No authoritative yamai repository/package, immutable revision, license, or
  `ReplayProcessor` API is available; no real yamai acceptance run occurred.
- Authenticated riichi.dev ranked/validate transcripts and immutable Protocol v2
  evidence remain unavailable without the external Bot Token/source.
- Linux x86_64 and Linux ARM64 artifacts require successful GitHub Actions runs;
  local Windows smoke cannot claim foreign-platform execution.
- `test-live` remains manual/scheduled and credential-gated; it was not replaced
  by a permissive status probe.

Therefore strict compatibility, Design Frozen, and final release acceptance
remain unclaimed.
