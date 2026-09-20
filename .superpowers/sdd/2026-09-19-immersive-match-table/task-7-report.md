# Task 7 Implementation Report

## Scope

Completed the immersive Live Match/Replay visual, accessibility, reduced-motion, synchronization, terminal-state, and viewport acceptance matrix for the approved 16:9 Pixi table surfaces.

- Base: `2bec73e7cb77add95fc6130d850d76b8a52ef8f8`
- Head: `eb2f5ef` (`eb2f5efc1be9709be017f35d8eec7f2329c8be86`)
- Commit: `eb2f5efc1be9709be017f35d8eec7f2329c8be86 test(frontend): verify immersive table acceptance matrix`
- Review package: `/c/Users/eitab/Documents/js/driichi/.worktrees/double-riichi-v1/.superpowers/sdd/2026-09-19-immersive-match-table/review-2bec73e..eb2f5ef.diff`

## TDD evidence

### RED

The new acceptance coverage exposed concrete issues during implementation:

- Synchronization copy rendered only inside the Pixi canvas, so the DOM contract could not find it.
- The 4p fixture exposed only one Chi candidate, so the candidate-dialog keyboard path could not be exercised.
- The real-server test still used the removed `.score-row` selector.
- Axe reported `aria-prohibited-attr` on the hand hit layer.
- The first real-server browser run exposed a result-broadcast race: a later accepted `action_result` replaced the submitted action id before the exact-id assertion read the DOM.

### GREEN

- `npx playwright test tests/task12.spec.ts tests/task15.spec.ts`: 27 passed.
- Focused real-server validation after the race fix: `npx playwright test tests/task16-real-server.spec.ts --config=playwright.real.config.ts --grep "real 4p-red-east" --reporter=line`: 1 passed in 4.0m.
- The previously approved real-server matrix completed 3 tests in 5.3m after the acceptance-only 240s match deadline / 360s test timeout adjustment.

## Implementation details

- Expanded Task 12, Task 15, and real-server viewport matrices to `1024x600`, `1280x720`, `1600x900`, and `1920x1080`.
- Added exact 3p/4p wall counts (`54`/`69`), canvas instrumentation, 16:9/size checks, player-frame checks, local-hand and Action-row checks, legal-hit viewport checks, long-name/missing-portrait fallback coverage, and required screenshots.
- Added reduced-motion coverage that verifies static animation state and no visible animation marker.
- Added Axe checks for Settings, candidate dialog, gameplay, Replay, and floating controls, plus keyboard focus paths for Settings, candidate selection, and Replay transport controls.
- Added DOM synchronization status with `role="status"`, while keeping the Pixi canvas free of duplicate synchronization copy.
- Added terminal Room-deleted blocking coverage and preserved the gameplay surface for its alert instead of navigating away.
- Added `role="group"` to the hand hit layer for Axe compatibility.
- Hardened real-server action-result waiting against authoritative broadcasts replacing the just-submitted action id: an alternate accepted result is accepted only when the room revision has advanced beyond the submission revision.
- Kept reduced-motion animation state explicitly idle and instrumented normal animation state for deterministic assertions.

## Validation

### Frontend gate

```bash
cd frontend
npm run typecheck
```

PASS: `tsc --noEmit`.

```bash
npm test -- --run
```

PASS: 4 files, 47 tests.

```bash
npm run build
```

PASS: Vite production build.

```bash
npm run test:browser
```

Final run: 38 passed, 1 failed. All Task 12, Task 15, embedded-gameplay, and both real-server tests passed. The sole failure was the existing `tests/entry.spec.ts` transient loading assertion: after the intentionally delayed 250ms mocked 404 resolves, `getByText(/loading room/i)` was no longer present when the assertion ran. The failure was not in the immersive table surfaces. I did not repeatedly rerun this flaky full gate.

### Repository checks

```bash
just fmt-check
just test-frontend
just test-spec
```

The configured environment does not provide `just` (`/usr/bin/bash: just: command not found`). Equivalent checks were run directly:

- `cargo --locked fmt --all -- --check`: PASS.
- `npm ci --prefix frontend`: PASS after removing a stale locked `rolldown` native binary left by the failed first install attempt.
- `npm run typecheck`, `npm test -- --run`, and `npm run build`: PASS as recorded above.
- The `test-spec` assertions were run directly and passed (`test-spec equivalent: PASS`).

