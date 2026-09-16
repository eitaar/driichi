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
- Frontend component coverage now produces durable JSON and JUnit artifacts: `frontend/test-results/task11-app-vitest.json` reports 21 total / 21 passed / 0 failed, and `frontend/test-results/task11-app-vitest.junit.xml` reports 21 tests / 0 failures.
- Browser coverage produces `frontend/test-results/task11-playwright.json` and `frontend/test-results/task11-playwright.junit.xml`, with 9 total / 9 passed / 0 failed. Admin and Lobby screenshots were captured at both required desktop viewports and visually inspected.

## Validation commands

- `npm ci --prefix frontend --no-audit --no-fund` — passed; repaired the stale optional native package installation.
- `cargo fmt --all -- --check` — passed.
- `cargo check --workspace` — passed.
- `cargo test -p double_riichi_server --test task9_http` — passed, 10/10.
- `npm run typecheck --prefix frontend` — passed.
- `node node_modules/vitest/vitest.mjs run src/app.test.tsx --reporter=json --outputFile=test-results/task11-app-vitest.json` — passed, 21/21; JSON parsed with nonzero passing count.
- `node node_modules/vitest/vitest.mjs run src/app.test.tsx --reporter=junit --outputFile=test-results/task11-app-vitest.junit.xml` — passed, 21 tests / 0 failures.
- `node node_modules/@playwright/test/cli.js test tests/entry.spec.ts --workers=1 --reporter=json` — passed, 9/9; JSON parsed with nonzero passing count.
- JUnit conversion from the passing Playwright JSON — 9 tests / 0 failures.
- `git diff --check` — passed.

## Visual critique

The UI stays on the approved broadcast-noir foundation: flat split workspace, quiet ink surfaces, lacquer-red state/focus accent, Geist body, Geist Mono labels, sharp controls, actual Character asset paths, and authoritative seat data without a synthetic lobby roster. No dashboard card grid, fake metrics, generic avatar art, extra icon family, GSAP in Admin/Lobby, or gameplay table was added. Desktop CSS keeps the control rail and detail surface usable at 1024x600 and 1440x900; mobile collapses the rail and lobby columns without pretending to support mobile gameplay.

## Commit

Review-fix baseline: `ec4cd34`.
Follow-up coverage/evidence fix: pending commit from `ec4cd34`.

## Concerns

None for the reviewed contracts. The Playwright captures use mocked Admin/Lobby data and intentionally expose the selected-character decode failure state when no character asset server is present; this is visible and keeps Ready disabled.
