# Desktop UI Migration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Delegation requires separate authorization. Steps use checkbox (`- [ ]`) syntax for tracking.

**Status:** Draft for user review. No implementation authorized by this document.

**Goal:** Apply the reviewed desktop prototypes to real Room, match, Replay and Admin flows without replacing authoritative data or existing capabilities.

**Architecture:** Keep the existing React views, server APIs, state stores and shared ThreeTable renderer. Port presentation and narrowly scoped interaction changes rather than copying prototype pages or their sample state. Migrate independently testable surfaces, with an explicit stop at any missing server contract.

**Tech Stack:** Existing React 19, TypeScript, Three.js/R3F, vanilla CSS, Vitest and Playwright. No new runtime dependencies.

**Spec:** Root `DESIGN.md`; domain terminology in `CONTEXT.md`; `docs/adr/0014-use-react-three-fiber-for-shared-match-table.md`. Visual evidence lives under `.worktrees/prototype-visual-language/frontend/src/game/prototype-*`; those files are references, not production entry points.

## Global Constraints

- Desktop targets: 1024×600, 1440×900, 1920×1080. Smartphone polish is explicitly deferred; do not intentionally regress existing narrow-screen access.
- UI and accessible labels remain English. User-entered names remain unmodified.
- Preserve routes, sessions, permissions, reconnect behavior and server-issued legal action IDs.
- Do not copy sample players, scores, wall contents, synthetic replay events, preview timers or preview asset URLs into production.
- Character art and voices remain licensed runtime Character Pack assets. Never commit local third-party files or screenshots containing them.
- Keep server scoring and match advancement authoritative. UI animation completion is not permission to advance a match.
- Appearance approval is not complete functional acceptance. Multiple winners, draws, ties, three-player matches and missing data require explicit verification.
- Preserve all existing Admin functionality, including actions omitted from the simplified prototype.
- No deploy, runtime copy, commit of unrelated changes, or blanket worktree merge as part of migration.

## Review Focus

1. Stale decisions and double activation must never submit twice or use obsolete action IDs (Task 2).
2. Hidden opponent tiles must stay hidden in DOM, WebGL, glow and accessibility output (Tasks 2 and 4).
3. Reconnect or missing result facts must not invent scores or restart an acknowledged result sequence (Task 3).
4. Missing Character Packs and generic Replay sources must retain readable identities and permitted silent fallback (Tasks 1 and 4).
5. Admin errors, unready players, token secrets and destructive confirmation must retain existing safeguards (Task 5).

## Before execution

- [ ] Inspect root and prototype worktree status. Root baseline at planning time: `9281895`; multiple unrelated Rust/server and frontend edits already exist.
- [ ] Agree on execution workspace. Do not reset, stash, cherry-pick or overwrite those edits automatically. Use a reviewed snapshot of the current intended production baseline, not the old prototype branch base.
- [ ] Run baseline `npm run typecheck`, `npm test`, `npm run build` from `frontend/`; classify pre-existing failures before touching code.
- [ ] Record reference captures locally at desktop target sizes; do not upload restricted artwork.

## Task 1: Entry, character selection and Lobby

**Files:** `frontend/src/app.tsx`, `frontend/src/styles.css`, `frontend/src/app.test.tsx`, `frontend/tests/task12.spec.ts`.

**Consumes:** Existing Room join/session, character registry and ready-state handlers in `app.tsx`.
**Produces:** The same route/component contracts and API calls with reviewed pre-match presentation.

- [ ] Add tests preserving invalid room-code errors, real registry selection, display-name retention, voice-preview failure, pending join and readiness updates.
- [ ] Run `npm test -- src/app.test.tsx`; confirm new presentation assertions fail on the current view, without weakening existing functional assertions.
- [ ] Apply Entry/Character/Lobby layout and styles from their prototypes, retaining real API loading/error paths and legal asset resolution. No demo code `246810` validation.
- [ ] Run the same tests, `npm run typecheck`, and `npm run test:browser -- tests/task12.spec.ts`; inspect the three desktop widths and unavailable-asset state.

## Task 2: Shared table and direct discard

**Files:** `frontend/src/game/three-table.tsx`, `three-table-scene.tsx`, `three-table-layout.ts`, `tile-atlas.ts`, `table-hit-targets.ts`, `gameplay.tsx`, their existing tests, and `frontend/src/styles.css`.

**Consumes:** Existing projected state, legal actions and shared Live/Replay table props.
**Produces:** The existing ThreeTable interface with 2D local hand, 3D table elements and data-driven WebGL center. No preview contexts in production.

- [ ] Add tests for single click/Enter submission using the current legal action, pending disable, expired decision and ambiguous action choices. Preserve required call/riichi selection rather than treating every tile activation as an unconditional discard.
- [ ] Add layout tests for four/three players, varying meld counts, real riichi declaration position, dora wraps and concealed-information protection. Port the useful prototype docking/spacing assertions, not its fixed sample wall.
- [ ] Run `npm test -- src/game/three-table.test.tsx src/game/gameplay.test.tsx` and the layout test files selected above; confirm the intended new assertions fail.
- [ ] Transfer geometry, camera fitting, atlas sharpness and local-hand presentation into their existing owners. Derive center scores/winds/round/remaining count from real state, never prototype constants. Keep production wall and declaration semantics.
- [ ] Run affected unit tests, typecheck and `npm run test:browser -- tests/task12.spec.ts`. Verify resize pixels, hand/call overlap and keyboard actions at the desktop widths. Existing intermittent resize concern must not be declared fixed merely because one run passes.

