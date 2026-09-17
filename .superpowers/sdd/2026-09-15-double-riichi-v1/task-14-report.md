# Task 14 report — MCP resource-driven agent play

## Status

`SUPPORTED_PROVISIONALLY` for the local authenticated MCP server, resource/tool
contract, in-process stdio bridge, and bounded protocol-bot Match. The bridge
keeps the production token contract: `driichi-mcp` reads `DRIICHI_MCP_TOKEN`
only; the explicit token argument is a non-default `test-support` seam used
only by the in-process transport test and is not serialized or logged.

## Review evidence refreshed from `e1b9883`

- Added a live join barrier regression: `join_room` does not return until the
  watcher has consumed the initial snapshot, so an immediate revision-zero
  `wait_for_turn` returns the initial `deselected` wake rather than timing out.
- Strengthened the real stdio bridge Match to use a multi-Kyoku FourPlayer
  Half room and assert the Post-Match history envelope retains bounded current
  Kyoku events plus prior summaries, while private fields remain redacted.
- Added deterministic terminal-wake coverage for equal-revision
  `server_shutdown` waits and all three shutdown resource notification URIs.
  The bridge now also observes all three shutdown notifications end to end.
- Added unjoined transport cap+1 coverage through the bounded session manager,
  including permit recovery after closing the cap-sized set.
- Added repeated legacy subscribe/unsubscribe cycles in the real bridge and
  semaphore exhaustion/recovery coverage for subscription permits.
- Existing core `TemporaryAuto` turn/response tests were rerun; both prove
  immediate follow-up decisions after timeout for Casual and Riichi.dev timing.

## Rereview 1/4 fixes

- Closed the `RevisionWake` lost-wakeup window by enabling the notification
  future before the final revision check.
- Classify only the subscription's initial snapshot; later snapshots are
  ignored so unrelated Room commands and normal opponent discards do not wake
  waits or resource subscriptions.
- Serialize MCP token revocation with the join transition and retain a
  revocation tombstone so stale in-flight transports and joins cannot bind.
- Lease active `wait_for_turn` calls across idle reaping and refresh the
  session when the wait completes.
- Added focused regressions for all three state/ownership lifecycle fixes and
  post-initial snapshot filtering.

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
- `cargo test -p double_riichi_server mcp::tests -- --nocapture` — 16 passed.
- `cargo test -p double_riichi_server --test task14_mcp -- --nocapture` — 6 passed.
- `cargo test -p double_riichi_server --test task14_mcp live_mcp_bridge_protocol_bot_completes_resource_driven_match -- --exact` — passed; complete stdio bridge Match and shutdown notifications completed within the bounded test.
- `cargo test -p double_riichi_core --test task4_machine temporary_auto -- --nocapture` — 3 passed.
- `cargo clippy -p double_riichi_mcp --tests --features test-support -- -D warnings` — passed.
- `cargo check --workspace` — passed.
- `git diff --check` — passed.

The workspace test command was run. Two unrelated pre-existing Task 13 live
compat tests (`live_validate_reports_illegal_action_but_completes_match` and
`live_ranked_bot_completes_and_persists_mjson_metadata`) failed to observe their
external live `end_game` messages in this environment; Task 14 focused tests
and all local workspace targets passed. Strict server/workspace Clippy remains
blocked by documented pre-existing core/replay/server baseline lints. No
external OAuth or upstream compatibility claim is made.

## Commit

`fix(mcp): close Task 14 rereview findings`
