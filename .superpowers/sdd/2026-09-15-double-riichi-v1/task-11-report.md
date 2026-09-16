# Task 11 report: Admin and Lobby UI

## Routes, state, and data flow

- `/admin/login` keeps the Task 10 Admin login form and hands off to `/admin`.
- `/admin` and `/admin/rooms/{join_code}` use TanStack Query keys `['admin','rooms']`, `['admin','room', join_code]`, and `['admin','tokens']`. Room detail uses `refetchInterval: 2000` with `refetchIntervalInBackground: false`; every Room mutation invalidates the exact Room key and the Room list key.
- Admin provides Room list, create/open, settings PATCH, participant select/deselect/kick, Fill with Bots, start, back to Lobby, delete confirmation, seat rail, Character display, and lifecycle state. Loading, empty, disabled, and Problem Details states are visible in each async surface.
- `/room/{join_code}/lobby` opens the Task 9 Human WebSocket using the HttpOnly Guest cookie. The participant ID is the non-secret identity key stored in sessionStorage for reload handoff; the cookie is never read by JavaScript. Snapshots and room updates replace the central Room state. Identity, presence, selection, ready, and controller are rendered as separate axes.
- Human Lobby reconnects with bounded jittered backoff, handles `connected_elsewhere`, `room_deleted`, `session_expired`, and slow-consumer states, renders Lobby, Playing, and Post-Match lifecycle phases, and intentionally leaves Pixi gameplay to Task 12.
- Ready sends `set_ready` only when the complete selected roster count matches the 3p/4p seat count and every selected Character portrait/icon preload succeeds.

## Bot Token contract completion

Task 9 had the storage/auth primitives but no Admin HTTP resource. This task adds the minimal native routes:

- `GET /api/v1/admin/tokens`
- `POST /api/v1/admin/tokens` with `{name}`
- `POST /api/v1/admin/tokens/{token_id}/revoke`

`ServerState` loads `BotTokenAuthority` from SQLite at startup and wires `BotTokenService`. Successful revoke calls `RoomRegistry::revoke_token` after the durable commit, so active Room Participants receive the existing global token signal. List responses contain only `token_id`, name, state, and timestamps.

The raw `driichi_...` value is serialized only in the create response. The frontend keeps it in local React state for the open dialog, does not put it in local/session storage, URL, query data, logs, or copied page state, and clears it on dialog close. Copy is user initiated. The HTTP test proves list redaction and irreversible revoke.

## TDD evidence

- RED: Added failing Admin workspace, Token lifecycle, Human WebSocket, preload/Ready, and browser route tests before implementing Task 11 behavior. The inherited MSYS Node installation contains Windows-only npm binaries, so the Vitest/Playwright wrappers exited without producing runner output; this is recorded as a validation concern rather than treated as test evidence.
- GREEN: `cargo test -p double_riichi_server --test task9_http` passed 10/10, including the new one-time Token HTTP lifecycle test. Rust workspace tests passed before the command timeout reached the later long-running server suite; the focused complete Task 9 suite then passed 10/10.

## Validation commands

- `cargo fmt --all -- --check` passed.
- `cargo check --workspace` passed.
- `cargo test -p double_riichi_server --test task9_http` passed 10/10.
- `cargo test --workspace` ran successfully through core, replay, Task 6, Task 7, Task 8, and most server suites before the 120-second shell timeout; the complete Task 9 integration suite passed separately in 23.86 seconds.
- `npm install --prefix frontend --ignore-scripts --no-audit --no-fund` passed and lockfile includes exact `@tanstack/react-query` 5.90.3.
- `npm run typecheck --prefix frontend`, `npm test --prefix frontend -- --reporter=dot`, and `npm run test:browser --prefix frontend -- --workers=1` returned zero from the inherited wrapper but emitted no runner output or new screenshots because the MSYS node_modules binaries target Windows. Existing Task 10 screenshots remain intact; new Admin/Lobby screenshot capture is an open environment concern.
- `git diff --check` passed.

## Visual critique

The UI stays on the approved broadcast-noir foundation: flat split workspace, quiet ink surfaces, lacquer-red state/focus accent, Geist body, Geist Mono labels, sharp controls, actual Character asset paths, and a seat rail that carries real selected-roster state. No dashboard card grid, fake metrics, generic avatar art, extra icon family, GSAP in Admin/Lobby, or gameplay table was added. Desktop CSS keeps the control rail and detail surface usable at 1024x600 and 1440x900; mobile collapses the rail and lobby columns without pretending to support mobile gameplay.

## Commit

Commit: `feat(frontend): add admin and lobby flows` (the report is included in this commit).

## Concerns

1. Frontend component/browser runners need to be rerun in a native Node installation because this worktree's MSYS environment resolves Windows-only optional binaries and produces no test/screenshot output.
2. The Bot Token HTTP paths were absent from the supplied Task 9 implementation/spec fixture, so the resource-style `/api/v1/admin/tokens` path and `/{token_id}/revoke` command path were added as the narrow native contract matching existing Admin room command conventions.
