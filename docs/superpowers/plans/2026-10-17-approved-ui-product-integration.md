# Approved UI Product Integration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans for native execution, or superpowers:subagent-driven-development if the owner explicitly selects delegation. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Transplant the owner-approved interactive preview into the real frontend without importing its demo infrastructure or changing game/backend contracts.

**Architecture:** Modify the existing product owners, not the running preview or its transform pipeline. Keep existing API, query, socket, result-action, privacy and native-resource lifetimes; replace their presentation and the explicitly approved entry/login transitions. Consolidate accepted CSS into its existing owning stylesheets instead of stacking historical revision overrides.

**Tech Stack:** Installed React, TypeScript, Vite, Tailwind, shadcn/Base UI, Recharts, Phosphor, Geist, Vitest and Playwright. Node24.11.0/npm11.4.0 are the declared toolchain; no dependency changes.

**Spec:** Owner-approved checkpoint at `C:/Users/eitab/Documents/js/driichi/.worktrees/ui-ux-cutover/design-plans/ui-rollout-preview/owner-approval-current.md`, its latest handoffs, and `design-plans/run-dashboard-preview/owner-approval-v11.md` in that same reference worktree. PR24 remains the UI contract, with the latest explicit owner amendments taking precedence. The subsequent owner `go` authorizes plan→implementation→verification, not staging, commits, push, merge or publication.

## Global Constraints

- Integrate only in `C:/Users/eitab/Documents/js/driichi/.worktrees/ui-approved-integration`, branch `feat/ui-approved-integration`, baseline `2fcfee8178e31adf0c09ee66dd49459afe34e554`. Owner subsequently selected subagents, parallel where possible: parallel implementation is allowed only in separate task-owned worktrees, one writer per cwd; return unstaged patches/manifests for deliberate integration, never auto-merge/commit.
- The accepted preview worktree, 4187/4188 servers, screenshots, logs, licenses and original assets remain unchanged. Existing evidence is reference material, not proof of integrated correctness.
- Frontend source, affected tests and integration documentation only. No backend/Rust/core/migrations/Bot/API contract, dependency, renderer/camera/material changes.
- English product UI. Neutral semantic Light/Dark shell, Geist, installed Phosphor and genuine house components. No Cards in Join, no new artwork/fonts/frameworks/animations.
- Do not transplant `bridge.mjs`, seed/scenario/catalog/demo credentials, fake auth, memory storage, demo notices, query forwarding, cookie suppression or Vite string transforms. Retain real product storage, real `api.loginAdmin`, routing and authentication cleanup.
- Internal `p0`–`p4` remain keys, never visible text or accessible names. Participant identity, Seat, dealer and scores retain their separate authoritative mapping.
- Desktop table/observer eligibility remains `>=1024×600`. Phones must not initialize private observer requests/data, native modules, Character images for native-only surfaces, Canvas, Worker or audio. Public pre-match art is not native observer art.
- Preserve one steady active Worker, actual host/ref identity, fullscreen portal ownership, popup-first Escape→Exit, fallback, privacy purge and cleanup. StrictMode starts2/active1 is not automatically a leak.
- Preserve critical connecting/reconnecting/Stale/auth/error/terminal states, legal action IDs, deadline/cohort guards, authoritative lifecycle and uncertain mutation recovery. Remove ordinary `Connected`/`Step n of K` presentation only.
- Unknown Run-start time stays `--:--:-- elapsed`; no mount/Match/Replay-derived timestamp. No invented backend metrics, SSE, Resume or pagination.
- No staging/commit/push/merge/deploy, cleanup, dependency install, unowned process termination or silent provider/CLI fallback.

## Review Focus

1. A user leaves while lookup/join/login is pending: no late navigation or stale updates; duplicate submission must not create duplicate work. Task1 pins this.
2. A result expires, its cohort changes or access is revoked: no stale confirmation, even though Review and ordinary ready/expired copy disappear. Task4 pins this.
3. A phone deep link or desktop→phone resize reaches Run/Replay/post-match: no concealed private state or newly initialized native resources. Tasks3/4/6 pin this.
4. Live refresh, filtering and duplicate names interact with charts: stable identity, no ID leakage, no changed full-roster Radar scale, and cycling cannot overwrite an active reading interaction. Tasks5/6 pin this.
5. Browser fullscreen is denied, a popup handles Escape or Replay returns from final results to a chapter: local controls stay usable without duplicate Workers or stale portals. Tasks4/6 pin this.

