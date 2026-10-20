# driichi UI/UX Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task after owner authorization. If the owner selects subagent execution, use superpowers:subagent-driven-development instead. This plan does not itself authorize agents, product changes, CI dispatch, merge or release. Checkboxes describe future work.

**Goal:** Deliver PR #24's complete, accessible UI in one coordinated public cutover, retaining authoritative gameplay and all supported non-3D mobile functionality.
**Architecture:** Retain React/Vite/TanStack Query/Zustand, Rust RoomActor/SQLite, and existing Three.js/Pixi renderers. Introduce shared shadcn/Base UI primitives and Tailwind tokens first; share pure result presentation between separate Live and Replay controllers. Compute Benchmark analytics from persisted Match evidence, independently of chart rendering and live observation.
**Tech Stack:** Existing React 19/Vite 8/TypeScript, Tailwind v4, shadcn Base UI, Recharts v3 through shadcn Chart, existing Phosphor icons/fonts, Vitest/Testing Library/Playwright/axe.
**Spec:** [PR #24](https://github.com/eitaar/driichi/pull/24), `docs/superpowers/specs/2026-10-14-ui-ux-grill.md` at `5fb2cae234302f9161e92ec19531af7f051a6ec3`. Do not assume this unmerged file exists on main. Backend companion: [PR #23](https://github.com/eitaar/driichi/pull/23) at `37407f3c4d4d2b181c4c9e41b55a2151656b7c02`.
**Written against:** `origin/main` commit `7e42556d9a3c7635c80522042d771abec378b870`, 2026-10-15.
**Workspace:** `C:/Users/eitab/Documents/js/driichi/.worktrees/ui-ux-cutover`, branch `feat/ui-ux-cutover`, based on fetched main. Original checkout/other worktrees remain untouched.
**Plan status:** Supersedes the initial handoff at this same path. Product source has not been changed. No dependency setup, tests, rendered inspection, commits, push, remote implementation PR, CI or deployment performed. Metadata-only shadcn CLI commands downloaded tooling into npm cache, not project dependencies.

## Global Constraints

- PR #24 is the authoritative UI specification; PR #23 owns Benchmark backend/domain decisions and remains separately owned. Neither documentation PR receives application code or title changes.
- Latest narrow supersessions govern: D82 no mobile 3D; D83 one Draw step; D84 chapter-only playback preservation; D85 accessible desktop Review Continue; D86 separate controllers; D87 renderer mapping evidence; D88 persisted-data charts; D89 portrait/name-only Lobby cards; D90 foundation first; D91 independent PRs.
- English-only controls, names, errors and accessible copy. User-entered names remain untranslated and rendered as text.
- System/Light/Dark; neutral/slate shell with restrained teal/green semantics. Existing tabletop/tiles/materials/artwork/dora behavior are not globally recolored.
- Only 3D-table functionality is desktop-only. Existing supported viewport is >=1024×600. This viewport capability gate is not an app-wide device/route ban or a reason to discard mobile Room/Ready/Admin/results.
- Mobile: Home/Join/Lobby/Room/Ready, authorized Admin management, Benchmark setup/status/Stop/guarded Resume, all four charts, history and authorized non-3D results/Replay metadata. No phone Canvas, renderer Worker, alternative 2D table or enabled/disabled `Open Live Table`.
- Normal legal actions only when server-offered, directly above local hand. Preserve tile-hit/candidate/decision/deadline/rejection behavior; no side Action Panel or independent dock.
- Participant identity is not Seat or display name. Preserve Participant→Seat→position→score in 3p/4p and all renderer paths. Lobby has no assigned compass Seats; previous post-Match Seats cannot imply next Seats.
- Benchmark uses fixed software roster, 1–1,000 sequential durably completed Matches, unchanged MJAI/MCP/Built-in contracts, 3p MJAI prohibition, no automatic fill. Preserve Completed/Stopped/Interrupted/Failed and persisted Replay linkage.
- Four actual-result charts: absolute cumulative table-relative net score; moving-average rank; 100%-stacked horizontal rank distribution; selected-Participant signed per-Match net bars. No live-hand projection or summary-only substitute for persisted evidence.
- No A/B Experiment, breaking Bot Protocol v2, reset/handshake requirements, research instrumentation, second renderer, distributed scheduler or parallel Matches.
- Internally staged work; one public all-route cutover. No prepared old-UI rollback. Owner retains final design, merge and release approval.

## Review Focus

1. Duplicate names/rotating Seats: identity, focus, series and scores must remain Participant-ID based (Tasks 4, 7, 9, 11).
2. Partial/duplicated/racing Match pages: no fabricated zeros, double count or false complete analytics (Tasks 7, 9).
3. Result expiry/reconnect/multi-Ron: Review cannot reset timer, send stale confirmation or leak old hand data (Task 5).
4. Mobile route/deep-link/resize: non-3D operations remain available without viewer mounting or confidential observation fetching (Tasks 2, 5, 6, 10).
5. Revoked auth/failed assets/fullscreen/renderer: purge private caches, retain safe fallback/focus and honest state (Tasks 3, 5, 10, 11).

---

## Grounding and skill decisions

Used: Superpowers writing-plans, brainstorming constraints, git-worktrees, TDD/writing-good-tests and verification-before-completion; shadcn component/CLI/composition/forms/styling/Base-vs-Radix rules; UI/UX Pro Max targeted searches; fixing-accessibility; improve-ui evidence discipline; ponytail scope control.

- PR #24's entire 848-line specification, latest PR bodies/files/commit lists/comments/reviews inspected. No issue/review comments or reviews returned. PR #23 read selectively for active contracts, not treated as independent UI authority. Both heads rechecked unchanged on 2026-10-15.
- shadcn `info --json`: existing Vite/TypeScript SPA, `rsc=false`; no components.json, preset, aliases, Tailwind or installed components. No current preset to overwrite. Existing package runner is npm.
- shadcn CLI docs and Base registry metadata confirm Base UI Toast/Drawer and shadcn Chart using Recharts `3.8.0`. Use `toast.add`, not Sonner. Source inspection is via CLI; docs via published documentation URLs.
- CLI `view @shadcn/toast` without config defaulted to a missing new-york registry item. Explicit Base registry URLs resolved; this is not evidence Base Toast is unavailable. CLI search printed results then a Windows libuv shutdown assertion; do not treat that as product failure.
- UI/UX Pro Max found applicable web focus-not-obscured and controlled-modal guidance. Chart searches did not precisely identify the requested rank-distribution design; do not substitute Waffle/Treemap recommendations. The four chart designs remain dictated by PR #24.
- Current Win/Final Standings already have the approved left-character/right-data layout; reuse. Live already has above-hand actions and a desktop gate. Existing result decoders, identity checks, socket ownership/preload, auth cache purge and renderer tests remain.
- Actual gaps: shared foundation/theme; two-step Join; mobile Join-first order; Lobby cards/summary; mobile Admin list→detail; Review/Draw display; chapter-preserving Replay; four charts/Drawer/paging; unified desktop boundary on Replay/Benchmark; SSE/Resume/paged index API.

### Important interpretation, not new cosmetic design

UI-D76/D79's older phone Continue wording conflicts with newest D85's mobile read-only statement. This plan uses the newest restrictive rule: mobile results expose authorized information/countdown/status and local Expand/Return, **no `confirm_result` or gameplay action**. Desktop Review retains Continue. Include this interpretation in final owner design signoff. If the owner requires mobile acknowledgment, stop and revise Tasks 2/5 explicitly rather than silently enabling it.

The score-at-another-Seat report remains unverified. Tests and screenshot evidence are required; geometry imports or paint-call assertions cannot justify a fix claim.

## Locked implementation defaults

These are engineering defaults, not newly claimed owner decisions:

- Existing application, no new scaffold, `add --all`, design-system showcase route, unnecessary community blocks or extra state/form/router dependency.
- shadcn `base-nova`, Phosphor icon family, existing locally available fonts. Set `iconLibrary=phosphor` in resolved config; audit CLI-generated imports. Default control hit area 44×44px, icon controls 44px, form text 16px; compact table rows do not reduce their interactive targets. Frame/card radius 8px, overlay radius 12px, gap scale 4/8/12/16/24/32px. Adjust shared variants, never per-screen color/typography overrides.
- Tokens in existing `frontend/src/styles.css`, Tailwind v4 `@theme inline`, class-based dark theme. Standard light roles: background slate-50, foreground slate-900, card white, muted slate-100/slate-600, primary teal-700/white, destructive red-700/white. Dark: background slate-950, foreground slate-100, card slate-900, muted slate-800/slate-300, primary teal-400/slate-950, destructive red-400/slate-950. Store values only in theme definitions; verify WCAG contrast rather than assume swatch names prove it. Borders/focus must meet applicable non-text contrast.
- Default theme System, preference key `driichi:theme`; storage failure uses in-memory preference, system changes apply only in System, cross-tab storage events synchronize explicit choices.
- 800px breakpoint for non-3D list/detail and mobile Drawer; >=1024×600 3D capability; >=1280px comfortable Benchmark 2:1 table/rail, otherwise supported-desktop stack. Do not resize the table to a 2/3 column at the smallest supported viewport if it makes necessary content unreadable.
- Base UI Sheet for desktop side detail/Review; Base UI Drawer for mobile bottom detail. Always title, close/expand button, usable footer, focus handling. No swipe-only action or manual overlay z-index.
- Recent 50/All applies to time-series x-range. Leaderboard/distribution use full loaded Run evidence and explicitly label `All completed Matches`; cumulative/moving calculations precede clipping. Moving rank window=10 trailing Matches, partial windows display actual n. Provisional banner at 1–9 completed Matches; zero=`No results yet`.
- History/index page size=25; persisted results fetch size=100, maximum 1,000 Matches, one request at a time. Incremental fetch must use a backend consistency watermark/revision; never silently join contradictory pages.
- Keep accepted mutation feedback server-driven; no optimistic fake lifecycle. Disable duplicate intent while pending. Backend defines wire revision/idempotency semantics.

## Ownership and backend boundary

UI branch changes frontend and UI plan/test evidence only. Backend agent owns Rust, migrations, persisted recovery, SSE authorization and wire endpoint contract. Do not cherry-pick/reset/edit their branch or redefine protocol from UI.

### UI-facing contracts to freeze with backend owner (Task 0)

Use existing `BenchmarkRun` roster/Match evidence fields and existing auth/cookie transport. Add frontend types only after backend fixtures agree:

```ts
type RunStatus = 'running' | 'completed' | 'stopped' | 'interrupted' | 'failed';
type BenchmarkMatch = BenchmarkRun['matches'][number];
type ReplayAvailability = 'unknown' | 'available' | 'unavailable' | 'too_large';
type BenchmarkMatchEvidence = BenchmarkMatch & { replay_availability: ReplayAvailability };
type BenchmarkRoster = BenchmarkRun['roster'];
type RunSummary = Pick<BenchmarkRun,
  'run_id' | 'room_code' | 'game_mode' | 'target' | 'completed' | 'status' | 'reason' | 'failed_match_id'>
  & { room_name: string; roster: BenchmarkRoster; created_at: string };
type RunIndexQuery = { search: string; status: RunStatus | 'all'; mode: GameMode | 'all'; offset: number; limit: number };
type Page<T> = { items: T[]; offset: number; limit: number; total: number; has_more: boolean };
type ResumeEligibility = { allowed: boolean; blockers: string[] };
type RunDetail = { summary: RunSummary; revision: number; stop_requested: boolean; resume: ResumeEligibility };
type MatchPage = Page<BenchmarkMatchEvidence> & { completed: number; revision: number };
type MutationIntent = { revision: number; idempotency_key: string };
type BenchmarkInvalidation = { run_id: string; room_code: string; revision: number };
```

These define the client boundary, not already existing HTTP payloads/routes. Backend owner may publish equivalent wire shapes; `api.ts` maps them to these frontend types once, not in components. No speculative endpoint strings. Missing room name/time/eligibility must be supplied or an honest unavailable representation agreed, not invented.

Required methods after freeze:
`listBenchmarkRunSummaries(query, signal): Promise<Page<RunSummary>>`, `getBenchmarkRunDetail(runId, signal): Promise<RunDetail>`, `listBenchmarkMatches(runId, {offset,limit,revision}, signal): Promise<MatchPage>`, `resumeBenchmarkRun(runId,intent): Promise<RunDetail>`. The agreed SSE endpoint is consumed by Task 10. Stop retains current room-targeted operation unless backend explicitly changes it; integrate agreed intent fields without breaking other callers.

**Required backend semantics:**
- D240 safe Interrupted-only manual Resume, persisted Match boundary, unchanged roster/mode/target/Run ID/results; fail closed on unsafe/unavailable reconnect checks, never mid-Match or Stopped/Completed/Failed.
- D241 Admin-cookie-authenticated SSE invalidation→authorized Snapshot, scoped monotonic revisions, heartbeat/revocation/reconnect semantics. Heartbeat proves transport liveness, not successful Snapshot refresh; the client tracks successful authorized reads and unapplied invalidations separately. Not every-frame Bot streaming. No auth token in URL.
- D242 bounded searchable/filterable newest-first index, separate consistent persisted-result detail/pages. Match pages supply Replay availability keyed to the exact match_id: `available` requires real saved and permitted Replay evidence, `unavailable` covers missing/failed-save/deleted Replay, and `too_large` retains the existing service limitation. `unknown` is the safe frontend state while unverified/unfetched or after a metadata error; never infer availability from Match completion or ID alone. Availability is metadata, not an analytics score: unknown/unavailable Replay must not suppress valid persisted charts. Deliberate old-client compatibility.
- RunDetail exposes the server's authoritative `stop_requested` for that run_id over the ordinary non-3D authorized detail read, independent of private live projection/SSE. Re-fetch it on entry/reload, window focus and after uncertain mutation outcomes; poll detail every 5s while running so other tabs/mobile recover accepted pending Stop. Terminal status supersedes the pending label. No sessionStorage-only or local optimistic stop flag.
- Revision/CAS and idempotent mutation responses; 409 refresh/retry, 401 cache purge; no duplicate start/resume or counting. Match Stop responses to the requested Run identity before updating UI; never apply a room's later Run progress to an older Run detail.
- Current Participant→Seat projection across new Matches; focus is presentation only, not changed audience or Bot control.

**Parallelism:** Tasks 1–8 (including HUD Task 5a) and renderer characterization can be planned/implemented before backend integration. Tasks 9–10 require frozen fixtures, and final cutover requires real backend evidence. A mock adapter is not release acceptance.

## Execution conventions

All paths below are relative to the dedicated worktree. Initial baseline is **unknown**, not assumed green. After implementation authorization, use Node 24.11.0/npm 11.4.0 and existing lockfile; install in this worktree only, run baseline typecheck/unit/build/browser commands, report any failure by name and obtain direction before attributing it to new work. Never link another worktree's mutable node_modules/build output.

For every code task below:
1. Write the named behavior tests with hand-derived expectations; don't compute expected values using production mapping/analytics helpers.
2. Run its RED command; confirm failure is missing behavior, not malformed fixtures/import errors. Preserve prior passing regressions.
3. Implement minimum change and update affected callers/fixtures.
4. Run the same command GREEN, then full `npm --prefix frontend test` and typecheck. Add task-specific browser verification.
5. Inspect diff; stage **only the task's files**, commit the listed message on UI branch once authorized. Do not push/merge. Generated primitives/config are setup, but their product consumers still require tests.

No product classes get test-only cleanup APIs. External network/SSE may be faked at transport boundaries; render actual components. Do not assert that a chart mock exists or that source contains a CSS string.

---

## Milestone A — Contracts and common foundation

### Task 0: Freeze backend compatibility and test fixtures

**Files:** update this plan's contract record only; planned fixtures `frontend/tests/fixtures/benchmark-ui-contracts.json`; proposed frontend type owner `frontend/src/api.ts`.
**Consumes/produces:** Backend owner's real route/response/error/event fixtures and compatible commit; frontend contracts above. No backend implementation.
- [ ] Send contract handoff to PR #23 owner without posting/pushing on their behalf; record acknowledged routes, schemas, revision scope, paging consistency, heartbeat and safe-Resume blockers.
- [ ] Record 401, 409, Interrupted unsafe/safe, Stopped, between-Match and growing-history cases; include server `stop_requested=true` after reload/second-tab read and after an accepted Stop whose response was lost. Include per-Match Replay available/missing/failed-save/deleted/too-large/unknown cases and healthy SSE heartbeats with failed or lagging Snapshot reads. Check runtime result fields against current API types, not just TypeScript assertions.
- [ ] Record which frontend tasks can proceed independently; do not schedule Tasks 9–10 until this gate passes.
**Gate:** Evidence of agreement, not a claimed message delivery. Wire names unresolved means integration remains blocked, not permission to invent them.

### Task 1: Tailwind/Base UI primitives and semantic tokens

**Files:** modify `frontend/package.json`, `package-lock.json`, `vite.config.ts`, `tsconfig.json`, `src/styles.css`; create `frontend/components.json`, `frontend/src/lib/utils.ts`, required `src/components/ui/*.tsx`, `src/components/theme-provider.tsx`, `theme-provider.test.tsx`.
**Interfaces:** `ThemePreference = 'system'|'light'|'dark'`; `ThemeProvider({children}: {children: ReactNode})`; `useTheme(): {preference: ThemePreference; setPreference(value: ThemePreference): void}`. `@/*` maps `src/*` in TypeScript and Vite. Existing frontend globals remain in styles.css, no second token owner.
- [ ] RED: tests `uses system scheme until explicit choice`, `survives rejected localStorage access`, `synchronizes explicit cross-tab preference`, `does not change scene appearance settings`. Assert DOM theme/consumer-visible style context; never test a copied constant.
- [ ] Run `npm --prefix frontend test -- src/components/theme-provider.test.tsx`.
- [ ] Add Tailwind v4 Vite plugin without replacing optimizeDeps/Worker Fast Refresh exclusions/production store entry. Initialize existing app with Base/Nova; no overwrite/reinstall/scaffold. Re-run `npx shadcn@latest info --json`, require base=base, rsc=false, correct CSS/alias/icon fields.
- [ ] Before each consumed component: CLI docs→fetch actual docs; inspect explicit Base registry, preview install and read generated files. Add only Button/Input/Field/InputGroup/Select/ToggleGroup/RadioGroup/Checkbox/Switch/AlertDialog/Dialog/Sheet/Drawer/Card/Avatar/Toast/Sidebar/Breadcrumb/Pagination/Table/Alert/Empty/Skeleton/Spinner/Separator/Accordion/Chart plus required registry dependencies. No `add --all`, Sonner, Vaul/Radix substitute or unused chat/resizable components.
- [ ] Theme and component variants own styling. `FieldGroup+Field`, `data-invalid+aria-invalid`, Base `render` composition, proper groups, mandatory overlay Titles, complete Card/AvatarFallback, `data-icon`, `cn`, `gap`, semantic colors, no app-supplied overlay z-index. Pin resolved dependency versions/CLI provenance in lockfile/evidence; verify generated utility/icon imports resolve to this app.
- [ ] GREEN; build validates Tailwind, typecheck validates generated code. Commit `feat(ui): add Base UI token foundation`.
**Gate:** Token consumer/theme tests and build pass; no renderer materials or Bot dependencies changed.

### Task 2: Shared shell and desktop-only viewer lifecycle

**Files:** modify `frontend/src/app.tsx`, `src/game/viewport.ts`, `viewport.test.ts`; create `src/components/app-shell.tsx`, `desktop-table-boundary.tsx`, `app-shell.test.tsx`, `desktop-table-boundary.test.tsx`; update `src/benchmark.tsx`, `src/replay.tsx` entry compositions.
**Interfaces:** `AppShell({variant,children}: {variant:'public'|'admin'|'table'; children:ReactNode})`; `DesktopTableBoundary({children,fallback}: {children:ReactNode; fallback:ReactNode})` consumes existing viewport hook. Keep `Route`, `navigate(path)` and HumanLobby socket owner unchanged; Toaster/theme providers live above route changes.
- [ ] RED: `retains confirmed success toast across navigation`, `guest navigation contains no Admin-only links`, `mobile viewer does not mount renderer or fetch confidential observer`, `resize to unsupported viewport disposes viewer and preserves dashboard`, `3D-only deep link has desktop limitation and safe Back`. Test actual boundary with lifecycle/resource spies, not mocked boundary markup.
- [ ] Run `npm --prefix frontend test -- src/components/app-shell.test.tsx src/components/desktop-table-boundary.test.tsx src/game/viewport.test.ts`.
- [ ] Use Sidebar/Breadcrumb for Admin, minimal public topbar, labeled mobile menu; compact existing table HUD for desktop Live/Replay so persistent navigation cannot shrink the tabletop. Keep QueryClient/session purge centralized.
- [ ] Gate lazy viewer children before resource/subscription mount; keep Room state/non-3D summary outside boundary. Lazy-load Benchmark observer renderer separately from charts. Mobile omission removes 3D opener completely; Replay metadata remains navigable.
- [ ] GREEN plus deep-link browser coverage. Commit `feat(ui): unify shell and desktop table boundary`.
**Gate:** Boundary prevents Canvas/Worker/private observation fetch on phone but doesn't block authorized setup/status/Ready/admin actions.

## Milestone B — Public and management flows

### Task 3: Join-first Home and same-URL two-step Join

**Files:** modify `frontend/src/app.tsx`, `src/pre-match.css`, `src/app.test.tsx`, `src/pre-match.test.tsx`, `tests/entry.spec.ts`, `tests/pre-match-migration.spec.ts`.
**Interfaces:** keep `RoomLookup`, `HumanCharacter`, `api.joinHuman` and existing URLs/sessionStorage identity. RoomJoin local state `step: 'room-name'|'character'`; no new route/history entry.
- [ ] RED: mobile Home reading/focus order puts Join before decorative art; Step 1 Room/name then Step 2 Character; in-app Back retains name/selection; browser Back leaves Join; invalid/rejected join retains inline error and input; unavailable portraits/voice do not claim successful media playback.
- [ ] Run `npm --prefix frontend test -- src/app.test.tsx src/pre-match.test.tsx`.
- [ ] Compose Field/FieldGroup, Input, InputGroup if action is inside input, RadioGroup for Character choice, Button/Alert. Preserve six-digit lookup, async cancellation, permitted join, character preload/voice and asset fallback. Don't add account, animation gate, validation/form library or synthetic same-URL history.
- [ ] GREEN, browser `npm --prefix frontend run test:browser -- tests/entry.spec.ts tests/pre-match-migration.spec.ts`. Commit `feat(ui): migrate Home and two-step Join`.
**Gate:** Field errors announced/associated, Back semantics correct, long names and mobile zoom don't clip Join.

### Task 4: Stable portrait Lobby and mobile Admin list→detail

**Files:** modify `frontend/src/app.tsx`, `src/pre-match.css`, `src/admin.css`, `src/pre-match.test.tsx`, `src/app.test.tsx`; create `src/components/lobby-participant-card.tsx`, `lobby-participant-card.test.tsx`.
**Interfaces:** `LobbyParticipantCard({participant,own,previousSeat}: {participant:RoomParticipant; own:boolean; previousSeat?:number})`; server data/Set Ready/preload remain existing owners. Detail state is local presentation only.
- [ ] RED: three/four cards ordered stable ID; changing ready/presence doesn't reorder; default visible card only portrait/name, no Ready/You/glyph/wind; Enter/tap opens readable exact states; header selected/ready/connected counts are separate; former Match Seat is explicitly previous; blocked participant identifiable without hover.
- [ ] Run `npm --prefix frontend test -- src/components/lobby-participant-card.test.tsx src/pre-match.test.tsx src/app.test.tsx`.
- [ ] Compose frame-only mild skew (desktop 3° maximum), straight art/name/focus targets; compact mobile grid, remove skew at cramped widths. Use existing portraits, not icon thumbnails/new art. Focusable Base detail trigger is a button with participant name; do not make an article pretend to be interactive.
- [ ] Add Room code/header summary and labeled Set Ready/blockers; preserve server-determined ordering/selection/readiness semantics and image preload. Use Sidebar/Table/Fields for Admin; mobile /admin shows Room list, /admin/rooms/:code shows full-width detail with Back. Token dialogs remain one-time, accessible, and secret-free in toast/history/log.
- [ ] GREEN and narrow browser screenshots. Commit `feat(ui): migrate Lobby and Admin Room flow`.
**Gate:** No fake fourth player or prestart Seat; exact readiness remains available to keyboard/touch/screen reader; management data remains Admin-only.

## Milestone C — Outcome, themed gameplay HUD and Replay controllers

### Task 5: Pure outcome views and deadline-safe Review

**Files:** create `frontend/src/game/result-presentation.tsx`, `result-presentation.test.tsx`; modify `src/game/gameplay.tsx`, `result.css`, `gameplay.test.tsx`, `results.test.ts`, `src/replay.tsx`, `tests/result-surfaces.spec.ts`, `tests/kyoku-results-real-server.spec.ts`.
**Interfaces:** use existing validated `WinResult`, `KyokuSettlement`, `FinalStandings`, `KyokuEndStep`, `RoomSnapshot`, `AssetState`. Extract existing `RoundWinSurface`/`ScoreTransfer`/`ResultsPanel` into pure presentation owner, update Replay imports. Add `DrawResult({settlement,room}: {settlement:KyokuSettlement; room:RoomSnapshot|null})`. Views never import live store/transport or Replay timeline. Live controller owns Review/current step; Replay retains its local animation clock.
- [ ] RED: Win reveals winner-only exact hand/Yaku/points; scoreboard includes before/signed delta/after and validates IDs/Seats; Draw reason and scores use one step/timer; Review exposes authorized Continue tied to current step; step change/expiry/revocation closes sheet; stale/double confirmation rejected; replay view sends no confirmation; phone result read-only but complete metadata/Return accessible.
- [ ] Run `npm --prefix frontend test -- src/game/result-presentation.test.tsx src/game/results.test.ts src/game/gameplay.test.tsx`.
- [ ] Reuse left Character/right data and scrim rather than redesign working layout again. Draw left typographic actual reason + existing quiet tile motif, never fake winner/tenpai. Final winner left, complete ranks right including first; Return to Room only.
- [ ] Draw contains explicit Result/Scores ToggleGroup, initially Result, with optional 600ms cosmetic switch only if user has not selected a view and deadline still active. Both views reachable immediately; no new step/ack/timer. Win/multi-Ron/Settlement follow server steps only. Score animation remains 3s change within server's 5s presentation, never extends authoritative remaining time; reduced motion instant.
- [ ] Compact footer/Review Sheet: one currently reachable desktop Continue/Waiting, same `confirmResult(step.id,send)` idempotency. Move/compose canonical action for modal access, don't maintain independent confirmation state. Countdown isn't a 50ms assertive live-region stream; announce step/expiry/waiting, expose readable remaining value. Invalidate Review key on result-step change/access loss and restore safe focus if trigger vanished.
- [ ] Mobile authorized summary uses Base Drawer with explicit Expand/Close, winner/draw/final truthful variants, hand/Yaku/score after Expand. No table backdrop or live confirmation. Required navigation/status remains usable.
- [ ] GREEN; browser result surfaces and separately authorized real-server result suite. Commit `feat(ui): share outcome presentation and safe Review`.
**Gate:** Real Draw one-step evidence, multi-Ron progression, shrinking reconnect deadline, renderer/asset stalls don't gate Continue; no extra private hands.

### Task 5a: Migrate normal-game HUD and controls without changing table behavior

**Files:** modify `frontend/src/game/gameplay.tsx`, `live-table.css`, `src/styles.css`, `src/game/gameplay.test.tsx`, `table-appearance.tsx`, `table-appearance.test.tsx`; create `frontend/tests/gameplay-hud.spec.ts`. Change `src/replay.tsx`/`replay.css` only for shared DOM HUD consumers, not playback/controller semantics.
**Interfaces:** preserve existing `ActionDeck`, `CandidatePopup`, `GameplayControls`, `AudioSettingsPanel`, `useTableAppearance` and tile/action callbacks. Consume Task 1's theme/primitives. No new Decision, action IDs, socket owner, renderer API, asset upload or game authority. Task 5 still owns outcome presentation; this task owns non-result gameplay controls/HUD.
- [ ] RED: Light/Dark/System visibly update ActionDeck, candidate chooser, connection/error HUD and audio/table-image settings while table materials/tiles stay unchanged. Offered Ron/Chi/Pon/Kan/Riichi/Pass remain above hand; discard-only and idle states add no fake buttons. Variant selection, legal tile hit targets, keyboard focus, pending/rejected/expired/offline behavior and current deadline are unchanged.
- [ ] RED: Escape/focus restoration and Enter activation work in candidate/settings UI; settings preserve volume/mute and local table-image validation, failed-image errors and local-only privacy. Reduced-transparency fallback remains legible against moving table; theme change does not reopen a candidate dialog or reset Decision/animation state.
- [ ] Run `npm --prefix frontend test -- src/game/gameplay.test.tsx src/game/table-appearance.test.tsx` for RED/GREEN. Browser verification: `npm --prefix frontend run test:browser -- tests/gameplay-hud.spec.ts` at 1024×600 and 1440×900, Light/Dark/System, normal/reduced motion/transparency.
- [ ] Migrate action/settings buttons, switches/sliders/fields and accessible overlay content to shared Base UI components. Add only required `slider`/`popover` primitives via Task 1's docs/preview/source-inspection workflow if their consumers need them. Replace `.text-button`/legacy-class-dependent focus queries with stable refs or semantic targets when those classes disappear; do not break existing candidate focus management. Persistent action/network/image failures stay inline Alert, never toast-only.
- [ ] Replace hardcoded peripheral HUD background/text/border values in `game/live-table.css` with shared semantic tokens and a theme-aware translucent HUD surface; controls retain solid-enough contrast. Keep HUD placement/hand-safe geometry, tile-hit imagery/dora sheen, tabletop/background materials and rendering CSS unchanged. Document only actual scene-artwork exceptions, not a blanket exemption for old HUD buttons.
- [ ] GREEN/full unit suite/typecheck/browser evidence; commit `feat(ui): migrate themed gameplay HUD controls`.
**Gate:** UI-D34/D51/D65/D90 verified on the actual game HUD in both themes, no score/Seat/action/timer regression, and no scene material recoloring. Task 12 cannot accept an unthemed legacy HUD as a specialized-renderer exception.

### Task 6: Kyoku chapter navigation with narrowly preserved playback

**Files:** modify `frontend/src/replay.tsx`, `replay.css`, `replay.test.tsx`, `tests/task15.spec.ts`, `tests/result-surfaces.spec.ts`.
**Interfaces:** keep ReplayViewer frame/decode/playback engine. `selectKyoku(position:number):void` preserves existing `playing`; existing `selectPosition(position:number):void` remains pausing event seek. Existing round destination derivation stays authoritative.
- [ ] RED: while playing chapter jump seeks chapter start and keeps playing; paused jump stays paused; Prev/Next/slider/event-log always pause; multiple honba chapters correctly distinguished; unavailable legacy fields stay absent; phone shows metadata/chapters but no playback Canvas.
- [ ] Run `npm --prefix frontend test -- src/replay.test.tsx`.
- [ ] Desktop vertical chapter list with current chapter semantics and keyboard links/buttons; mobile non-3D dropdown/chapter metadata, no fake player input or renderer. Reuse Task 5 pure views; keep Replay local clock/seek/Play/voice independent of Live deadlines/Continue.
- [ ] GREEN and result-surface Replay browser coverage. Commit `feat(ui): add state-preserving Kyoku navigation`.
**Gate:** Preservation applies only to chapter jump, not every seek; no competing Replay controller or live confirmation.

## Milestone D — Persisted analytics and inspection

### Task 7: Validated analytics model independent of charts

**Files:** create `frontend/src/benchmark-analytics.ts`, `benchmark-analytics.test.ts`; modify current `src/benchmark.test.tsx` expectations only when its actual consumer changes in Task 8.
**Interfaces:**
```ts
type AnalyticsPoint = { sequence:number; matchId:string; net:number; cumulative:number; movingRank:number; movingN:number };
type ParticipantAnalytics = { participantId:string; displayName:string; averageRank:number|null; firstPlaceRate:number|null; cumulativeNet:number; rankCounts:number[]; points:AnalyticsPoint[] };
type RunAnalytics = { kind:'ready'; completed:number; provisional:boolean; participants:ParticipantAnalytics[] }
  | { kind:'invalid'; reason:string };
function deriveRunAnalytics(input: Pick<BenchmarkRun,'game_mode'|'completed'|'roster'|'matches'>): RunAnalytics;
```
Zero records with completed=0 is ready with null rank/rate and empty points. Missing advertised records is invalid, not zero results. Use fixed roster order for identity then separately sort leaderboard average-rank/ID; deterministic ties use codepoint ID comparison.
- [ ] RED: 4p first Match scores A=40000/B=30000/C=20000/D=10000 yield net `[15000,5000,-5000,-15000]`; 3p 40000/30000/20000 yields `[10000,0,-10000]`; rotating Seats and duplicate names do not change identity. Add second Match A=10000/B=20000/C=30000/D=40000: A cumulative=0, mean rank=(1+4)/2=2.5, first-place rate=0.5, rank counts=[1,0,0,1].
- [ ] RED: duplicate Match/Participant/Seat/sequence, missing participant/results, invalid ranks/scores/mode, completed mismatch, >1000 count return invalid. Preserve duplicate rank values if supplied by valid authoritative outcomes; never reconstruct winners by score. Zero, partial moving window, 9/10 provisional boundary, negative scores and 50/51 recent window covered.
- [ ] Run `npm --prefix frontend test -- src/benchmark-analytics.test.ts`.
- [ ] Implement net=`score - sum(scores)/actualPlayerCount`, full-run cumulative and 10-Match trailing rank, full denominator distribution/rate. Validate trust boundary integers/finite scores and exact roster/seat/count consistency; no zero-fill/interpolation. Keep precision until display formatting. Memoization belongs at consumer, not global cache.
- [ ] GREEN, full suite/typecheck. Commit `feat(ui): derive persisted Benchmark analytics`.
**Gate:** Hand-derived literal assertions, no expected value computed by production code; no summary-only or live projection input.

### Task 8: Four shadcn Charts and one responsive Match detail

**Files:** create `frontend/src/benchmark-charts.tsx`, `benchmark-charts.test.tsx`, `benchmark-match-detail.tsx`, `benchmark-match-detail.test.tsx`; modify `src/benchmark.tsx`, `benchmark.css`, `benchmark.test.tsx`; create `tests/benchmark-dashboard.spec.ts`.
**Interfaces:** `BenchmarkCharts({analytics,onOpenMatch}: {analytics:Extract<RunAnalytics,{kind:'ready'}>; onOpenMatch(matchId:string):void})`; `BenchmarkMatchDetail({match,roster,onClose}: {match:BenchmarkMatchEvidence|null; roster:BenchmarkRoster; onClose():void})`. RunDetail owns selected Match ID and reads the corresponding MatchPage item's `replay_availability`, not four competing Drawers or an unconnected availability boolean. Task 7 consumes numeric Match fields only; Replay metadata does not affect analytics.
- [ ] RED: exactly four correct chart families default visible, full-run cumulative retained for Recent 50, separately labeled window net; leader initially chosen only once standings exist; manual Bot/series choices survive refresh, independent chart controls; 3p distribution no fourth rank; identified Match click or keyboard detail action opens correct record; distribution aggregate never opens arbitrary Match. Replay link exists only for `available`; unknown/unfetched/error offers status/Retry, missing/failed-save/deleted shows unavailable, and too_large shows its limitation. All variants retain real placements/points and charts, with no fake Replay navigation.
- [ ] Run `npm --prefix frontend test -- src/benchmark-charts.test.tsx src/benchmark-match-detail.test.tsx src/benchmark.test.tsx`.
- [ ] Use shadcn ChartContainer/Tooltip/Legend and Recharts 3.8.0 LineChart/BarChart. Hero full width, 3 secondary cards responsive 2-column grid. Line types linear, no misleading smoothing; cumulative zero reference, rank axis 1 best, signed net bars zero reference, distribution 100% horizontal. Add at least 240px measured chart region to prevent zero-size responsive mounts.
- [ ] Chart config keys are safe fixed `p0..p3` assigned from ID-sorted fixed roster; never inject raw participant IDs/names into CSS config selectors. Display labels as React text, colors via semantic chart tokens plus line dashes/patterns. Rank colors/labels are consistent across distribution, Participant colors consistent across time-series.
- [ ] Enable accessibilityLayer, give chart descriptive heading/axes; restore visible focus suppressed by upstream ChartContainer styles in owned primitive source. Tooltip remains accessible without hover. Use an explicit keyboard `View Match details` action bound to selected sequence and paged numeric Table equivalent with actual Button triggers; no 4000 SVG tab stops or tooltip-only critical data.
- [ ] Mobile Performance/Consistency Accordion `multiple`, both values initially open, plus jump menu. 25-row history pages recent-first. Desktop Sheet/mobile Base Drawer share detail data and restore trigger focus/scroll; build the existing authorized Replay route from match_id only when that item's `replay_availability === 'available'`. Revalidate a failed/deleted Replay navigation through existing Replay error/refresh handling, never fabricate playable data. Provisional banner sample n shared above analytics, visible sample context per chart; invalid score evidence shows Alert/Retry instead of graphs, while unknown Replay metadata leaves valid charts intact.
- [ ] GREEN/browser `npm --prefix frontend run test:browser -- tests/benchmark-dashboard.spec.ts`. Commit `feat(ui): add accessible Benchmark charts and detail`.
**Gate:** Actual Recharts browser rendering, numeric/chart parity and keyboard/touch detail verified; charts lazy-loaded apart from 3D renderer and do not perform private live reads.

## Milestone E — Backend integration, actions and observation

### Task 9: Bounded index/detail adapters and honest Run operations

**Files:** modify `frontend/src/api.ts`, `benchmark.tsx`, `benchmark.test.tsx`, `src/app.tsx` Benchmark setup callsite; create `src/benchmark-contracts.test.ts` and `src/benchmark-data.ts`, `benchmark-data.test.ts`.
**Interfaces:** Task 0 methods/types; `useBenchmarkEvidence(runId:string)` returns loading/incomplete/error/complete consistent evidence, never pretends summary is full Run. Preserve existing API endpoints for unrelated clients; only backend-owner published wire changes.
- [ ] RED: index search/status/mode/page reaches transport with encoded validated values, newest-first and empty/out-of-range pages; detail incremental pages 0/100/... loaded once, mixed revision triggers fresh metadata/retry, append invalidates tail not duplicate prior records; aborted old Run request cannot populate current Run. Match pages keep Replay metadata associated with exact Match ID; absent/unverified availability normalizes to unknown, not available, and metadata failures cannot manufacture links or corrupt numeric results.
- [ ] RED: exact connected eligible roster (3p excludes MJAI/human/disconnected), invalid target 0/1001/fraction, Start never autofills; New Run form creates Room then existing Admin route; Stop requires confirmation and accepted pending status; Resume allowed only Interrupted+server allowed, duplicate/conflict/auth cases don't fabricate Running. `stop_requested=true` in fresh detail restores Stopping after reload, mobile without observer, and another tab's accepted Stop. Lost Stop response triggers authoritative detail read; only matching Run identity is applied, terminal status clears pending label, and a failed read stays unknown/stale rather than assuming Stop failed or allowing duplicate scheduling.
- [ ] Run `npm --prefix frontend test -- src/benchmark-contracts.test.ts src/benchmark-data.test.ts src/benchmark.test.tsx`.
- [ ] Index uses 25-entry Table + search/filters/Pagination; resets offset on filter, bounded data; retain room/Bot names. New Run Dialog/Sheet only name/mode/time, then real roster setup. Inline Ready Panel immediately above roster/Start with exact blockers and 1–1000 target.
- [ ] Stop AlertDialog `Finish current Match, then stop`; disable only duplicate intent, not all navigation. Display `Stopping after current Match` from matching accepted server response or fresh RunDetail.stop_requested, never a local optimistic flag. Re-fetch ordinary authorized RunDetail on entry/window focus and uncertain outcomes; poll every 5s while running through one non-overlapping detail query, independent of desktop/private observer, to recover other-tab Stop. Task 10 invalidations coalesce with this same detail query rather than add another detail polling loop. Terminal status wins over pending Stop. Interrupted Resume displays concrete safe eligibility/blockers and accepted server response; terminal Stopped has no Resume. Missing backend feature remains visible unavailable, but final release cannot claim its integration passed.
- [ ] Generate idempotency key per confirmed intent, reuse for transport retry of same request, revision from current server snapshot; 409 means inline conflict/refresh and explicit new intent, never blind retry. Cache keys include run/filter/page scope and are canceled on navigation/auth loss. Validate received evidence before Task 7.
- [ ] GREEN and real backend fixture/integration acceptance. Commit `feat(ui): integrate bounded Benchmark contracts and actions`.
**Gate:** Task 0 agreement plus actual service evidence; no guessed endpoints, unsafe resume or unbounded list fallback.

### Task 10: One authorized observer with freshness and Participant focus

**Files:** create `frontend/src/benchmark-observer.tsx`, `benchmark-observer.test.tsx`; modify `src/benchmark.tsx`, `benchmark.css`, `admin-session.ts`; minimally extend existing `src/game/three-table.tsx`, `three-table-renderer.tsx`, `three-table-render-messages.ts`, `three-table-worker.tsx`, `three-table-scene.tsx`, `three-table-layout.ts`, `table-center-pixels.ts`, `orientation.ts` only where view transform requires it.
**Interfaces:** `BenchmarkObserver({runId,roomCode}: {runId:string; roomCode:string})`; local `focusedParticipantId:string|null`, resolved from current authoritative Match mapping. Add presentation-only `viewSeat?:number` to `ThreeTableProps`, backward-compatible when omitted. Inside ThreeTable derive one memoized display projection with only its viewer-seat orientation overridden; retain original audience, Participant IDs, Seats, scores and hand authorization. Pass that display projection consistently to layout, DOM portraits/hand/text scores, center painter and renderer/Worker; never write it to game store or send it to the server. Both main-thread and Worker consume the same display/bitmap revision; no second camera/renderer subsystem.
- [ ] RED: one observer Snapshot request at a time; repeated invalidations coalesce; old/out-of-order Run/Room responses ignored; stale/terminal/between-Match states truthful; 401/sign-out clears current/cached concealed data and disposes stream/renderer; mobile never subscribes or live-fetches. With healthy 15s SSE heartbeats but failing Snapshot GETs, show Stale and last successful update time, never Live. A notification for revision 8 with only revision 7 applied stays Updating then Stale on failure, even if heartbeats continue; a valid authorized revision 8 response clears the pending update, while late revision 7 cannot restore Live. Fake-clock tests cover independent 45s transport/data expiry and a quiet, unchanged Match renewed only by successful authorized reads.
- [ ] RED: choose ID B then rotate its Seat: focus remains B, labels/hand/center score follow actual new Seat. Camera/view changes never send game commands, rewrite participant identity or expose beyond authorized projection. Fullscreen transitions don't remount table each Match.
- [ ] Run `npm --prefix frontend test -- src/benchmark-observer.test.tsx src/benchmark.test.tsx`.
- [ ] One cookie-authorized SSE invalidation channel→GET Snapshot, coalesced with one in-flight request and pending refresh flag; never accumulate every event. Scope revision by Room/run lifetime. Track `lastTransportAt`, `lastSnapshotAt`, `appliedRevision`, `pendingRevision` and last read failure separately: heartbeat updates only transport time; only a valid authorized current-lifetime GET updates Snapshot time/applied revision. Newer unapplied revision removes Live immediately (Updating while request pending); failed refresh becomes Stale with last successful Snapshot timestamp. A behind-pending response cannot clear the dirty state or restore Live.
- [ ] Freeze freshness defaults with Task 0: heartbeats 15s; transport inactivity or Snapshot age >=45s becomes Stale. While observing a quiet Match, one idle renewal at 30s requests an authorized Snapshot through the SAME request coordinator, coalesced with invalidations; this is bounded data/auth revalidation, not a second independent poll loop. Live requires recent successful authorized Snapshot, no failed latest refresh/unapplied revision, an active current Match and healthy selected transport mode; successful HTTP fallback reads may restore fresh data while transport still explicitly says Polling. Waiting/terminal/unauthorized states never say Live.
- [ ] Reconnect SSE 1s exponential+jitter capped 30s; after 3 failures poll 5s and probe SSE 30s using the same observer request coordinator, with idle renewal disabled during fallback. Retire fallback only after restored SSE plus a valid current authorized Snapshot satisfying pending revision. Heartbeat or SSE-open alone cannot end fallback, renew confidential-data freshness or restore Live. Revocation cancels all renewal/retry/probe work and purges private projection caches.
- [ ] Resolve focus via one pure mapping and feed existing single table/camera presentation. Use current viewer-relative scene normalization as the final focused view: focus changes the display projection, not authoritative identity. Local hand is DOM, so apply the same view to it and every overlay. Do not add a seat-angle camera orbit on top of normalization (double rotation) or rotate canvas alone. Reuse existing camera presentation for a short 150ms focus accent; reduced motion snaps without accent, ordinary Live/Replay default orientation remains unchanged. Task 11 proves final pixels, not merely prop values.
- [ ] Desktop 2:1 table/leaderboard with narrow-desktop stacking; focus named Bot/current Seat near table and fullscreen. Fullscreen same mounted DOM instance; necessary Exit/status always visible, optional overlay fade after 5s; restore focus on exit, desktop in-page fallback if browser fullscreen denied. Persist viewer across between-Match transitions without stale Match hand masquerading as current.
- [ ] GREEN and backend auth/reconnect/focus browser cases. Commit `feat(ui): integrate authorized single-table observation`.
**Gate:** Coherent current view across DOM/bitmap/main-thread/Worker, real backend transport/revocation and honest freshness; no promise of every fast Bot frame.

## Milestone F — Acceptance, cleanup and release evidence

### Task 11: Independent Seat/score and renderer regression proof

**Files:** create `frontend/tests/seat-score-mapping.spec.ts`; extend `tests/renderer-worker.spec.ts`, `tests/support/renderer-worker-product.tsx`, `tests/multiplayer-regressions.spec.ts`, `tests/fixtures/*` only with hand-authored test data; reuse `src/game/three-table.test.tsx`, `table-geometry.test.ts`, `tests/support/real-server.ts`.
**Interfaces:** authoritative fixed Participant/Seat/score inputs and independently recorded expected screen locations. Tests must not use production `seatPositions`/`tableSeatGeometry` to generate their oracle.
- [ ] RED matrix: 4p viewers 0/1/2/3 and 3p viewers 0/1/2; distinct IDs/names and scores 11100/22200/33300/(44400), second update includes negative/unequal values. Literal expected bottom/right/top/left mapping in test fixture; 3p top empty. Repeat normal Live, Replay, Admin focus before/after rotation/update.
- [ ] Exercise product main-thread renderer and native Worker/center-bitmap path; semantic DOM checks paired with screenshots/crops at actual score positions. A 2D canvas paint-call spy alone cannot pass this task.
- [ ] Run focused semantic tests and `npm --prefix frontend run test:browser -- tests/seat-score-mapping.spec.ts`. Native Worker probe uses an exclusive build output and unchanged product CSP; configure own ports/processes, not shared other worktree servers.
- [ ] If mismatch reproduces, capture failing evidence first and fix the one shared transform/painter root cause across callers; no unrelated render rewrite or score reordering workaround.
- [ ] Review pinned browser/assets/viewport/DPR baselines with owner. Record reproduced/fixed or unreproduced explicitly. Commit `test(ui): verify real Seat and score correspondence` (include minimal fix only if proven).
**Gate:** Human-reviewed final renderer output plus independent semantic oracle; raster tolerance cannot mask wrong player/Seat/score.

### Task 12: All-route accessibility, mobile/privacy and cutover evidence

**Files:** create `frontend/tests/ui-cutover.spec.ts`; extend `tests/benchmark-dashboard.spec.ts`, `tests/result-surfaces.spec.ts`, existing Playwright configs as required; remove superseded consumer CSS in `src/styles.css`, `pre-match.css`, `admin.css`, `benchmark.css`, `replay.css`; record evidence in `design-plans/ui-cutover-acceptance.md`.
**Interfaces:** Existing routes, role/auth boundaries and tasks' published consumer contracts. This task introduces no new product feature.
- [ ] RED inventory: public/admin route/deep links, Guest/Admin/expired sessions; loading/empty/error/blocked/stale/running/between-Match/terminal; same-URL Back, join rejection, Room delete, one-time token close, Review expiry, Replay metadata, paged charts/history and mobile operations. Include Task 5a actual themed HUD/action/settings, heartbeat-alive/Snapshot-failed observer, reload/second-tab/mobile Stop restoration and Replay available/unknown/missing/failed-save/deleted/too-large cases; verify numeric charts survive unavailable Replay.
- [ ] Desktop 1024×600/1440×900, DPR1/2; web mobile 320/375/390px plus phone landscape and 200% zoom/text enlargement. Assert no horizontal document overflow, clipped footer/focus, mobile Canvas/Worker/live private GET or 3D opener. A large supported viewport uses shared capability rule, not unreliable user-agent detection.
- [ ] Axe plus manual keyboard/screen-reader/contrast: every field label+error association, disabled action explanation, dialog Title/initial focus/Escape/restore, custom chart keyboard navigation + numeric table actions, no hover/color-only states. Normal text >=4.5:1, large text/non-text >=3:1 where applicable; avoid fully obscured focus. Don't mislabel enhanced AAA focus criterion as AA.
- [ ] Retain reduced-motion, prefers-reduced-transparency fallback for translucent HUD, safe-area padding and long/untrusted/duplicate names. No popup/footer/timer announcement flood. Native-only skill guidance is not treated as browser platform requirements.
- [ ] Remove migrated duplicate shells/dialogs/token overrides; preserve specialized scene CSS and explicit HUD exception inventory. Run complete frontend, build, browser/main-thread/Worker and real-server suites; backend owner supplies durability/recovery tests. Failures remain named blockers, never hidden by fixture/screenshot regeneration.
- [ ] Baseline/candidate performance same pinned environment, repeated samples and absolute results. Companion D225 budgets: latency no >10% regression, desktop FPS no >10% drop, peak memory no >15% growth; mobile non-3D bundle/interaction/memory only. Record variability and unsupported hardware probes honestly.
- [ ] Prepare separate UI implementation PR against main after owner authorizes publishing: decision→task→test/evidence map, compatible backend commit, migration-free frontend compatibility, screenshots/contrast/privacy/performance, known limitations and forward-fix/maintenance procedure. Reconcile accepted DESIGN.md changes later without editing docs PR branches. Keep one coordinated public cutover, not per-route releases.
- [ ] Commit `test(ui): add coordinated cutover acceptance evidence` only after green checks. Owner separately approves design scope, exact cutover commit, merge/deploy/release; no automatic action.
**Gate:** No missing backend contract or unsafe recovery, all desktop mapping and mobile non-3D evidence, final owner Go/No-Go. If shared security breaks, maintenance/forward fix is separately authorized; no prepared old-UI rollback.

## Verification commands — implementation phase only

From worktree root after setup authorization:

```sh
npm --prefix frontend run typecheck
npm --prefix frontend test
npm --prefix frontend run build
npm --prefix frontend run test:browser
npm --prefix frontend run test:browser:real
cargo --locked test -p double_riichi_server --test benchmark_http --test benchmark_run --test benchmark_storage --test benchmark_visibility --test benchmark_mjai
```

This project's `frontend/tsconfig.json` includes `src` directly, so `npm run typecheck` does check app sources; don't substitute generic Vite tsconfig assumptions. Existing build also checks node/build configuration.

Native Worker verification, from frontend directory, with a unique outDir/port per runner:
```sh
npx vite build --config tests/support/renderer-worker-build.config.ts --outDir <exclusive-worker-probe-dir>
DRIICHI_WORKER_PROBE_DIST=<exclusive-worker-probe-dir> npx playwright test --config=playwright.worker.config.ts
```
Use Windows-appropriate environment syntax when needed; no production outputs/data/credentials. Existing worker config assumes port 4173, so serialize those probes or explicitly configure an exclusive port. Backend recovery tests are run against the compatible backend implementation, not docs-only PR #23.

## Spec coverage / self-review ledger

| Requirement family | Tasks |
|---|---|
| D1–D22 shell/theme/index/setup/leaderboard/focus/freshness | 0–2, 9–10; D20 mobile exception canceled |
| D23–D27 four-chart hierarchy/provisional/mobile categories | 7–8; D27 mobile 3D canceled |
| D28–D38 Home/Join/Lobby/game actions/Replay/Admin/HUD/detail/toast | 1–6 including 5a, 8–10; explicitly migrate HUD/controls, preserve above-hand actions |
| D39–D59 ranges/distribution/bar selection/mobile layouts/inspection | 3–4, 5a (D51 themed HUD), 7–9 (Replay metadata); D44 mobile observer canceled |
| D60–D66 stable Lobby/no prestart Seats/actions/Back/chapters | 3–6, 11; D64 mobile opener canceled; D66 default icons overridden |
| D67–D81 Win/Draw/score/final/Review/shared presentation | 5–6, 11–12; mobile read-only interpretation explicit |
| D82–D91 latest overrides + architecture/privacy/real evidence | 0–2, 4–12 |
| D240–D244 backend dependencies (not UI-owned implementation) | 0, 9–12 integration gates |

Self-review: no Experiment/v2/mobile-3D revival; no summary-only analytics; no local timer authority; same function/type names at consumers; pure result presentation distinct from controllers; five Review Focus classes mapped to tests; generated component compatibility and old API callers included. Milestones are independently reviewable, not separate public launches.

### Review corrections applied

- P1 HUD migration: Task 5a owns actual ActionDeck/candidates/audio/table-settings and `game/live-table.css`; theme/action regression and browser gates added. Renderer/art exceptions cannot exempt peripheral controls.
- P1 observer freshness: Task 10 separates transport heartbeat from successful Snapshot age/applied/pending revision, defines safe bounded renewal and tests live-heartbeat/failed-read scenarios.
- P2 Stop restoration: RunDetail.stop_requested plus Task 0/9 authoritative non-3D reads, reload/second-tab/response-loss/terminal tests; no observer dependency or local-only stop flag.
- P2 Replay availability: MatchPage carries BenchmarkMatchEvidence.replay_availability; Task 8 consumes it directly, Tasks 0/9 validate its source, and missing/unknown/failed/too-large records retain valid analytics without invented links.
- Task 12 includes all four corrected cases in coordinated-cutover acceptance. No product code or other agent's branch changed.

## What this handoff does not claim

The plan is implementation-ready for backend-independent tasks and explicitly gated for cross-agent contract integration. API agreement, final owner design signoff, runtime screenshots, baseline test health and performance evidence are not yet obtained. Metadata/doc lookups are not passing product tests. No cosmetic questionnaire, broad rebranding, component showcase, protocol rewrite or rollout automation was added.
