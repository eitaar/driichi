# driichi UI/UX — Grill with Docs (Rounds UI-1–UI-5)

Date: 2026-10-14
Status: **UI-1 Q237–Q246, UI-2 Q247–Q258, revised UI-3 Q259–Q274 and UI-4 Q275–Q292 decided; limited UI-5 Q293–Q300 awaiting owner. Documentation only.**
Scope: shadcn/ui (Base UI) + Tailwind-based English-only redesign for **all** public/admin pages, starting with Run-first Benchmark Dashboard. One public all-pages cutover; no planned old-UI rollback. Dedicated existing Three.js/Pixi Mahjong renderer remains.

## Dependencies and explicit cancellations

- Owner canceled **all A/B Experiment features** (previous Q234 C).
- Owner now also canceled the **breaking Benchmark Bot protocol v2** and its handshake/required checkpoint/reset/migration/preflight infrastructure. This is a UI/core Benchmark product refresh with the **existing MJAI/MCP/Built-in Bot protocols unchanged**.
- Use the current Rust RoomActor/SQLite, existing Run history/stats and live Admin projection. The existing frontend `frontend/src/benchmark.tsx` provides a Run list, Run details, table, history and current 1-second HTTP polling; `frontend/src/routes.ts` maps `/admin/benchmark` and `/admin/benchmark/runs/:id`. No shadcn/Tailwind/Base UI dependency exists yet. UI designs are NOT implemented.
- Baseline v1 semantics: 1–1,000 consecutive successfully persisted Matches per Run, selected 3p OR 4p ruleset, fixed selected Bot roster, round-robin seat rotation, average rank / first-place rate / accumulated table-relative net score. Preserve current 3p MJAI constraint. `Stopped` vs `Completed` vs `Interrupted` vs `Failed` must be clear.
- Previously approved UI invariants: clear English copy, shared primitives/tokens, keyboard/screen-reader support, real Room/Live/Replay/Benchmark score-to-Seat visual correctness, **one desktop/large-screen live 3D table** with fullscreen across consecutive Matches, role-aware private-hands viewing, existing renderer preserved; actual-data not fabricated; system/light/dark; user selected no old-UI rollback. **Owner Q244 C now overrides any older claim that the mobile Run-detail dashboard must show or prioritize a 3D table: mobile is chart-first with the live table hidden.** Owner Q256 B explicitly permits opening the single live table **only on manual request**, fullscreen (with an accessible in-page fallback). Neither hidden rendering nor automatic fetching of concealed hands is implied.
- **Do not overengineer:** don't create new generic design systems, multiple renderers, multi-table viewers, Experiment panels, protocols or wire-level affordances. This is design-only and Draft PR-only; no code, CI or merge.

## Owner decisions — Round UI-1 (2026-10-14)

Answers: **Q237 A, Q238 A, Q239 A, Q240 A, Q241 A, Q242 A, Q243 A, Q244 C, Q245 A, Q246 A.**

### UI-D1. Restrained modern admin visual language — DECIDED (Q237 A)

A polished, modern control-room/software UI with clear hierarchy and balanced information density/spacing; restrained transitions, not arcade-wide effects, terminal density or decorative pseudo-poetry. This is the aesthetic direction for the entire English-only app, while existing dedicated Mahjong graphics remain game-specific.

### UI-D2. Desktop Run-detail table-first two-column layout — DECIDED (Q238 A)

One live actual Three.js table occupies approximately **2/3 of the main detail region**, with a **~1/3 Leaderboard/Run status side rail**; cumulative trend graph and per-Match Replay history appear below. Do not create a second or dummy 3D table.

### UI-D3. Searchable Run index table — DECIDED (Q239 A)

`/admin/benchmark` shows a clear and searchable list of Runs with state, completed/target progress, game mode and Bot roster plus a prominent **New Run** action. Existing history stays discoverable; no giant per-Run card gallery required.

### UI-D4. Small New Run form, then genuine existing roster flow — DECIDED (Q240 A)

Use a brief form/sheet to create a Benchmark Room with name, exactly one 3p/4p mode and existing time control, then navigate to existing Admin Room to **connect/select exactly enough eligible software Participants** and start a 1–1,000-Match Run. Explicitly show preconditions. Never pretend create-and-start is a single API operation or auto-create missing external Bots.

### UI-D5. Ordinary Benchmark statistics, broadened by later UI-2 Q252 B — DECIDED (Q241 A; Q252 B supersedes single-chart limit)