## Workspace and baseline

- Isolated worktree created from the frozen source baseline. `frontend/node_modules` is a junction to the already installed reference frontend dependencies; no install or package-file changes.
- Baseline `npm run typecheck`: PASS, exit0 (the chained shell reached `npm test`).
- Baseline `npm test`: NOT PASS. The wrapper reached180s; the log subsequently reports5files/95tests passed and26 unhandled Worker errors: `Worker exited unexpectedly with exit code 3221225794 during starting state`.
- Logs: `design-plans/ui-product-integration/evidence/baseline-{typecheck,unit}.log`. JSDOM canvas/media warnings are separate observations, not an established explanation for native Worker startup failure.
- Read-only process inspection found no remaining Vitest runner. No processes were killed. A first PowerShell inspection had a shell-quoting error; it made no product changes and was corrected for the read-only inspection.
- **Owner decision:** `未確認を明記して実装へ`. Proceed with implementation while explicitly retaining the incomplete/failed baseline and recheck it at final verification. Do not call the baseline clean, disable failing tests or silently change pools/dependencies.
- **Owner execution decision:** `サブエージェントで可能なら並列的に実装`. Use isolated independent implementation lanes and task reviews, serializing shared owners and integration. All child roles remain pinned `openai-codex/gpt-6.1-sol:medium`; prior quota/runtime failure is not permission for provider/model/CLI fallback. No child fanout. Infrastructure failure stops the affected workflow and must be reported.

## Reference → product ownership map

All reference paths below are relative to the frozen `ui-ux-cutover` worktree, not the integration worktree.

| Accepted input | Product owner | Transfer / exclusion |
|---|---|---|
| `ui-rollout-preview/presentation.mjs` and `handoff-room-code-inline.md` | `frontend/src/app.tsx` EntryShell | Async existing lookup before navigation, inline404/transport distinction, pending/unmount guards. Exclude bridge/storage/routes transforms. |
| `ui-rollout-preview/r3-join.tsx`, latest R3/R4 CSS | `frontend/src/app.tsx` RoomJoin; `frontend/src/pre-match.css` | One portrait-left/form-right screen, actual Character icon radios, direct Lobby after binding. Reuse actual CharacterImage and real sessionStorage. |
| `lobby-center-presentation.mjs`, `lobby-center.css`, `handoff-lobby-footer-alignment.md` | `components/lobby-participant-card.tsx`; `pre-match.css` | Whole-frame170% art, bottom integrated footer, text in actual skewed owner, original foreground stroke. No transform plugin. |
| `r2/r3/r4-presentation.mjs`, `r4-theme.tsx`, `r4.css` | `components/app-shell.tsx`, new `components/theme-toggle.tsx`, theme styles; actual local gameplay/Replay/final actions | House Sidebar/default size, meaningful nav icons, footer Sign out; header/local theme cycle. Keep real provider/cookie/session behavior. |
| `screens.tsx` DemoLogin; `login-direct-redirect-amendment.md` | `app.tsx` AdminLogin | House form, validation/pending guards, clear fields then direct `/admin` only after real API acceptance. Never copy demo auth. |
| `r2-results.tsx`, R3/R4 result amendments | `app.tsx` HumanLobby; `game/result-presentation.tsx`, `game/result.css`, `game/gameplay.tsx` | Compact Win and dedicated Match END; authoritative ResultsPanel and recovery notices. Retain deadline guards, remove Review. |
| R3/R4 Replay amendments | `frontend/src/replay.tsx`, `replay.css` | Current result composition/local theme only; retain read-only playback, seek, chapters and native gating. |
| `r2-run.tsx`, `r2-observation.tsx`; v11 chart transform chain/geometry | `benchmark.tsx`, `benchmark-observer.tsx`, `benchmark-charts.tsx`, `benchmark.css`, `benchmark-match-detail.tsx` | Actual Run/Card/chart composition; preserve live-query hooks and observer ownership. No preview-file imports or fixture metrics. |
| `preview.css`, `correction.css`, `r2.css`, `r3.css`, `r4.css` | Existing owning product stylesheets | Carry final computed presentation only; exclude catalog, scenario/safety notice and superseded fixed-theme/tall-frame/name-plate overrides. |

