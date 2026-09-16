# Task 11 report: Admin and Lobby UI

## Routes, state, and data flow

- `/admin/login` keeps the Task 10 Admin login form and hands off to `/admin`.
- `/admin` and `/admin/rooms/{join_code}` use TanStack Query keys `['admin','rooms']`, `['admin','room', join_code]`, and `['admin','tokens']`. Room detail uses `refetchInterval: 2000` with `refetchIntervalInBackground: false`; every Room mutation invalidates the exact Room key and the Room list key.
- Admin provides Room list, create/open, settings PATCH, participant select/deselect/kick, Fill with Bots, start, Rematch, back to Lobby, delete confirmation, seat rail, Character display, and lifecycle state. Loading, empty, disabled, and Problem Details states are visible in each async surface.
- `/room/{join_code}/lobby` opens the Task 9 Human WebSocket using the HttpOnly Guest cookie. The participant ID is the non-secret identity key stored in sessionStorage for reload handoff; the cookie is never read by JavaScript. Snapshots and room updates replace the central Room state. Identity, presence, selection, ready, controller, and authoritative seat are rendered as separate axes.
- Human Lobby reconnects with bounded backoff, handles `connected_elsewhere`, `room_deleted`, `session_expired`, and slow-consumer states, and lifecycle callbacks/timers are guarded by connection generation and room ownership. Command errors remain action errors and do not mark a live transport disconnected. Successful open clears the current-generation reason.
- Ready invalidates the roster preload on every authenticated connection and sends `set_ready` only when the complete selected roster count matches the 3p/4p seat count and every selected Character portrait/icon has loaded and decoded.
- Modals use the native dialog lifecycle when available, with a deterministic initial focus target, Escape cancellation, and focus return to the opening control.

## Bot Token contract completion

Task 9 had the storage/auth primitives but no Admin HTTP resource. This task adds the minimal native routes:

- `GET /api/v1/admin/tokens`
- `POST /api/v1/admin/tokens` with `{name}`
- `POST /api/v1/admin/tokens/{token_id}/revoke`

Successful revoke calls `RoomRegistry::revoke_token` after the durable commit. A repeated `AlreadyRevoked` request retries Room signaling without changing the irreversible SQLite/cache state; Room-side revocation is idempotent, so an earlier delivery failure can be retried. List responses contain only `token_id`, name, state, and timestamps.

The raw `driichi_...` value is serialized only in the create response. The frontend keeps it in local React state for the open dialog, does not put it in local/session storage, URL, query data, logs, or copied page state, and clears it on dialog close. Copy is user initiated. The HTTP test proves list redaction, active-seat controller transfer, and repeat revoke delivery.

## TDD evidence

- Added focused regressions for authoritative seats, every Admin mutation family, Rematch, command rejection ownership, connection-generation stale callbacks, reconnect asset preload, decode failures, and modal focus lifecycle.
- Rust HTTP focused coverage passed 10/10. Frontend TypeScript passed. Vitest could not execute in this inherited MSYS environment: direct `node node_modules/vitest/vitest.mjs run src/app.test.tsx --reporter=verbose` terminated with segmentation fault (exit 139), so it is not claimed as passing evidence.

## Validation commands

- `cargo fmt --all -- --check` — passed.
- `cargo check --workspace` — passed.
- `cargo test -p double_riichi_server --test task9_http` — passed, 10/10.
- `npm run typecheck --prefix frontend` — passed.
- `npm run build --prefix frontend` returned zero from the inherited wrapper without Vite output; direct Vite startup segfaulted (exit 139), so no frontend build is claimed.
- `node node_modules/vitest/vitest.mjs run src/app.test.tsx --reporter=verbose` — environment failure, exit 139 (segmentation fault).
- `node node_modules/@playwright/test/cli.js test tests/entry.spec.ts --workers=1` — environment failure: configured webServer exited early. Direct `node node_modules/vite/bin/vite.js --host 127.0.0.1` also exited 139 (segmentation fault). No browser pass or new screenshots are claimed.
- `git diff --check` — passed.

## Visual critique

The UI stays on the approved broadcast-noir foundation: flat split workspace, quiet ink surfaces, lacquer-red state/focus accent, Geist body, Geist Mono labels, sharp controls, actual Character asset paths, and authoritative seat data without a synthetic lobby roster. No dashboard card grid, fake metrics, generic avatar art, extra icon family, GSAP in Admin/Lobby, or gameplay table was added. Desktop CSS keeps the control rail and detail surface usable at 1024x600 and 1440x900; mobile collapses the rail and lobby columns without pretending to support mobile gameplay.

## Commit

Pending review-fix commit from `5516c35`.

## Concerns

1. Frontend component/browser runners and required Admin/Lobby screenshots need a native Node installation because this worktree's MSYS Node environment segfaults when loading Vitest/Vite and Playwright's configured web server exits early.
2. Browser visual evidence is intentionally not marked as passing; rerun the required Admin and Lobby captures at 1024x600 and 1440x900 after the environment is repaired.