## Task 3: Win, transfer and final standings

**Files:** `frontend/src/game/gameplay.tsx`, `frontend/src/game/gameplay.test.tsx`, `frontend/src/styles.css`, `frontend/tests/result-surfaces.spec.ts`.

**Consumes:** Existing `RoundWinEffect`, `RoundWinSurface`, result parsing and Room result data.
**Produces:** Reviewed result composition over authoritative existing data. Any new server contract is a separate reviewed change.

- [ ] First inventory available winner hands, melds, yaku, han/fu, awarded points, score deltas, ordering and acknowledgement/advance APIs against Wayfinder #10 and #12. Stop this task if data or synchronization is missing; do not synthesize it in CSS or client heuristics.
- [ ] Add tests for missing facts, ron/tsumo, multiple winners, draw, ties, three players, missing portrait and reconnect. The correct fallback is omission or an explicit unavailable state, not a fabricated value.
- [ ] Add fake-time tests: signed zero and original totals at start; final values at 3 seconds; 2-second hold; Continue skips remaining presentation; reduced motion shows final values immediately. Verify that client timers alone cannot advance server state.
- [ ] Run `npm test -- src/game/gameplay.test.tsx` to observe new failures.
- [ ] Apply left portrait/right hand-yaku-points composition and final left portrait/right rank-name-score list. No duplicate name beside the final portrait. Keep existing generic and missing-asset fallbacks.
- [ ] Run unit tests and `npm run test:browser -- tests/result-surfaces.spec.ts`; obtain desktop visual acceptance before proceeding past any newly exposed result cases.

## Task 4: Replay

**Files:** `frontend/src/replay.tsx`, `frontend/src/replay.test.tsx`, `frontend/src/styles.css`, `frontend/tests/task15.spec.ts`.

**Consumes:** Real `ReplayView.frames`, `visible_state`, existing `ReplayViewer`, asset and audio behavior.
**Produces:** Bottom playback controls on the same production table: play/pause, previous/next, seek, speed and Kyoku selection.

- [ ] Add tests for own-hand discard/river consistency when seeking forward and backward, first/last frame boundaries, empty timeline, unavailable assets and generic/silent sources.
- [ ] Add speed tests for 0.5/1/2/4/8/16/32/64 and endpoint stop. At high rates, visual and audio scheduling must not build an unbounded queue.
- [ ] Test Kyoku selection across East/South and repeated honba; the current numeric `kyoku` comparison is not a sufficient unique key. Derive destinations from real round boundaries rather than inventing IDs.
- [ ] Run `npm test -- src/replay.test.tsx` and observe failures for the requested new behavior.
- [ ] Restyle existing viewer and add native range/select controls, preserving real frame projections, saved-event timing policy and audio permissions. Do not import synthetic prototype events or its one-second timer as the production replay contract.
- [ ] Run unit tests and `npm run test:browser -- tests/task15.spec.ts`; verify desktop resize and seeking visually.

## Task 5: Admin

**Files:** `frontend/src/app.tsx`, `frontend/src/app.test.tsx`, `frontend/src/styles.css`; add `frontend/tests/admin-visual.spec.ts` using existing request-mocking patterns.

**Consumes:** Existing `AdminWorkspace`, `RoomDetailPanel`, `ParticipantRow`, `TokenPanel` and mutations.
**Produces:** Reviewed room-list/detail presentation with existing full administration capabilities intact.

- [ ] Add coverage for selection, ready gating, room creation, long names, empty/loading/error states and mutation retry. Preserve separate participant presence/selection/controller meanings.
- [ ] Pin token one-time-secret behavior, revoke confirmation, kick/delete confirmations and authorization failures before moving disclosure boundaries.
- [ ] Run `npm test -- src/app.test.tsx` and the new browser test to prove the new presentation assertions fail.
- [ ] Apply neutral detail panel, compact rows, adjacent readiness/start action and collapsed secondary settings/token management. Do not replace existing operations with the prototype's local-only notices or simplified four-player gate.
- [ ] Re-run unit/browser tests; inspect keyboard disclosure traversal, dialogs and desktop long-content states.

## Release gate and handoff

- [ ] Run `npm run typecheck`, `npm test`, `npm run build`, `npm run test:browser` in `frontend/`. Run configured real-server suite with `npm run test:browser:real` when its prerequisites are available; missing coverage is a blocker to claiming integrated completion.
- [ ] Check desktop routes from real Entry through Lobby/Live/results plus Replay and Admin. Verify no prototype imports, `/prototype-characters` URLs, sample constants or restricted assets enter the bundle.
- [ ] Review the migration-only diff, preserve unrelated edits, update Wayfinder with exact verified scope and retain unresolved issues.
- [ ] Present screenshots locally and request final acceptance. Commits, deployment and runtime copying remain separately authorized actions.

## Explicitly deferred

Smartphone visual redesign, native apps, new framework/dependencies, broad backend changes, autonomous agent play, asset redistribution and deployment. The intermittent Replay resize concern remains open until reproduced and explained or explicitly accepted as a known limitation.