Paths prefixed only by `components/` above are under `frontend/src/`. Read full affected bodies and grep callers/imports before each edit. Do not add exported APIs merely to imitate preview adapters.

---

### Task 1: Integrate Home, Join and real Admin sign-in transitions

**Files:** Modify `frontend/src/app.tsx`, `frontend/src/pre-match.css`; test `frontend/src/app.test.tsx`, `frontend/tests/entry.spec.ts`, `frontend/tests/pre-match-migration.spec.ts`, and any existing login-flow callers found by grep.

**Interfaces:** Keep `EntryShell()`, `RoomJoin(existing props)`, `AdminLogin()` and `navigate(path:string)`. Consume existing `api.lookupRoom`, `api.joinHuman`, `api.loginAdmin`; do not change their contracts. RoomRoute retains its second authoritative lookup and deep-link error handling.

- [ ] Replace old two-step/success-interstitial expectations with named cases: `missing_room_stays_on_entry` (404, same URL/history/code, linked FieldError), `transport_failure_is_not_missing_room` (503/retry), `pending_lookup_cannot_navigate_after_unmount`, `accepted_join_binds_before_lobby`, `accepted_login_clears_fields_then_opens_admin`. Assert each using the existing API mocks and deferred promises; assert no duplicate request after repeated submit.
- [ ] Run `npx vitest run src/app.test.tsx`; expect the new transition cases to fail on the old source, not infrastructure. Stop and classify any native runner error separately.
- [ ] Make `EntryShell.openRoom` async; preserve six-digit validation and exact copy:404 `Room not found. Check the code and try again.`, other failures `Could not check the room. Try again.`. Disable pending controls, retain typed code, clear errors on edit, ignore late completion after leaving.
- [ ] Replace RoomJoin's internal step machine/JoinSuccess with approved single form. Large portrait left, name and actual Character icon RadioGroup right; visually hidden circle but intact Base UI keyboard/focus semantics; equal Back/Join. Keep max64, loading/retry/full/error/voice cleanup. Persist accepted Participant binding to real sessionStorage before direct Lobby navigation; failed binding does not pretend success.
- [ ] Replace AdminLogin's native controls with Field/Input/Button/Alert. Add nonblank validation, in-flight/mounted guards and pending controls; only after real `api.loginAdmin` accepts, clear both controlled values before direct `/admin` navigation. No intermediate success page/Open Admin action. Preserve cookie-based auth, error details and401 session cleanup; do not log or persist entered credentials.
- [ ] Update affected Playwright flows to fill the one-screen Join and to mock the extra Home GET lookup; remove only obsolete steps/success-page assertions, retaining credential/privacy/failure coverage. Run focused unit and entry/pre-match checks to PASS after baseline recovery.

### Task 2: Integrate house navigation, theme and shared component presentation

**Files:** Modify `frontend/src/components/app-shell.tsx`, `frontend/src/styles.css`, `frontend/src/admin.css`, touched house-button/Card call sites in `app.tsx`, `benchmark.tsx`, `replay.tsx`; create `frontend/src/components/theme-toggle.tsx`, `frontend/src/components/theme-toggle.test.tsx`; update affected foundation/Admin browser tests.

**Interfaces:** New `ThemeToggle(): React.JSX.Element` consumes actual `useTheme()`. Existing AppShell variant contract and signOutAdmin are unchanged. Tasks4/6 reuse the same ThemeToggle in existing headerless action regions.

- [ ] Add tests `cycles_system_light_dark_system` and `announces_current_and_next_theme`: three clicks yield Light/Dark/System and accessible labels exactly `Theme: System. Switch to Light` etc. Keep system-preference propagation/storage tests.
- [ ] Run the new focused Vitest file to establish red; implement one house ghost icon button with44px target, opaque semantic resting surface/no border and visible focus. No viewport-fixed control or extra header.
- [ ] Replace ThemeSelect in actual public/Admin headers, at the far right. Implement installed SidebarHeader/Content/Group/GroupContent/Menu/Footer composition and default sizing; actual Rooms/Benchmarks/Replays Phosphor icons, bottom accessible icon-only Sign out. Preserve mobile-sheet close, modified-link navigation and real session clearing.
- [ ] Convert the preview's actual changed button/Card surfaces explicitly at their product call sites, preserving type/disabled/handlers/ref/ARIA. Do not run a global regex rewrite or modify house internals to hide incompatibilities. Remove duplicate Selected roster surface after confirming no caller needs it; retain authoritative participant/start-control guards.
- [ ] Run new unit tests and affected navigation/foundation checks. Assert theme placement relative to its real header, mobile Sidebar keyboard operation, no double shell, and sign-out privacy purge.

