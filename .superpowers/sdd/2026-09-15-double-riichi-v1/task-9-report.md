# Task 9 report — HTTP and Human WebSocket

## Scope delivered

- Added Axum 0.8.9 HTTP/WS server router and `driichi` runtime startup (`--config`, default `config.toml`, `--version`).
- Added Admin login/logout, room list/detail/create/configure/delete, participant selection/deselection/kick, fill/start/rematch/back-to-lobby commands.
- Added public room lookup and atomic Human join issuance with room-scoped HttpOnly/SameSite guest cookies.
- Added Human upgrade-time cookie authentication, audience projection snapshots, authoritative room/game updates, `set_ready`, action result/stale responses, reconnect/disconnect handling, bounded outbound flow, heartbeat, replacement, and semantic close reasons.
- Added RFC Problem Details, server-generated uppercase ULID request IDs, JSON/body and WebSocket limits, rate limiting, trusted-proxy client-IP derivation, strict Origin/Referer checks, and security/cache headers.
- Extended the RoomHandle boundary with participant-scoped projection retrieval and room metadata required by HTTP; no canonical match state is serialized to Human clients.

## Routes/contracts/security matrix

| Surface | Contract/security behavior |
|---|---|
| `/api/v1/admin/login`, `/logout` | strict JSON, same-origin Origin or exact same-origin Referer fallback, HttpOnly/SameSite=Strict cookie, HTTPS `Secure`, fixed session lifetime |
| `/api/v1/admin/rooms*` | Admin cookie only; HTTP mutations; strict JSON and Problem Details; list/detail metadata and RoomHandle commands |
| `/api/v1/rooms/{join_code}` | public metadata only; lookup rate limit and no participant identity leakage |
| `/api/v1/rooms/{join_code}/join` | nickname/Character validation, participant creation rate limit, no raw credential in JSON, Room-bound guest cookie scoped to `/ws/v1/rooms/{join_code}` |
| `/ws/v1/rooms/{join_code}/human` | exact `Origin`, cookie authentication during upgrade, 64 KiB message/frame limit, 20s Ping/60s Pong timeout, 64-message bounded outbound, 1008/1009 and authenticated semantic close reasons |
| all custom responses | generated uppercase ULID `X-Request-ID`, `nosniff`, no-referrer, DENY framing, Permissions-Policy, CSP, API `no-store` |

## TDD evidence

- **RED:** `cargo test -p double_riichi_server --test task9_http` before implementation failed because `ServerState` and `server_router` did not exist (the test also initially exposed a test-only response-borrow error, fixed before implementation).
- **GREEN:** `cargo test -p double_riichi_server --test task9_http -- --nocapture` — 3/3 passing, including Problem Details/request IDs, Admin cookie/room creation/public redaction, and strict unknown-field rejection.

## Verification commands

- `cargo fmt --all -- --check` — passed.
- `cargo check --workspace` — passed.
- `cargo test -p double_riichi_server --tests` — passed (24 tests).
- `cargo test --workspace` — passed (all workspace tests; 100+ tests including Task 3/4/5/6/7/8/9).
- `npm run typecheck --prefix frontend` — passed.

## Commit and residual concerns

Commit: `feat(server): expose admin and human room protocols` (the final commit SHA is recorded by git after this report is committed).

The owner-deferred yamai and authenticated riichi.dev evidence remains deferred; this work does not claim Design Freeze or final external compatibility. Frontend implementation, MJAI/MCP, Replay Admin UI, TLS termination, and other out-of-scope surfaces remain untouched.

## Security review follow-up

- Added atomic room configuration validation/application, permanent-auto projection rejection, canonical Origin matching, Problem Details for method rejection, serialized participant reconnect/disconnect, leave-time guest-session invalidation, projection/event failure closes, bounded writer draining, bounded rate/session stores, configurable production network limits, graceful room shutdown, and deletion acknowledgement before registry removal.
- Added `live_human_upgrade_authenticates_cookie_sends_snapshot_and_replaces_connection`, covering a real TCP WebSocket upgrade, room-bound cookie authentication, snapshot delivery, and replacement close code/reason.
- Focused verification: `cargo fmt --all -- --check`; `cargo test -p double_riichi_server --tests`; `cargo test -p double_riichi_core --tests` — all passed.
- Remaining explicit boundary: no external yamai/riichi.dev compatibility or Design Freeze claim.

## Security review round 2 follow-up

- Shutdown now retries busy room queues, notifies every room before Axum drain, and bounds the combined room/connection drain by the configured shutdown deadline.
- Guest sessions are invalidated on failed leave attempts, periodically reconciled against live room participants, and reconciled before public join/Human upgrade; disconnected expiry and empty-room deletion therefore cannot leave an accepted stale credential.
- Reconnect/register and the connection permit now occur inside Axum's `on_upgrade` callback, so an HTTP upgrade that never completes does not mutate room presence or consume a permit. Participant operation locks reclaim their map entries after the final waiter.
- Rate-limit buckets retain their own configured window; a short-window request cannot prune the 15-minute admin-failure bucket. Human wire values now normalize all protocol enum variants, including nested controller/role values.
- Added regressions for rate-window isolation, lock reclamation, room-reconciled guest sessions, trusted-proxy IP derivation, enum serialization, live Human leave close behavior, and the existing live snapshot/replacement flow.
- Final round-2 verification: `cargo fmt --all -- --check`; `cargo test -p double_riichi_server --test task9_http -- --nocapture`; `cargo test -p double_riichi_server --lib`; `cargo test -p double_riichi_core --tests` — all passed.

## Security review round 3 follow-up

- Shutdown now completes the bounded room notification/retry phase before releasing Axum's graceful-drain gate; the remaining configured deadline bounds connection drain.
- Reconciliation preserves sessions when a room snapshot is transiently busy and only revokes on a successful absence check or closed/deleted room. Leave invalidation and close signalling now hold the participant operation lock, and upgrade callbacks re-authenticate after acquiring that lock, closing the replacement race.
- Protocol normalization is key/context scoped rather than recursively rewriting arbitrary strings; `ConnectionLost` and nested controller/role values are serialized snake-case while display names remain unchanged.
- Expanded live Human coverage to heartbeat Ping, Ready snapshot, match start/stale action rejection, replacement, and semantic Leave close; added bounded outbound-queue and protocol-string regressions alongside trusted-proxy/rate/guest-session/lock tests.
- Final round-3 verification: `cargo fmt --all -- --check`; `cargo test -p double_riichi_server --test task9_http -- --nocapture`; `cargo test -p double_riichi_server --lib`; `cargo test -p double_riichi_core --tests` — all passed.

## Security review round 4 follow-up

- Added explicit DecisionKind protocol mapping (`turn`/`response`) in the context-scoped wire normalizer; the regression now covers nested controller reason, decision kind, role, and preservation of user display strings.
- Extended the live Human test through match setup and stale-action handling while retaining heartbeat, Ready snapshot, replacement, and semantic Leave close assertions; the bounded outbound queue has a hard-capacity regression and existing room slow-connection coverage remains green.
- Final round-4 verification: `cargo fmt --all -- --check`; `cargo test -p double_riichi_server --test task9_http -- --nocapture`; `cargo test -p double_riichi_server --lib`; `cargo test -p double_riichi_core --tests` — all passed.
