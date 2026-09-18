# Task 16 report — contracts and operations slice 1

## Status

`SUPPORTED_PROVISIONALLY` for the local contracts/operations slice only. The
spec remains **Conditional Design Freeze**. This slice does not claim release
acceptance, Design Frozen status, yamai ReplayProcessor completion, or
authenticated immutable riichi.dev parity.

The preserved `spec/fixtures/` work was audited in place. Five representative
fixtures plus the retained fixture index were kept; the interrupted NUL-filled
placeholders were repaired into valid JSON rather than reset or discarded.

## Implemented

- Added hand-authored `spec/openapi.yaml` and `spec/asyncapi.yaml`. OpenAPI
  covers the local HTTP/asset routes but deliberately has no MCP or Human WS
  schema. AsyncAPI defines Human `/ws/v1`; Room MJAI remains a join/auth
  wrapper with an explicit provisional riichi.dev upstream pointer and no
  duplicated Protocol v2 schema.
- Added exact-pinned local contract validation through
  `scripts/validate_contracts.py` using the complete lock in
  `scripts/requirements-contracts.txt` (`PyYAML==6.0.3`,
  `jsonschema==4.25.1`, and `openapi-spec-validator==0.7.2`). It validates
  official OpenAPI and vendored AsyncAPI 3.0.0 schemas, full local references,
  representative fixtures, the five-fixture index, exact OpenAPI/Axum route
  inventory comparison, and runtime DTO/event field sets.
- Added the pinned official AsyncAPI schema plus source/checksum metadata and
  `just contracts-install`; `just test-contract` now provisions the exact lock,
  runs the validator, and runs the focused Rust contract integration test.
- Added default-off `api_docs = false` configuration and a fixed
  `/api/v1/admin/openapi.yaml` route. When enabled, it requires the Admin cookie and
  serves only the embedded raw YAML; Swagger UI is not present.
- Added configurable `tracing_format = "text" | "json"` (text default), one
  startup subscriber, and request logs limited to request ID, route template,
  method, status, latency, peer IP, authenticated record ID, and bounded Room,
  Match, and Participant IDs. Query strings, bodies, response bodies, cookies,
  Authorization values, passwords, and raw credentials are not logged.
- Added build-script compile-time semver/12-character commit metadata for
  Health and `driichi --version`; current smoke output is
  `driichi 0.1.0 (<12-character commit SHA>)`.
- Kept Health component responses bounded/authenticated and public `/status`
  minimal. Shutdown now records safe cleanup failure categories and invokes
  existing incomplete replay/DB cleanup after Compat and Room shutdown.
- Added focused Health/auth/status/docs/config/metadata/shutdown acceptance
  coverage in `task16_contracts.rs`, including cookie-path-aware Admin docs,
  storage-backed Health 200/503, accepted room DTO fields, exact public status
  and public room lookup comparisons, Human snapshot comparison, build watcher
  probes, and retained-fixture cleanup evidence.
- Recorded the provisional external-evidence ruling and cost-if-wrong in the
  SDD progress ledger.

## Verification

- Strict RED stage: the focused validator initially failed because the exact
  OpenAPI/JSON Schema validator dependencies were absent; after provisioning,
  it caught the invalid AsyncAPI parameter shape and OpenAPI version pattern
  before the green fixes.
- `python scripts/validate_contracts.py` — passed after exact lock install;
  official OpenAPI/AsyncAPI validation, fixture checks, and route inventory all
  passed.
- `cargo test -p double_riichi_server --test task16_contracts -- --test-threads=1` — 12 passed.
- `cargo run -p double_riichi_server -- --version` — passed; printed the
  compile-time semver and short SHA shown above.
- `cargo fmt --all -- --check` — passed.
- `cargo check --workspace` — passed.
- `cargo test --workspace -- --test-threads=1` — passed all workspace unit,
  integration, and doc tests, including Task 16's 12 tests.
- `git diff --check` — passed.
- Frontend checks were not run because no frontend files were touched.
- `just test-contract` remains environment-blocked when `just` is not installed;
  its locked dependency provisioning, validator, and focused Cargo commands were
  run directly and passed.

## Residual risks and parked gates

- The contract validator now has a complete resolver lock, but Python
  packages remain an environment prerequisite; the local exact lock installed
  successfully during this fix round.
- External yamai source and authenticated immutable riichi.dev evidence remain
  unavailable. If authoritative upstream evidence differs, the local contract
  and MJAI adapter may require rework; this is the recorded cost if wrong.
- Real-server 3p/4p Human Playwright, axe, release archives/checksums,
  platform CI, scheduled `test-live`, yamai processing, and final release
  acceptance remain outside this local slice and unclaimed.
- `cargo clippy -p double_riichi_server --all-targets -- -D warnings` remains
  blocked by the same five pre-existing `double_riichi_core` lints
  (type-complexity, two needless-range-loop, too-many-arguments, and
  len-without-is-empty); no Task 16 lint was reported before that baseline
  failure.