### Screenshot inspection

Manually inspected every generated image under:

- `frontend/test-results/task-12/` (all 9 PNGs)
- `frontend/test-results/immersive-table/` (both required PNGs)

The 3p captures show exactly three frames and 54 tiles; the 4p captures show exactly four frames and 69 tiles. Across all four required viewports, the 16:9 table, local hands, legal hits, Action row, and player surfaces remain visible without clipping or texture dominance. The long-name fallback remains actionable, and the reduced-motion capture has no animated marker.

## Files changed

- `frontend/src/app.tsx`
- `frontend/src/game/gameplay.tsx`
- `frontend/src/game/pixi-table.tsx`
- `frontend/src/styles.css`
- `frontend/tests/fixtures/task12-projection.json`
- `frontend/tests/task12.spec.ts`
- `frontend/tests/task15.spec.ts`
- `frontend/tests/task16-real-server.spec.ts`

## Self-review

- Preserved React/Pixi/Zustand ownership and the shared Live Match/Replay Pixi scene.
- No backend, protocol, projection, domain, Three.js, or new runtime dependency changes.
- Kept the approved 16:9 table composition without restoring a top gameplay bar or right rail.
- Kept wall rendering bounded by the fixture's `remaining_wall` count; no dead-wall geometry is implied.
- Preserved action ids, pending behavior, retry behavior, candidate focus return, and no-optimistic-update coverage.
- Verified 1024x600 gameplay and the below-supported-viewport guidance path remain covered.
- Verified generated-texture fallback, Character fallback, reduced motion, Replay generic/silent behavior, synchronization, and Room-deleted blocking coverage.
- `git diff --check` passed; only the existing LF-to-CRLF warnings for browser fixtures/specs were reported.

## Concerns

1. The first clean-state full browser attempt exposed a realtime action-locator race; it was fixed in `78573f2` and the final full browser gate passed 39/39.
2. The repository-level `just` commands could not be invoked because `just` is not installed in this environment; their direct equivalents passed where available.

## Validation takeover final evidence

Validation started from the clean `d4c3229` worktree. The existing Task 7 implementation/fix commits were retained unchanged:

- `4c21255` — `test(frontend): close Task 7 acceptance findings`
- `af021be` — `fix(frontend): preserve accepted action identity`
- `d4c3229` — `test(frontend): retry rejected realtime actions safely`

The first clean-state full browser run reproduced one concrete realtime test failure:

```bash
cd frontend
npm run test:browser
```

Result: 38 passed, 1 failed in 6.5m. The 4p real-server test captured action id `a328`, then a generic Playwright `Pass` locator waited about 60 seconds while the realtime Decision changed and clicked `a448`; the exact-id result wait correctly rejected the stale `a328` assertion. This was a test synchronization failure, not a gameplay-surface failure.

The corrective validation commit was:

- `78573f2` — `test(frontend): pin realtime action clicks to captured ids`

It pins action clicks to the captured `data-action-id`, bounds transient actionability waits, and skips a stale action instead of clicking a newer Decision. Evidence after the fix:

```bash
cd frontend
npx playwright test tests/task16-real-server.spec.ts --config=playwright.real.config.ts --grep "real 4p-red-east" --reporter=line
```

PASS: 1 passed in 4.3m.

```bash
cd frontend
npx playwright test tests/task12.spec.ts tests/task15.spec.ts --reporter=line
```

PASS: 27 passed in 1.7m.

```bash
cd frontend
npm run typecheck
npm test -- --run
npm run build
```

PASS: typecheck, Vitest 4 files / 47 tests, and production build.

The final post-fix full browser gate was rerun to verify the concrete failure was closed:

```bash
cd frontend
npm run test:browser
```

PASS: 39 passed in 8.5m, including both real-server 3p and 4p matches.

The required repository commands were attempted without installing tools:

```bash
just fmt-check
just test-frontend
just test-spec
```

All three are unavailable in this environment (`just: command not found`, exit 127). Direct equivalents already pass:

- `cargo --locked fmt --all -- --check`: PASS.
- `npm run typecheck`: PASS.
- `npm test -- --run`: PASS, 47/47.
- `npm run build`: PASS.
- The four exact `test-spec` assertions from `justfile`: PASS.

