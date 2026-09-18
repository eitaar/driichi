# Task 16 E2E lane — real-server recovery

## RED / recovery evidence

- The prior async runner (PID 11696) disappeared at 69m49s. Its coherent dirty
  worktree was preserved; no reset, checkout, stash, or discard was performed.
- The preserved recovery evidence records isolated real-server passes before the
  runner disappeared: 3p Human `1.1m`, 4p Human `1.4m`.
- The prior accessibility failures were production rendering defects, not
  assertion relaxations: the app token panel now has an explicit labelled
  region, low-contrast state text uses the shared muted token, and Pixi scene
  updates are coalesced on animation frames.

## GREEN / exact verification

All commands ran from `frontend/` unless noted:

- `npm run test:browser:real` — **2 passed (2.0m)**, one worker:
  - `completes a real 3p-red-east Human match through Post-Match` — **51.7s**
  - `completes a real 4p-red-east Human match through Post-Match` — **57.8s**
  - axe checks passed on Admin, Room, Lobby, and Post-Match surfaces; reporter
    ended with `HTML clean · 4564ms`.
- `npm run typecheck` — passed (`tsc --noEmit`).
- `npm test -- --run src/app.test.tsx src/game/task12.test.ts src/replay.test.tsx`
  — **3 files passed, 41 tests passed**, Vitest 5.0.1, 93.58s.
- `npm run build` — passed; TypeScript build and Vite production build
  transformed 5,409 modules and completed in 1.45s.
- `git diff --check` — passed.

## Worktree hygiene

Removed generated debris from the commit/worktree: root `.vitest/`, root
`node_modules/`, `frontend/playwright-real-report/`, and `frontend/test-results/`.
The ignored `frontend/node_modules/` remains available as the local dependency
installation. No infrastructure-only combined-suite flake occurred, so no
assertion was weakened and no retry was performed.

Only owned `frontend/**` changes and this lane report are intended for the
commit. External yamai/riichi.dev, release, and Conditional Design Freeze gates
remain outside this lane and are not claimed here.
