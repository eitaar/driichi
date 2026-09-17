# Task 13 report — MJAI compatibility endpoints

## Status

`SUPPORTED_PROVISIONALLY` for the local, evidence-supported MJAI adapter and
server endpoints. The implementation does not claim strict upstream drop-in
compatibility or release acceptance while the external evidence gates below
remain blocked.

## Implemented

- Preserved strict MJAI wire parsing, bounded request/action fields, red-tile
  and `tsumogiri` matching, request/legacy reply tracking, per-request timing,
  observations, and deterministic timeout defaults.
- Exposed authenticated `/ws/ranked`, `/ws/validate`, `/status`, and four-player
  Room MJAI join/upgrade paths with origin, connection, queue, replacement, and
  revocation handling.
- Ranked matches persist a `writing` metadata row and participant records before
  play, finalize the MJSON artifact and result metadata after completion, and
  degrade persistence without aborting the Match when replay/metadata work
  fails. Unfinished writers and metadata are cleaned up.
- Added live acceptance coverage for authentication/origin/revocation, Room
  token ownership and replacement, three-player rejection, ranked MJSON and
  metadata persistence, and validation illegal-action completion.

## Exact verification

All commands below were run from the clean-base worktree after restoring the
preserved patch:

- `cargo test -p double_riichi_server --test task13_compat -- --nocapture` — 4 passed.
- `cargo fmt --all -- --check` — passed.
- `cargo test -p double_riichi_mjai` — 13 passed; doc tests passed.
- `cargo test -p double_riichi_replay` — 15 integration tests passed; doc tests passed.
- `cargo test -p double_riichi_server` — 13 unit tests, 4 Task 13 tests, 10 Task 6 tests, 11 Task 7 tests, and 10 Task 9 tests passed; doc tests passed.
- `cargo check --workspace` — passed.
- `cargo test --workspace` — passed.
- `git diff --check` — passed.

The ranked live test was isolated and rerun after the persistence ordering fix;
the Room live test was isolated and rerun after replacing an invalid Lobby
initial-event wait with a bounded protocol reply check. Repeated focused runs
were deterministic.

## Explicit blockers

1. **yamai blocker:** no authoritative yamai repository/package, immutable
   revision, license, or `ReplayProcessor` API was discoverable. No yamai
   ReplayProcessor execution or compatibility claim is made.
2. **upstream-token blocker:** an accepted authenticated riichi.dev ranked or
   validate Bot Token was unavailable. No production upstream ranked/validate
   transcript, queue transcript, validation transcript, or reconnect evidence
   could be captured.
3. **upstream-pin blocker:** the public riichi.dev pages do not expose an
   immutable Protocol v2 revision/checksum. Exact exhaustive event/action/ack
   schemas, ordering, heartbeat/close behavior, and `/status` schema therefore
   remain unpinned.

These blockers keep Conditional Design Freeze, strict drop-in compatibility,
and release acceptance unresolved; the local tests are evidence for the
provisional implementation only.