`git diff --check`: PASS. The existing screenshot review remains complete for all 9 `frontend/test-results/task-12/` PNGs and both required `frontend/test-results/immersive-table/` PNGs; the final browser runs regenerated the same required evidence without any visual-surface changes.

## Follow-up P2 validation

Added positive non-zero Replay viewer assertions in `frontend/tests/task15.spec.ts` for `data-rendered-tile-count`, `data-rendered-table-primitives`, and `data-rendered-visual-primitives`, alongside the existing `data-render-ready` assertion.

```bash
cd frontend
npx playwright test tests/task15.spec.ts --reporter=line
```

PASS: 8 passed in 49.7s across all four Replay viewports and both Replay cases.

## Broad-review recovery evidence

Recovery preserved the eleven pre-existing uncommitted frontend files on base `b24d4e401a75152b0c82f6bf940c5e9dd399015c`; no reset or restart was performed.

### RED

The focused reconnect/preload contract failed before the recovery edit:

```text
$ cd frontend && npx vitest run src/app.test.tsx -t "repeats selected asset preload for a fresh connection generation" --reporter=verbose
FAIL: expected 6 to be 12
```

The failure was caused by preserving the authoritative lobby Room during reconnect without rerunning the selected-character preload effect for the new `connectionGeneration`. The expectation remained justified: a fresh WebSocket generation must decode the selected portrait and icon assets again before Ready can be sent.

### GREEN / focused corrections

- Added `connectionGeneration` to the lobby preload effect dependencies; the focused test now passes with image counts `6` then `12`.
- Recoverable close/error states retain the last Room/projection scene; terminal close reasons (`connected_elsewhere`, `room_deleted`, `server_shutdown`, `slow_consumer`, `token_revoked`, `session_expired`) preserve the scene and route gameplay to blocking UI, while `network_error` remains transient.
- Shared `wallTileCount()` now bounds numeric and array center labels exactly as the rendered wall, including non-finite values; added focused unit assertions.
- Kept the viewer hand at the larger actionable frame and opponent hands at the smaller frame/gap.
- Candidate buttons are disabled with the rest of action input while disconnected/pending; failed sends clear pending state; multi-candidate triggers expose the dialog relationship and the chooser traps Tab focus with Escape/focus restoration.
- Replay transport bounds now have explicit 1920×1080 child/control containment assertions.

Focused unit validation:

```text
$ cd frontend && npm test -- --run src/game/table-geometry.test.ts src/game/task12.test.ts src/replay.test.tsx
PASS: 3 files, 28 tests

$ cd frontend && npx vitest run src/app.test.tsx -t "repeats selected asset preload for a fresh connection generation" --reporter=verbose
PASS: 1 test (20 skipped)
```

Focused browser validation:

```text
$ cd frontend && npx playwright test tests/task12.spec.ts --grep "candidate popup|disables open candidate|preserves the last table scene|keeps full player status|blocks play when the Room is deleted|captures 4p-red-east authoritative gameplay at 1920x1080" --reporter=line
PASS: 6 tests

$ cd frontend && npx playwright test tests/task15.spec.ts --grep "captures Replay library and viewer at 1920x1080" --reporter=line
PASS: 1 test
```

The 1920×1080 Replay screenshot was inspected at `frontend/test-results/task-15/viewer-1920x1080.png`; the complete Play/Previous/Next/speed/Kyoku transport is inside the stage and viewport with no scroll clipping.

Affected acceptance lanes:

```text
$ cd frontend && npm run typecheck && npm test -- --run && npm run build
PASS: typecheck; Vitest 4 files / 49 tests; Vite production build

$ cd frontend && npx playwright test tests/task12.spec.ts tests/task15.spec.ts --reporter=line
PASS: 30 tests in 1.7m

$ cd frontend && npx playwright test tests/task16-real-server.spec.ts --config=playwright.real.config.ts --reporter=line
PASS: 3 tests in 6.8m (embedded CSP, real 3p, real 4p)
```

### Final validation

The required single full browser gate was run only after the focused checks above were green:

```text
$ cd frontend && npm run test:browser
PASS: 42 tests in 7.2m
```

`git diff --check` passed (only existing LF→CRLF conversion warnings for the modified frontend text specs/modules). Backend/domain/projection/protocol files were untouched; action `decision_id`/`action_id` semantics and no-optimistic-update behavior remain covered.