### Task 3: Integrate final Lobby frame and simplified routine state

**Files:** Modify `frontend/src/components/lobby-participant-card.tsx`, `frontend/src/pre-match.css`, HumanLobby presentation in `frontend/src/app.tsx`; update `frontend/tests/lobby-admin-migration.spec.ts` and affected `app.test.tsx` assertions.

**Interfaces:** Retain `LobbyParticipantCard({participant,own,previousSeat})` and actual DialogTrigger/Dialog semantics. No socket/Ready command/lifecycle API changes.

- [ ] Add browser assertions `name_center_follows_skewed_footer_slice`, `foreground_frame_stays_above_footer`, and face-visible samples at1440/390/320 text200 in Light/Dark. Measure the actual transformed footer slice, not the outer unskewed card; check glyph net skew0 and original1px stroke. Retain full-name Dialog Space/Escape/focus tests and missing-art/long-name cases.
- [ ] Establish red for the old art/frame geometry, then put the name inside the actual clipped skewed art owner with counter-skewed glyphs and original foreground frame stroke. Common original art rule is `width:170%;margin:-14% 0 0 -35%`; no per-Character offsets/material edits. Keep the accepted compact frame, integrated footer height `min(30%,3.5rem)`, Geist550/natural tracking/responsive18–24px. Reimu left transparency is accepted, not a requirement to crop both opaque edges.
- [ ] Remove routine Room-status/Match-readiness groups and connected-count/ordinary Connected labels. Keep Ready controls, host waiting, reconnection/recovery, terminal/errors and command rejection semantics. Keep hidden internal IDs as keys, not Dialog identity copy.
- [ ] Run focused Lobby/unit checks; include3p/4p, duplicate/64-character names, original Reimu/Marisa and fallback. Do not change real socket mount ownership or hide validation/blocker behavior that controls Start.

### Task 4: Integrate Win, dedicated Match END and read-only Replay

**Files:** Modify `frontend/src/game/gameplay.tsx`, `game/result-presentation.tsx`, `game/result.css`, `game/live-table.css` only for affected local controls, `app.tsx`, `replay.tsx`, `replay.css`; update result/gameplay/Replay unit tests plus `frontend/tests/result-surfaces.spec.ts`, `desktop-table-boundary.spec.ts`, `gameplay-hud.spec.ts`.

**Interfaces:** Keep `RoundWinSurface(existing props)`, `ResultsPanel({room,assets})`, `useResultRemaining` and existing result confirmation/store APIs. If shared with HumanLobby, expose `MatchVictoryPage({room,onReturn,notice})` from result-presentation with RoomSnapshot/ReactNode props; render actual ResultsPanel data, not preview state.

- [ ] Replace obsolete Review assertions with `no_review_and_one_legal_confirmation`, preserving tests for expired deadline, duplicate/changed step ID, shrinking cohort, accepted confirmation and revoked access. Assert zero stale confirm sends after each invalidation; live Win label derives from the existing deadline, while Replay has no live countdown/ack action.
- [ ] Establish red, remove Review state/Sheet/UI and only now-unused imports. Remove ordinary Win-ready status, ordinary Step count and exact expired paragraph, not expiry/action guards or necessary waiting/critical faults. Continue reads `Continue (Ns)` while legally available.
- [ ] Transfer compact balanced horizontal Win geometry: explicit `minmax(0,1fr)`/stretch and bounded18rem art track so no implicit orphan column; original art stays uncropped. Keep responsive result scrolling, score transfer/reduced-motion behavior and accessible actions.
- [ ] Render post-match authoritative final standings as a dedicated page, not modal, when existing validated standings are available. Retain invalid/missing result recovery, Return-to-room, connection/close/command faults and persisted lifecycle state; no synthesized standings.
- [ ] Add shared ThemeToggle beside actual gameplay Settings and existing final Return actions; Replay local controls/mobile metadata own theirs. Keep local z-order reachable above result overlays at1024×600 without a fixed global theme overlay.
- [ ] Update Replay presentation using the same result owners but retain playback/seek/chapters/speed/read-only audio/native gating. Add final→previous chapter assertion that awaits actual table remount-ready, counts steady active Worker1 and no result acknowledgements.
- [ ] Run focused result/Replay/privacy/fullscreen checks. Check popup-first Escape, denied-fullscreen fallback/return focus, renderer failure, phone no native loads and post-match resize. Do not remove state-machine assertions to make new copy pass.