Visible Leaderboard per software Participant: **average rank, first-place rate and cumulative score relative to table mean**, with **multiple advanced analysis charts visible by default** under Q252 B (Q241's old one-chart limit superseded). Allow collapsing/paginating complete per-Match results/Replay history. Graph families/interaction and data feasibility are unanswered Q259–Q261; **do not revive canceled Experiment/Bootstrap CI metrics or invent model inference measurements**.

### UI-D6. Contextual Run action UX — DECIDED (Q242 A)

Run header holds unobtrusive **Stop after current Match** with a confirmation and pending feedback; **Resume** appears only if an `Interrupted` Run is verifiably safe to resume using existing Bot/Room protocols, **never** from terminal `Stopped`. Actions must remain server-authorized, revision-safe and non-duplicating; the UI cannot promise impossible recovery.

### UI-D7. Explicit Participant focus in desktop live 3D — DECIDED (Q243 A)

Select a Participant with a compact labeled focus control, accessible by pointer/touch/keyboard, not a cycling camera. Follow Participant ID as Seat rotates; protect private all-hand Admin data. An in-game camera rotation is not an identity switch.

### UI-D8. **Mobile Run detail is chart-first; hide live 3D table** — DECIDED (Q244 **C**, overrides A suggestion)

**On small-screen Run detail, the live 3D Mahjong table must NOT be displayed by default.** The content focuses on performance charts/Run statistics rather than an automatic/shrunken table. Essential Run progress, valid Admin operations, status and accessibility remain available; hiding a table may **not** hide required Stop/Resume or show a fake Live indicator. This is the owner's intentional deviation from the previous proposed mobile-table-first fallback. UI-2 **Q256 B** creates an explicit opt-in exception: mobile detail remains chart-first and inline-table-free, but a user can choose **Open Live Table** to view that same single table fullscreen or via accessible in-page fallback. No autoplay, background renderer or unsolicited private-hand fetch. Further return/transition UX is Q264. Desktop/large-screen live 3D remains mandatory.

### UI-D9. Honest live/reconnection/empty/terminal states — DECIDED (Q245 A)

Distinguish `Live`, `Reconnecting`, `Stale`, `Waiting for next Match`, `Stopped`, `Interrupted`, `Failed` and `Empty`. Show known historical snapshot labeled stale, not as current. Clear retry/next steps; stop relying on a generic indefinite spinner.

### UI-D10. One role-aware application navigation shell — DECIDED (Q246 A)

Across Home, Room, Lobby, Live, Admin, Benchmark and Replay, use one shared shadcn/**Base UI** + Tailwind component/tokens vocabulary with role-appropriate links, safe deep links and clear route context. This does not grant Admin information to non-Admin users; existing public/private game views remain separate.

### Recorded supersession / unresolved UX consequences

- **Q244 C supersedes the earlier generic "mobile fullscreen live table fallback" proposal for the default mobile Benchmark Run-detail surface.** Existing game/Room and Replay views are not banned on mobile by this choice. Mobile manual entry is expressly approved by **Q256 B**; details of closing/returning to chart position and transition behavior remain Q264.
- No change to abandoned **breaking Bot v2 protocol** or canceled **A/B Experiment** design.
- The new **Run index** and **Run detail** use currently existing routes and data; additional UX can be a refactor, not a new backend scheduler.
- "Live" status represents fresh authorized server data, not a promise that a connection exists or that the Run is currently Playing.
- No UI/frontend/backend code, visual snapshot or CI was generated/executed by this documentation interview.

## Existing UX observations (grounded in main branch)

- Current Run list also includes a room-creation form and then navigates to Admin Room for roster selection; it is **not** already a one-click create-and-start flow.
- Run details put status, stats, history and an expandable live `<details>` region on one vertical page. Live observer fetches `getBenchmarkLive` every **1 second** while expanded and renders `ThreeTable`.
- Run stats are already average rank, first-place rate and cumulative net scores; per-Match history links to Replays. No need for a research dashboard.
- UI/UX should respect real permission and connection states, not introduce a false `Live` indicator when a stream/poll is stale. The prior normal Room seat-to-score visual mismatch remains unverified.
- **Revised owner questionnaire note:** the earlier UI-3 original option set was rejected as biased/nonviable and has no decision authority. User answered the replacement questionnaire with realistic A/B/C alternatives. Only the **revised Q259–Q274 answers** below count.

## Owner decisions — Round UI-2 (2026-10-14)

Answers: **Q247 A, Q248 A, Q249 A, Q250 A, Q251 A, Q252 B, Q253 A, Q254 A, Q255 A, Q256 B, Q257 A, Q258 A.** The user-supplied detailed text for **Q252 B** states "*複数の高度な分析グラフを標準表示*"; that owner wording **takes precedence** over the original old draft Q252 B text suggesting a stacked area chart. No stacked area chart is implicitly approved.

### UI-D11. Neutral/Slate with restrained teal/green semantic accents — DECIDED (Q247 A)

Share a neutral/slate light/dark palette across all English-only pages; use restrained green/teal for interactive accents and meaningful status, while true Mahjong artwork and the existing real 3D table provide game identity. Maintain System/Light/Dark selection. Avoid an all-green UI or meaningless grayscale state indicators.

### UI-D12. Adaptive desktop table/sidebar, no drag-resize — DECIDED (Q248 A)

At comfortable desktop widths, use the Q238 **~2/3 live 3D + ~1/3 leaderboard** ratio. When width is insufficient, **stack** the leaderboard rather than shrinking text, forcing horizontal scroll or inventing persisted splitters. **Q244 C still overrides small mobile Run detail: its default view hides live 3D entirely.** Breakpoint constants are implementation details.

### UI-D13. Search/Status/Mode/filter/paginate Run index — DECIDED (Q249 A)

Search visible Run/Room names and Bot display names, filter by persisted Run Status and selected game Mode, show recent Runs first, and paginate with a bounded number of visible entries. Do not imply backend full-text search currently exists; use a simple compatible API or locally filtered loaded pages according to scale. No saved custom report builder.

### UI-D14. Clear roster constraints before Start — DECIDED (Q250 A)

Show selected/required Bot counts and exact eligibility reasons, including existing **3p MJAI exclusion** and disconnected/non-software participants. Disable Start while invalid, but server still performs authoritative validation. No automatic creation of missing external Bots.

### UI-D15. Rank by ascending average placement with stable ties — DECIDED (Q251 A)

Leaderboard default is **ascending average rank** (1.00 best), with stable Participant ID tie-breaking; include first-place percentage, cumulative table-relative net score and honest unavailable markers when no persisted results. Current Seat is not an identity and does not drive sorting.

### UI-D16. Multiple advanced non-Experiment charts visible by default — DECIDED (Q252 **B**)

Explicit user choice: **"複数の高度な分析グラフを標準表示"**. This **overrides Q241 A's earlier compact/single-graph limit**, but preserves the Q238 A layout: real 3D table + leaderboard remain desktop-first, advanced plots occupy supporting dashboard regions **below** rather than replacing the table. Existing rank/score/per-Match history can support richer ordinary Run visualizations; no A/B comparisons, Bootstrap CI, research study dashboards, invented inference latency or hidden mandatory per-model instrumentation. Q259 chooses which plot families; Q260 chooses visible placement/density; Q261 chooses allowable data sources and unavailable-state treatment. **Do not assume the rejected old Q252 B stacked-area example**.

### UI-D17. Recent completed Matches + paginated full Replay history — DECIDED (Q253 A)

Show recent persisted Matches first and an expandable/paginated full historical results table with per-Match Replay links. Do not render thousands of 3p/4p rows into the default DOM or omit committed history.

### UI-D18. Confirm Stop, show server-acknowledged pending, guard Resume — DECIDED (Q254 A)

Confirm "**Finish current Match, then stop**". After an authenticated server acceptance, show "**Stopping after current Match**" and prevent duplicate requests. Resume is offered only on `Interrupted` when server/roster proves safe under existing unchanged MJAI/MCP/Built-in protocols; otherwise state concrete missing/disconnected Bot reasons. No optimistic false Running or raw token leakage.

### UI-D19. Visible labeled Bot/Seat focus above desktop/Fullscreen 3D — DECIDED (Q255 A)

Place accessible Bot-name + current Seat focus chips/dropdown near the real 3D table, available in fullscreen. Preserve underlying `participant_id` across Seat rotation, retain keyboard selection and focus restore, never mutate game identity/state to "change camera". Admin concealed-hand access remains protected.

### UI-D20. Mobile chart-first, optional manually opened *single* fullscreen table — DECIDED (Q256 **B**; refines Q244 C)

**Mobile Run detail stays chart-first and has no default inline, autoplay or background-rendered 3D table**, per Q244 C. Display an explicit `Open Live Table` action. Only on user activation open **the same one actual Three.js table** fullscreen (or accessible in-page fallback), with valid Admin authorization and no second Match/table. When closed, return to the stats/graph context. Details of restoration and inter-Match behavior depend on Q264. Neither blanket ban (Q256 A) nor default 3D inline display (Q256 C) is active.

### UI-D21. Clearly stale snapshot and recovery time — DECIDED (Q257 A)

Keep the last authorized snapshot with clear `Stale · last updated <time>`/retry or Reconnecting messaging, separate from persisted Run statistics. Never claim a dead stream/poll is Live; if auth is revoked, do not continue exposing concealed-hand observations.

### UI-D22. Compact desktop Admin rail, minimal public topbar, accessible mobile menu — DECIDED (Q258 A)

Shared role-aware shell/tokens: desktop Admin gets a compact sidebar/rail and current route context; public Home/Room/Lobby/Live uses a simple topbar; mobile uses an accessible labeled collapsible navigation menu with clear current page and safe authorized deep links. Do not show Admin-only affordances to a Guest or Spectator.

### Cross-round precedence (binding)

- **Q252 B supersedes UI-D5/Q241 A ONLY on the number/density of ordinary Run analysis plots.** Q241 A's key headline metrics, one simple net-score trend as a minimum, and Q238 A's table-first desktop two-column layout survive. Plotting advanced metrics means using **genuine available per-Match evidence**, not restoring canceled A/B Experiments or promising unmeasured model execution times.
- **Q256 B is the explicit opt-in mobile exception to Q244 C.** No inline table appears by default, but `Open Live Table` on explicit user request is approved. This is not a reversal to mobile autoplay/table-first.
- The previously approved **existing MJAI/MCP/Built-in Bot protocol unchanged**, **A/B Experiment canceled**, **English-only shadcn/Base UI**, **single public all-page UI cutover** and **no old-UI rollback** remain binding.
- UX choices recorded in documentation only; no renderer/UI/backend was edited, no tests, CI or deployment executed, and PR stays Draft.

## Confirmed Dashboard wireframe — post Round UI-2

```text
DESKTOP /admin/benchmark
[Shared Admin navigation                                 Theme]
[Benchmark Runs]                                       [New Run]
[Search Runs] [simple state/mode filters]
[Run name | Status | # Completed/Target | Mode | Roster | Open]
[...] real persisted Run results and history

DESKTOP /admin/benchmark/runs/:id
[Run title | Running | 40/100 | 4p Red East] [Stop after Match]
+---------------------------------------+-------------------+
| ONE existing real 3D Live Table       | Leaderboard       |
| ~2/3 of main width                    | ~1/3 width        |
| Participant focus, Live/Stale,        | Avg rank / 1st% / |
| Fullscreen (same one table)           | Net score         |
+---------------------------------------+-------------------+
[Multiple default-visible ordinary Run analytical plots: types Q259]
[Paginated Match results + Replay history]

MOBILE /admin/benchmark/runs/:id   (Q244 C, NOT previous A)
[Run title, progress, status, safe contextual actions]
[Multiple ordinary Run performance graphs / leaderboard]
[History/replay navigation]
[Open Live Table]  <-- explicit opt-in; ONE fullscreen table only
NO default inline/autoplay/background-rendered live 3D table.
Q264 determines close/return semantics.
```

All positions are **logical UX composition**, not a functioning preview, frontend code, tested breakpoint, or assertion that a particular backend SSE endpoint exists. All matches/rosters/graph values shown elsewhere are illustrative unless backed by real storage.

## Owner decisions — Round UI-3, REVISED questionnaire (2026-10-14)

**Authoritative owner answers:** `Q259 C; Q260 C; Q261 C; Q262 B; Q263 A; Q264 B; Q265 B; Q266 B; Q267 B; Q268 B; Q269 A; Q270 B; Q271 B; Q272 B; Q273 B; Q274 C`.

The previous, **biased original UI-3 option set is explicitly DISCARDED**. For example its old Q259 C meant "only one graph", while the **revised Q259 C** means four balanced plots. Do not infer design decisions from the rejected original A/B/C definitions. These owner-provided revised descriptions are canonical.

### UI-D23. Balance trend and volatility in four default charts — DECIDED (revised Q259 C)

Display four normal-Run analytics views **by default**: **cumulative table-relative net score**, **moving-average rank**, **rank/placement distribution**, and **per-Match score variation**. Use valid persisted `benchmark_results` Rank, Score, Seat, Participant ID and the existing Run/Match records. Do not invent model inference, Experiment confidence intervals or research fixture data. Q275–Q277 resolve time range, distribution and score-variation presentation without changing the agreed four plot families.

### UI-D24. Hierarchical full-width primary chart + paired secondary plots — DECIDED (revised Q260 C)

Desktop Run detail retains the real one 3D table at ~2/3 and Leaderboard ~1/3 at the top. **Below it**, a primary analytics plot uses the **full available width**; secondary plots use a two-column grid when room permits and stack at narrow widths. This is a main/sub hierarchy, not one oversized graph replacing the table. Consistent with Q274 C.

### UI-D25. Distinguish provisional from established views — DECIDED (revised Q261 C)

Render honestly computable chart values even for a few recorded Matches, but **label small-history analyses as provisional** and indicate observed sample size. Do not imply statistical significance, invent missing Match rows or synthesize unknown data. After sufficient data, display normally; exact provisional treatment remains Q279. No Bootstrap CI.

### UI-D26. Mobile categories default-expanded; collapsible when desired — DECIDED (revised Q262 B)

On narrow screens, the default Run detail remains **chart-first** with multiple visible graphs grouped into explicitly labeled default-expanded sections such as **Performance** and **Consistency**, with optional collapse. Run progress and valid controls remain easy to reach; history follows. No inline or background-rendered 3D table. The manually opened live viewer remains separately allowed under Q256 B.

### UI-D27. On-demand mobile fullscreen overlay with restored context — DECIDED (revised Q263 A)

An explicit `Open Live Table` opens the **same single authorized real 3D table** above the Run-detail screen, fullscreen (or accessible in-page equivalent if native fullscreen fails). It can persist across automatically starting Matches. Closing returns to the **same Run-detail scroll position and keyboard focus**, and unmounts/stops rendering confidential hand projections from the viewer. No navigation to a new tab per Match.

### UI-D28. Home two-column function + character art — DECIDED (revised Q264 B)

Home uses a usable **Join Room form** in one column and existing character/game visuals in the other, balancing practical entry and personality. Do not gate Join behind an intro animation. On phones content flows responsively; mobile ordering still a legitimate question Q282.

### UI-D29. Join Room as two clear steps — DECIDED (revised Q265 B)

After Room lookup, **Step 1** presents Room information and name entry; **Step 2** chooses a Character; then Join/confirmation as currently permitted. Keep inline errors and accessible previous/next behavior, without introducing account requirements. Whether steps share one URL or route-specific navigation is Q283.

### UI-D30. Lobby is Participant list first — DECIDED (revised Q266 B)

Present **Seat, Participant name, connection/readiness state** in a clear list rather than a centerpiece Seat diagram. Show current Room code and the next allowed action; never synthesize Ready from mere connection or automatically fill a missing Seat. How rows/cards communicate list state is Q284.

### UI-D31. Responsive normal Match action panel — DECIDED (revised Q267 B)

Keep the real live 3D Mahjong table primary. **Desktop:** legal action panel adjacent to the table. **Mobile:** action buttons in a reachable bottom Dock. Display current legal actions, timing, winds and correct Participant/Seat/score semantics without leaking Admin all-hands view to guests. Panel behavior in complex decision windows is Q285.

### UI-D32. Replay is navigated by Kyoku — DECIDED (revised Q268 B)

Use **Kyoku (局)** as the principal navigation unit (e.g., East 1 → East 2 → …), with event-step/playback controls **within** a selected Kyoku. Preserve real existing Replay frames and renderer, support longer Match browsing. The specific Kyoku navigation component is Q286; do not create a second replay engine.

### UI-D33. Admin Rooms uses Master–Detail layout — DECIDED (revised Q269 A)

The common Admin shell shows a **Room index/rail at left and selected Room's controls and status at right**, allowing quick Room switching while displaying Participants, Bot Tokens where authorized, readiness and configuration. On mobile a safe single-pane transition is expected; exact presentation is Q287. Protect role/private-hand boundaries.

### UI-D34. Shell + peripheral HUD components respond to theme — DECIDED (revised Q270 B)

Light/Dark applies to the app shell **and surrounding game HUD frame/action controls**, not to tiles, tabletop or the real Three.js/Pixi graphics themselves. Use accessible contrast, text labels and restrained semantic accents; no global recoloring of Mahjong assets. Theme integration and readable surrounding overlays are Q288.

### UI-D35. Per-Match results open a Detail Drawer — DECIDED (revised Q271 B)

Selecting a persisted Match in Run history reveals its **Participant placements, scores and Replay link** in a non-destructive side/bottom **Detail Drawer**, preserving the Run list and context. No Experiment comparison page. Mobile Drawer behavior and accessibility is Q289.

### UI-D36. Real-time Ready Panel for Room setup — DECIDED (revised Q272 B)

After Room creation, show the live selected/required eligible Bot count, actual connection state and concrete blockers, and enable `Start Run` only when current/server-validated conditions are satisfied. Reuse the existing Room/Run flow, no wizard/additional Bots. Its position relative to roster/Start is Q290.

### UI-D37. Toast for transient actions, inline for blocking states — DECIDED (revised Q273 B)

Use nonblocking **Toast** for short success confirmation; persistent errors, empty state, unauthorized/stale observations and blocking setup conditions appear **inline at the affected surface**, with Retry or next action. Never use a Toast as the only record of an important failure. Use the same shared shadcn/Base UI components throughout; nuanced feedback is Q291.

### UI-D38. Primary + secondary analytics under table — DECIDED (revised Q274 C)

Desktop priority: **large real 3D table and compact Leaderboard first**. Analytics begins below with one main **full-width** chart, then subordinate charts as a two-column layout; stack responsively. This reinforces UI-D24 and **Q252 B's multiple default-visible advanced graphs**. Primary chart ordering is still a choice Q292.

### Invariant summary after revised UI-3

- Main desktop: one real table + Leaderboard, below it **4 default-visible** genuine-result charts in a primary+secondary hierarchy and Drawer-based results/Replay history.
- Mobile: default **chart-focused**, grouped expanded Performance/Consistency sections; **no inline live table**. Explicit tap `Open Live Table` opens one real fullscreen overlay; close restores context. Keep User/Q244 C and Q256 B both operative.
- Public Home: two-column functional Join + existing game/character visuals; join is a **two-step** name then Character flow; Lobby uses a **Participant list**; normal 3D gameplay has **side control rail on desktop, bottom Dock on mobile**.
- Replay: **Kyoku-first**; Admin Rooms **Master–Detail**. App-shell & peripheral HUD respond to System/Light/Dark, assets are not recolored. **Toast + inline errors** by persistence.
- Breaking Benchmark Bot Protocol v2 and all A/B Experiment features remain **canceled**. Real unchanged MJAI/MCP/Built-in contracts, SQLite and existing Run semantics prevail.
- No implementation or UI source changes have been approved; Q275–Q292 below are pending user decisions, not assumptions.

## Owner decisions — Round UI-4 (2026-10-14)

**Authoritative owner answers:** `Q275 B; Q276 A; Q277 B; Q278 B; Q279 B; Q280 A; Q281 A; Q282 A; Q283 B; Q284 B; Q285 B; Q286 A; Q287 B; Q288 A; Q289 B; Q290 B; Q291 B; Q292 A`. **18/18 answered**, with 11 B and 7 A. The original A/B/C labels for UI-4 refer to the *realistic option definitions archived below*, not the rejected original UI-3.

### UI-D39. Recent 50 Matches default, optional whole Run — DECIDED (Q275 B)

Time-series analyses default to **most recent 50 durably completed Matches** (or all available if fewer), with an obvious **All Matches** switch. Switch does not change Run completion counts, percentages or stored results. How the hero cumulative plot's vertical baseline communicates absolute Run totals versus local-window change remains the only substantial analytical presentation question Q293.

### UI-D40. Rank distribution is Bot-wise 100%-stacked horizontal bars — DECIDED (Q276 A)

Each Participant gets a horizontal full-proportion bar segmented into rank 1/2/3/(4), using only completed persisted Match outcomes and honest per-Bot denominator. In a 3p Run, do **not** invent a fourth place or fourth Bot; accessible names/percentages must be available without relying on color alone.

### UI-D41. Per-Match score changes are selected-Bot bars — DECIDED (Q277 B)

Render signed **per-Match table-relative net-score bars** for one explicitly selected Participant, preserving Match order and current active range. Selection can switch to any participant in the roster. Do not portray the final raw score as a relative delta or confuse a seat reassignment with a different Participant; initial selection is Q294.

### UI-D42. One shared provisional-data banner over Analytics — DECIDED (Q278 B)

When completed observations are too few to support stable interpretation, show a clear **Analytics-level provisional-data notice** containing the number of durable completed Matches, while still rendering mathematically valid observations. All charts retain accessible axes/denominator or visible sample descriptions; a screen-reader or scrolled graph must not quietly masquerade as statistically established. Threshold, copy and statistical caveats are implementation-owned, not a new owner questionnaire; do **not** assert arbitrary confidence bounds.

### UI-D43. Mobile default-open category sections with jump menu — DECIDED (Q279 B)

Group four charts into **Performance** and **Consistency** sections with headings and an accessible compact jump menu on mobile. **Both categories are expanded by default**; owner may collapse them independently. No default 3D table inline, even when categories fold; Q244 C remains operative.

### UI-D44. Portrait and landscape both supported for opt-in live overlay — DECIDED (Q280 A)

Explicit `Open Live Table` renders the same authorized single 3D table in an accessible fullscreen overlay on phone; controls reflow in portrait and landscape. No forced device rotation or landscape-only viewer. Manual exit returns to charts and restores scroll/focus as UI-D27 requires.

### UI-D45. Mobile Home shows Join first, character artwork below — DECIDED (Q281 A)

At smaller widths, put the functional 6-digit Room Join input/action **before** the existing decorative character/game visual section. Desktop retains the approved two-column Join+art. Do not hide Join behind branding/animation.

### UI-D46. Two-step Room Join stays at the same URL — DECIDED (Q282 A)

Under the existing Room Join path, use local **Step 1: Room confirmation/name** then **Step 2: Character**; an explicit `Back` returns to Step 1 without erasing accepted input. Do not add invented signup, extra routes or new Bot/game protocol. How native browser Back interacts with the steps may be resolved by the UX choice Q299.

### UI-D47. Lobby uses large character Participant cards — DECIDED (Q283 B)

Show Seat, Participant name, actual presence/readiness and larger existing Character art in an easy-to-scan **vertical card list**; do not use a central Seat-map-only page. Each Seat identity remains visible; card sort and reconnect behavior can be constrained by Q296.

### UI-D48. Gameplay action panel is contextual only — DECIDED (Q284 B)

Desktop legal-action side panel appears **only when there is an actionable Decision**; outside that period, show small table-level status/timer cues and preserve more space for the real Mahjong renderer. On mobile the approved bottom action Dock is likewise contextual. Any legal action must remain reachable by keyboard/touch and distinguish expired/blocked choices; exact side-panel appearance is Q297. Do not block gameplay or show Admin concealed information to guests.

### UI-D49. Replay navigation is vertical Kyoku chapters on desktop, dropdown on mobile — DECIDED (Q285 B)

Preserve one renderer and Replay data; on desktop put an accessible vertical Kyoku chapter list next to the Replay table, on phones use a labeled dropdown. Event-level controls stay inside selected Kyoku. Exact jump-then-playback behavior is Q298; do not create a competing Replay engine.

### UI-D50. Mobile Admin Rooms uses List → Detail screens — DECIDED (Q286 A)

Desktop Admin continues Master–Detail split. On mobile, selecting a Room from a list opens that Room's detail/actions as a full-width screen with a clear Back to the list; preserve access controls and current Room context without a cramped split.

### UI-D51. Theme-aware restrained translucent HUD — DECIDED (Q287 B)

Game action/HUD frame uses **subtle semitransparent surfaces** integrated with System/Light/Dark app controls; focal/urgent/legal action controls must retain verified solid-enough contrast and text affordances against moving tiles. Preserve existing Mahjong table/tiles/materials unchanged. Reduced-transparency preference fallback and visual testing are implementation-owned.

### UI-D52. Match Detail uses right Drawer on desktop, bottom Sheet on phone — DECIDED (Q288 A)

From Run history a click/tap opens a **right-side Drawer** (desktop) or **bottom Sheet** (phone), showing persisted Participant placement/points and a Replay navigation action. On close restore original history scroll and keyboard focus; no second Results database/view required.

### UI-D53. Ready Panel inline immediately before Roster/Start — DECIDED (Q289 B)

In the existing Benchmark Room setup UI, display a live **Ready Panel immediately above the selected/eligible Bot roster** and put `Start Run` nearby/below it. Show selected/required counts, actual connections, and the deployed 3p MJAI restriction; disable Start until eligible. No sticky duplicate Ready service, fabricated Bots or additional wizard.

### UI-D54. App-shell-level temporary success Toasts persist through normal navigation — DECIDED (Q290 B)

Short action success messages (e.g., new Room created then routed to that Room) should remain briefly available **across normal client-side route transitions** in the shared shell; blocking/recoverable/auth/stale errors must stay inline in the correct surface. A toast must not claim success before server confirmation or leak Bot tokens/private hands.

### UI-D55. Four chart selections remain independent — DECIDED (Q291 B)

Each chart may have its **own legend and independent Participant focus/series toggles**. All four charts use a consistent Participant-name/color mapping, but changing a Bot in the selected-Bot score-change chart does **not** silently filter the other three; avoid hidden global selection complexity. Accessible patterns must be usable without depending only on color.

### UI-D56. Cumulative relative net score is the full-width hero chart — DECIDED (Q292 A)

Beneath the desktop live 3D + Leaderboard, render **cumulative table-relative net score** as the prominent **full-width hero**. Secondary analytics below: moving average rank, 100%-stacked rank distribution, and selected-Bot Match score-change bars, arranged in responsive two columns. On mobile all four remain default-visible inside expanded Performance/Consistency sections without default live inline 3D.

### Final coherent UX composition after UI-4

```text
DESKTOP /admin/benchmark/runs/:id
[Shared compact Admin rail]   [Run | status | progress | Stop/guarded Resume]
+------------------------------------------+------------------------+
| ONE true live 3D Table (~2/3)             | Leaderboard (~1/3)     |
| focus Bot identity/Seat, fullscreen       | Mean rank / 1st / Net  |
+------------------------------------------+------------------------+
[ HERO: Full-width cumulative relative net score · Recent 50 | All ]
+-------------------------+------------------------------+
| Moving-average rank     | Rank distribution 100% bars  |
| Per-Match delta bars    | (responsive grid)            |
+-------------------------+------------------------------+
[Analytics provisional notice when necessary]
[Recent Match history → desktop right Drawer → Replay]

MOBILE Run detail (Q244 C + Q256 B + Q263 A)
[Run name/status/progress/valid Stop or Resume]
[Performance / Consistency category jump menu]
[Sections OPEN; 4 chart types visible in scroll; no inline 3D]
[Leaderboard/history → bottom Match Sheet]
[Open Live Table] -- explicit action only --> [ONE portrait/landscape fullscreen overlay]
                                             [Close → same prior scroll/focus]

Home desktop: Join + existing character/game art split.
Home mobile: Join FIRST → character visuals below.
Room Join: same URL, Step 1 name/Room, Step 2 Character, explicit Back.
Lobby: vertical LARGE Character/Participant cards with Seat/presence/ready.
Normal Match: existing 3D table, action panel only when Decision pending;
              desktop side, mobile bottom Dock. HUD semitransparent themed frame.
Replay: desktop vertical Kyoku chapter list; mobile Kyoku dropdown;
        event controls within selected Kyoku.
Admin Rooms: desktop Master–Detail, mobile List → Room Detail/Back.
Admin setup: inline Ready Panel immediately before roster + Start.
Global UI: short success Toast survives route; blocking errors inline.
```

**Authority and constraints:** The newly confirmed Q275–Q292 choices do NOT revive A/B Experiment, Breaking Benchmark Bot protocol v2, multi-table viewer, new scheduler or automatic unverified Bot retries. No code/CI/merge authorization. All B/A selections are the owner answers, not assistant recommendations.

## Design tree — Round UI-5 genuinely independent frontier (8 questions)

The first four rounds have **56 documented UI decisions** covering all major page structures. Only these eight material interaction choices are still open. UI-5 is intended as a **short final behavioral frontier before a consistency/scope review**, not an endless series of theme-constant questions.

```text
UI-D39 recent-50 default + UI-D56 cumulative hero
└── Q293  Does a recent-range cumulative chart show absolute totals or rebased gains?
UI-D41 selected-Bot per-Match bar + UI-D55 independent legends
└── Q294  Which Bot is initially selected on the score-change chart?
UI-D35/UI-D52 Match Drawer + multiple charts
└── Q295  What does clicking a Match-specific chart point do?
UI-D47 big Lobby character cards
└── Q296  How should the cards be ordered while players arrive/disconnect?
UI-D48 contextual desktop side action panel
└── Q297  How does it appear without jarring table movement?
UI-D49 Kyoku-first Replay chapters
└── Q298  How does selecting a Kyoku affect playback state?
UI-D46 same-URL two-step Join
└── Q299  How should the browser's Back command behave on Step 2?
UI-D44 on-demand mobile fullscreen viewer + terminal Run states
└── Q300  What is shown for a Run without an active live Match?
```

All choices below preserve the already-selected chart families/desktop table hierarchy/mobile chart-first default, existing Bot protocols, Replay evidence and access controls. Each option is viable with an explicit usability trade-off. **Not decided until owner answers.**

### Q293 — Recent 50 Matches: cumulative chart vertical reference

The score graph is **cumulative table-relative net score**, and Q275 B selects recent-50 by default, with an All-Matches switch.
- **A:** plot **absolute cumulative total since Run start** for those 50 recent points; user sees current total and long-run standing, but the first point doesn't start at zero.
- **B:** **rebase the displayed 50-Match window** to zero, showing change within the window; better short-term comparison, but its vertical values aren't Run-to-date totals (must label `50-match gain/loss` explicitly).
- **C (recommended):** keep absolute Run-to-date y-values, and show the **change over selected window as a separate label/tooltip**; adds useful local context without reinterpreting the cumulative plot.
- **Why C:** preserves the established definition of cumulative net score while surfacing the recent-form signal that motivated the 50-Match default.

### Q294 — Per-Match delta bar initial Participant

Q277 B shows one Bot's net score changes and Q291 B keeps charts independently selectable.
- **A (recommended):** initial Bot = current **Leaderboard leader by lowest mean rank**, stable ID tie; quick analysis of the best performer, but default changes when standings change or the page first opens later.
- **B:** initial Bot = **first fixed roster Participant** (stable across visits); consistent, but may not be the most interesting.
- **C:** **remember the Admin's last selection for this Run** locally and use the Leaderboard leader only as a fallback; personalized, but adds small client preference state.
- **Why A:** avoids new preference storage while making the initial chart useful.

### Q295 — Click/tap a Match point in analytics

Four standard charts exist, and historical Matches already open a Drawer.
- **A (recommended):** data points with an actual Match sequence **open the existing Match Detail Drawer**; provide an explicit keyboard-accessible `View Match` action in Tooltip/accessible table. Distribution aggregates remain non-clickable or link to filtered history.
- **B:** **Tooltip only**; Match Drawer opens exclusively from the history table. Consistent roles, but requires scrolling to drill into a chart outlier.
- **C:** point **scrolls/jumps to the matching Match row** in paginated history (loading the required page), from which Drawer opens. Clear browsing context but adds one navigation step.
- **Why A:** reuses existing Drawer and makes score outliers actionable without another screen.

### Q296 — Order of large Lobby Participant cards

Seat labels must always be correct; arrival and disconnection can change.
- **A (recommended):** **stable Seat order** (East/South/West/North as applicable) with present/ready state changes inside cards; spatial stability.
- **B:** **join order** (first to most recently connected) with Seat labeled on every card; social arrival flow, but seat order isn't visualized.
- **C:** **Ready cards first**, Waiting below, always displaying Seat and Participant ID/name; progress easy to see, cards may reorder.
- **Why A:** changing readiness won't make card locations jump.

### Q297 — Contextual desktop gameplay action panel appearance

Q267 B chose a desktop side panel, Q284 B chose **only while action is pending**.
- **A (recommended):** an **anchored side panel slides into a reserved margin/side region** during legal Decision and retracts when idle; table doesn't shrink unexpectedly, but reserved space slightly constrains the maximum table size.
- **B:** a **thin persistent side affordance/status rail** expands into the action panel when needed, preserving spatial recognition at the cost of a small always-visible rail (not an always-visible full action panel).
- **C:** an **overlaid side panel** floats above a designated right-edge area of the table when actionable; maximizes the idle table, but care is needed to avoid hiding tiles/score.
- **Why A:** preserves the table geometry across critical decision transitions.

### Q298 — When Admin jumps to a Kyoku in Replay

Q268 B/Q285 B approved Kyoku-first desktop chapter list/mobile dropdown with event controls inside the Kyoku.
- **A:** always **pause at the start of the selected Kyoku**; clear control, but the user must press Play to continue.
- **B (recommended):** **preserve playback state**: if Replay was playing, continue from chosen Kyoku; if paused, stay paused. Less interruption; needs clear current-hand indication.
- **C:** selecting any Kyoku **autoplays** from that Kyoku's start, even if Replay was paused. Fast casual browsing, less predictable for close inspection.
- **Why B:** follows normal media-player expectations without inventing new Replay infrastructure.

### Q299 — Browser Back during same-URL Join step 2

Q282 A selected two Join steps at a **single URL**, with an explicit in-app Back preserving entered name.
- **A (recommended):** in-app `Back` returns to Step 1; **browser Back leaves the Join page** as an ordinary one-URL page. Simplest, but can surprise phone users.
- **B:** use **History API state entries on that same URL** so browser Back returns from Step 2 to Step 1; more natural phone behavior, but needs careful browser refresh/deep-link handling.
- **C:** keep in-app Back for Step 1, but **intercept an attempted browser Back at Step 2 with a short leave/keep-editing confirmation**; protects draft fields, but is more interruptive.
- **Why A:** honors the chosen single-route flow with minimal navigation machinery.

### Q300 — Mobile Open Live Table when Run has no active Match

Q244 C/Q256 B permit only explicit on-demand mobile live observation. A Completed/Stopped/Interrupted Run has no active live table.
- **A:** **hide `Open Live Table`** when no live Match exists; show `Watch latest Replay` for persisted results. Cleanest screen, but control disappears.
- **B (recommended):** keep **disabled `Open Live Table` with reason** (e.g. `No live Match · Run Stopped`) and a nearby `Watch latest Replay` link. Consistent discoverability, some visual clutter.
- **C:** allow opening a read-only viewer containing **the most recent persisted Replay frame**, very clearly marked `Recorded · not live`. Smooth handoff, but blends live and Replay surfaces and needs extra UI state.
- **Why B:** avoids implying a historical frame is live, while making the absence of live play explicit.

### After UI-5

Consolidate a concise owner-facing final UI/UX consistency report, including genuine unsettled design blockers vs implementer-level accessibility/navigation/CSS details. Continue to honor the explicit no-rollback publication choice, and **do not** start code, CI or PR merge. Do not create more owner rounds for details that do not materially change the experience.

## Historical decision tree — Round UI-2 (answered; pre-revised-UI-3 draft only)

```text
Round UI-1 fixed prerequisites
├── UI-D1 modern restrained visual language
│   └── Q247 Theme surface palette
├── UI-D2 desktop live table 2/3 + leaderboard 1/3
│   └── Q248 Desktop panel sizing / scrolling
├── UI-D3 Run index as searchable table
│   └── Q249 Run list filters and sorting
├── UI-D4 short New Run → Admin Room roster → Start
│   └── Q250 Setup readiness and validation guidance
├── UI-D5 compact leaderboard + one trend + history
│   ├── Q251 Leaderboard ranking sort
│   ├── Q252 Trend chart interaction
│   └── Q253 Historical Match details visibility
├── UI-D6 contextual Stop / safe Interrupted Resume
│   └── Q254 Confirmation / in-progress feedback
├── UI-D7 explicit Participant focus across seats
│   └── Q255 Focus control on desktop/fullscreen
├── UI-D8 MOBILE CHART-FIRST; live table hidden
│   └── Q256 Optional on-demand mobile live viewer?   [crucial exception]
├── UI-D9 reliable per-state labels
│   └── Q257 Stale view and recovery affordance
└── UI-D10 shared role-aware navigation
    └── Q258 Desktop/mobile navigation pattern
```

**Historical UI-2 option set below:** all Q247–Q258 are DECIDED above; the previously proposed Q252 B stacked-area chart does not reflect the owner's explicit chosen B explanation. The CURRENT frontier is Q259–Q274 below. **There is no product-code implementation approval.**

## Archived Round UI-2 options (owner's Q252 B wording overrides old draft option)

**Q247. Theme surface palette (visual direction):** The user chose restrained modern, but should the shell be visually neutral or strong-game themed?
- **A (recommended):** neutral/slate surfaces with restrained green/teal accents; real Mahjong tile/table graphics provide game identity; System/Light/Dark tokens.
- B: bright green game-table panels throughout all pages, including forms.
- C: stark grayscale everywhere, no colored semantic status indicators.

**Q248. Desktop 2/3 table vs 1/3 leaderboard resizing:** How should it behave when the desktop browser gets narrower?
- **A (recommended):** stable proportional grid at comfortable widths; stack the leaderboard below when too narrow. No new drag-resize framework.
- B: user-draggable split divider persisted per browser.
- C: enforce fixed pixel widths, causing horizontal scroll on a small laptop.

**Q249. Run index search/filters:** Keep it simple, but how should many Runs be found?
- **A (recommended):** search by visible Run/Room and Bot names, simple Status/Mode filters, newest-first default and bounded pagination.
- B: complex saved cross-user queries and custom column configuration.
- C: no search or sorting despite Q239 choosing searchable history.

**Q250. Roster setup readiness:** When the user has chosen 3 Bots in a 4p room, or an incompatible 3p MJAI Bot?
- **A (recommended):** inline per-rule explanation with current selected/required count and disabled Start until valid; explicit route to connect/select eligible Bots, no phantom fill.
- B: permit Start then show a generic server failure.
- C: automatically create missing external Bot Participants without authorization.

**Q251. Leaderboard default sort:** Scores/average rank across changing Seats.
- **A (recommended):** ascending average rank (1.0 best), tie-break by stable Participant ID; show exact first-place percentage and relative net scores, label no-data.
- B: always alphabetical even when comparing performance.
- C: sort by current Seat, changing ranks every Match.

**Q252. Cumulative performance plot UX:** What should the one chart provide?
- **A (recommended):** simple line chart with Participant legend, tooltip at Match index and a visible zero baseline; no research-grade additional analytics.
- B: stacked area chart that wrongly implies zero-sum results between all Participants without explanation.
- C: raw numbers only despite approved trend chart.

**Q253. Match history interaction:** Completed Run could have 1,000 Matches.
- **A (recommended):** show recent Matches by default, expandable/paginated full result table, Replay link per persisted Match; no giant DOM with 1,000×4 rows.
- B: display every row expanded at once.
- C: hide old persisted Match links once the Run completes.

**Q254. Stop confirmation and Resume feedback:** How detailed?
- **A (recommended):** confirm explicitly "Finish current Match, then stop"; show `Stopping after current Match` immediately after accepted server response, disable duplicate action; for Interrupted show guarded `Resume` with missing-Bot reasons.
- B: Stop immediately with no explanation, even while a Match is playing.
- C: allow multiple rapid Resume calls and optimistic fake Running state.

**Q255. Participant focus control placement:** How should Admin move the observation camera/hand focus?
- **A (recommended):** labeled Participant chips/dropdown above or adjacent to real table, repeated in fullscreen; show Bot name + Seat, preserve focus by immutable Participant ID, restore keyboard focus on exit.
- B: only an undocumented keyboard shortcut.
- C: clicking arbitrary seat pixels changes both focus and Participant identity.

**Q256. Mobile chart-first exception (Q244 C):** Mobile Run detail does **not** display the table by default. Is a manually requested mobile live viewer allowed?
- **A (recommended — strict reading of C):** no mobile Run-detail live view entry at all; mobile shows graph/status/stats/history only. Desktop hosts the Benchmark live 3D view.
- B: graph-only by default, but an explicit `Open Live Table` action may open the existing *one* table on demand in fullscreen/in-page fallback; no autoplay or hidden background renderer.
- C: automatically render the live 3D table underneath the chart on mobile, overriding Q244 C.

**Q257. Stale server snapshot UX:** When an Admin's live update connection stops but cached match data remains?
- **A (recommended):** keep the last authorized view visible with an unmistakable `Stale · last updated ...` indicator, retry/reconnection progress, and avoid misleading instantaneous badges.
- B: blank all useful accumulated Run data instantly, even though recorded history remains available.
- C: continue showing green Live despite disconnected state.

**Q258. Shared navigation pattern:** Owner selected one shared role-aware shell. Which layout fits desktop admin and mobile public pages?
- **A (recommended):** compact desktop Admin sidebar or rail with clear sections, public pages use a minimal topbar; mobile collapses navigation into a labeled accessible menu with persistent page context.
- B: permanent desktop-width sidebar on tiny phones.
- C: no consistent route navigation; browser Back is the only way to navigate.

## Historical deferments recorded before UI-2 answers (superseded by current UI-4 frontier)

- Exact state/interaction for mobile chart/no-table choice (Q256); no invented mobile live view until decided.
- Typography/spacing tokens and light/dark treatment guided by selected palette; concrete state mockups not fake backend screenshots.
- Design layout and real interactions for other app surfaces: Home/Room join/Lobby/Live/Replay/Admin, using the shared shell and unchanged game renderers.
- Testing and acceptance: real rendered Seat/score mapping, keyboard navigation, visual regression, stale authenticated data; no old-UI rollback.
- Limit questioning to substantive remaining UX trade-offs; delegate cosmetic constants and framework wiring. Owner asked not to overengineer.
