# Task 14 report — MCP resource-driven agent play

## Status

`SUPPORTED_PROVISIONALLY` for the local authenticated MCP server, resource/tool
contract, in-process stdio bridge, and bounded protocol-bot Match. The bridge
keeps the production token contract: `driichi-mcp` reads `DRIICHI_MCP_TOKEN`
only; the explicit token argument is a non-default `test-support` seam used
only by the in-process transport test and is not serialized or logged.

## Implemented

- Added injectable downstream transport wiring while preserving the normal
  `run()`/binary stdio path and environment-only production authentication.
- Added a real Axum `/mcp` integration test using `rmcp::serve_client`, a
  duplex async-RW transport, the bridge's authenticated HTTP upstream, one MCP
  player, `RoomActor`, `MatchMachine`, and built-in opponents.
- The protocol bot discovers and asserts the exact four Tools and three Resource
  Templates, joins once, reads the private/public/history Resources, subscribes
  to all three, checks notification delivery and private-data redaction, waits
  by revision, and submits only returned `action_id` values.
- The bounded Match loop asserts monotonic revisions, request timeouts, turn
  ownership, `game_ended`/Post-Match, and a persisted terminal result. Direct
  Room commands are limited to time-control/setup, selection, bot fill, and
  start.
- Implemented legacy `resources/subscribe` handling for the same resource
  update stream and made decision/terminal transitions advance revisions so
  terminal wake reasons cannot be masked by an earlier round-ended event.
- Added a subprocess CLI regression proving a missing `DRIICHI_MCP_TOKEN` is
  rejected before an upstream connection is attempted.

## Verification

- `cargo fmt --all -- --check` — passed.
- `cargo test -p double_riichi_server --test task14_mcp` — 5 passed.
- `cargo test -p double_riichi_server --test task14_mcp live_mcp_bridge_protocol_bot_completes_resource_driven_match -- --exact` — passed; bounded bridge Match completed in under 10 seconds.
- `cargo test -p double_riichi_mcp` — 2 unit tests, 1 CLI integration test, and doc tests passed.
- `cargo clippy -p double_riichi_mcp --tests --features test-support -- -D warnings` — passed.
- `cargo check --workspace` — passed.
- `cargo test --workspace` — passed across all workspace unit, integration, and doc-test targets.
- `git diff --check` — passed.

Strict server/workspace Clippy remains blocked by pre-existing baseline lints
in core/replay and existing server modules; the focused MCP package Clippy run
is clean. No external OAuth or upstream compatibility claim is made.

## Commit

`6c04978 test(mcp): prove resource driven agent play`