### Task 5: Integrate approved charts and close supplemental timer coverage

**Files:** Modify `frontend/src/benchmark-charts.tsx`, `frontend/src/benchmark.css`, visible-name owners in `benchmark-match-detail.tsx`; create `frontend/src/benchmark-chart-geometry.ts` and `frontend/src/benchmark-chart-geometry.test.ts`; update `benchmark-charts.test.tsx`, `frontend/tests/benchmark-dashboard.spec.ts` and relevant chart foundation checks.

**Interfaces:** Keep `BenchmarkCharts({analytics,onOpenMatch})` and existing analytics types. Port `radarUpperBound(participants,completed):number` and `fitRadar(width,legendTop,labelWidth,labelHeight,rem):{cx:number;cy:number;radius:number}` into typed product geometry (parameter types from existing analytics). No statistics recalculation, normalization or backend summary API.

- [ ] Test shared-domain examples: rank percentages33.3→50,20→25,50→75,100→100; no-completed fallback25, cap100. Filtering cannot change the bound. Test fit output is finite/positive and fits the measured plot rectangle; do not substitute guessed native geometry.
- [ ] Add fake-timer tests with stable Participant IDs: no cycle at7999ms, next at8000ms, wrap; Pause/Resume; manual selection resets the interval even for the same ID; pointer/focus/hidden-page suspend; blur/visibility restoration starts a fresh interval; single roster/unmount leave no active timer/listener. Preserve selection across authoritative refresh and ensure it is not matched by duplicate display name.
- [ ] Establish red, then port only final v11 chart behavior from the full transform chain: solid soft chart-only palette/shape identities, `Line type="monotoneX"` cumulative/moving rank, rounded signed Net Bar, Player-axis Rank1–4 percentage Radar. Rank count follows authoritative3p/4p roster. No dashed line identities or p-ID suffixes in labels/tooltips/options/numeric accessible text.
- [ ] Use real plot measurement/legend bounds and stable text metrics for adaptive Radar radius; shared full-roster max×1.10 rounded up `[25,50,75,100]`, min25/cap100. Exclude visibility/filter state from domain and preserve measure cleanup.
- [ ] Replace old Performance/Consistency grouping with approved hero cumulative plus equal-height three support Cards and numeric Match history. Retain plot options, keyboard-readable scrolling/details, exact Match IDs, recent/full selection, signed values, zero/reference axes and error/empty states. Keep one numeric/history owner; export ChartCard only if real product callers require it.
- [ ] Run focused chart/geometry tests and affected browser chart checks; update old stacked-rank/dashed-ID/group assertions, not numeric/data/privacy contracts. Supplemental timer cases are new product evidence, not retroactive claims about old preview runs.

### Task 6: Integrate Run composition without changing observer ownership

**Files:** Modify `frontend/src/benchmark.tsx`, `benchmark-observer.tsx`, `benchmark.css`; update `benchmark-observer.test.tsx`, affected benchmark tests and `frontend/tests/benchmark-dashboard.spec.ts`, `desktop-table-boundary.spec.ts`, native observer lifecycle tests found by caller search.

**Interfaces:** Keep `useBenchmarkEvidence`, `StopRun`, `BenchmarkObserver({runId,roomCode,analytics})`, refresh/unknown-mutation handling and existing host/trigger/tableVisual refs. Use Task5's BenchmarkCharts and Task2's actual shell; do not copy adapter props/API bridges into product.

- [ ] Extend observer tests for approved Observation/Run leaderboard/Current Match layout while retaining coalesced reads, Stale freshness, session purge, old-lifetime response rejection, focus retention and denied-fullscreen behavior. Ordinary reading/details changes must not remount host/table or overlap requests.
- [ ] Transfer Run summary/progress/meta Cards, real stopped/interrupted/error/empty states, upper persisted Run leaderboard and lower authoritative Current Match rail. Focus Participant uses real house Select/portalContainer tied to existing fullscreen host. Elapsed remains unknown; muted normal-weight timestamp/n/range metadata. No Performance/context Card/flavor/private-all-hands caption.
- [ ] Keep actual polling/ref/status/lifetime implementation, replacing return presentation only as necessary. Retain Stop CAS/idempotency/unknown-refresh logic, existing unavailable Resume behavior and replay availability from bounded exact-ID metadata query0..50. Server scores/dealer/seat map drive Current Match, not chart standings.
- [ ] Verify phone initial deep link and desktop→phone switch: no observer API/module/worker/audio initialization after ineligibility, immediate concealed-state cleanup, aggregate charts/history still usable. Desktop mount maintains one steady active Worker; details/focus/fullscreen do not renew ownership unexpectedly.
- [ ] Run focused observer/Run/native/privacy tests; preserve current source/fixture PASS versus real backend/GPU limitations.

### Task 7: Integrated verification, bounded visual pass and review

**Files:** Affected source/tests above; evidence and report under `design-plans/ui-product-integration/`. No product feature/config/dependency additions in this task.

- [ ] Active LSP probe all changed source/test paths, using `source=lsp,scope=paths`. Timeout/push-only silence is inconclusive, not clean. Record auxiliary/preexisting warnings separately. Run `npm run typecheck` and `npm run build` with preserved logs/exit codes.
- [ ] Run affected unit files, then full `npm test` only after the baseline gate is resolved/explicitly waived. Run affected existing Playwright projects/specs using their actual configs and strict127.0.0.1 owned test server on a free port distinct from4187/4188. Unknown listener→stop. Do not weaken failed assertions or clean old caches/evidence.
- [ ] Finish all authorized implementation before one initial affected screenshot batch: Home/missing code, Join,3p/4p Lobby, login/Admin Sidebar, Ron/Tsumo/final, Replay chapter/final→chapter and Run charts/observer at representative1440,1024×600,390 and320 text200 in Light/Dark. Inspect saved actual pixels and geometry; no link/iframe/degraded stand-in for accepted Run.
- [ ] Conduct fresh review of source/spec/contracts plus pixels. If an independent reviewer is authorized/available, pin `openai-codex/gpt-6.1-sol:medium`, read-only, no fanout; provider/quota/runtime failure→report and stop, never fallback. Otherwise explicitly label parent self-review, not independent PASS.
- [ ] Combine review findings into at most one authorized correction/confirmation batch; preserve initial failures and manifests. New revision/broader validation beyond cap requires owner authorization.
- [ ] Check diff scope, absence of preview imports/fixtures/fake auth, no backend/contract/dependency edits, original licensed assets unchanged and original reference worktree tracked/index unchanged. Do not stage.
- [ ] Report exact commands/counts/failures, visual captures, unresolved baseline/LSP/live-backend/GPU/lifecycle/a11y limits. Product-source/test proof, owner visual acceptance and publication remain separate. Ask for integrated visual acceptance; no commit/PR publication without separate instruction.

## Self-review

- Latest accepted entry/login transitions:Task1; genuine shell/theme/components:Task2; final170% footer/local-coordinate frame:Task3; Win/final/Replay and deadline/native boundaries:Task4; approved v11 charts and missing timer coverage:Task5; actual Run/observer/status lifetimes:Task6; bounded evidence/review/publication boundary:Task7.
- All five review-focus conditions have named coverage in their owning tasks. Existing API/query/socket/game interfaces remain authoritative; only ThemeToggle, typed chart geometry and (if shared) a small final-page composition are new product exports.
- Excluded deliberately: backend gaps/Resume/SSE, dependency and renderer redesign, fake demo auth, historical transform plumbing, preview cleanup and publication.
- No product code was changed while writing this plan. Owner has now selected isolated parallel subagents where practical and explicitly waived a clean baseline as a prerequisite, not waived final validation or the duty to report native Worker failures.
