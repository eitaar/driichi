# driichi UI/UX — Grill with Docs (completed UI-1–UI-5; outcomes UI-6 open)

Date: 2026-10-14
Status: **UI-D67 explicitly brings in-game Win/Draw, Score Transfer and Final Standings into the new design. UI-6 Q301–Q309 are pending. Previously decided UI-D1–UI-D66 stay binding; no product code/CI/deployment/merge approval.**
Scope: shadcn/ui (Base UI) + Tailwind-based English-only redesign for **all** public/admin pages, starting with Run-first Benchmark Dashboard. One public all-pages cutover; no planned old-UI rollback. Dedicated existing Three.js/Pixi Mahjong renderer remains.

**Read first:** newer owner **UI-D67** reopens *only* visual design of end-of-Kyoku outcome and post-Match ranking screens; UI-6 answers remain pending. Previous no-pending-questions audit predates the new explicit owner request. Earlier owner **UI-D66** selects subtly slanted portrait character cards with prominent avatar/name and very few SVG status/own-player icons instead of repetitive Ready/You text badges. UI-D66 supersedes the *vertical-only visual layout* of older UI-D30/UI-D47 but preserves stable participant collection semantics. Current code demonstrates **normal-room Seat assignment occurs at Start Match, NOT in the pre-Match Lobby**. The correction below supersedes the unimplementable Q296 A East/South/West/North prestart order **while preserving its intent: stable participant cards that do not jump on readiness**. Latest owner **UI-D65 overrides only the normal-game action-button PLACEMENT clauses** of UI-D31 (Q267 B), UI-D48 (Q284 B), and UI-D61 (Q297 A). The original 64 question choices are retained as historical design decisions and all their *unrelated* requirements remain active. The latest authority map and consistency audit below supersede any older archived options, particularly the rejected original UI-3 A/B/C definitions. Old "pending/frontier" text in archived proposal sections is not a new open decision.

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

**On small-screen Run detail, the live 3D Mahjong table must NOT be displayed by default.** The content focuses on performance charts/Run statistics rather than an automatic/shrunken table. Essential Run progress, valid Admin operations, status and accessibility remain available; hiding a table may **not** hide required Stop/Resume or show a fake Live indicator. This is the owner's intentional deviation from the previous proposed mobile-table-first fallback. UI-2 **Q256 B** creates an explicit opt-in exception: mobile detail remains chart-first and inline-table-free, but a user can choose **Open Live Table** to view that same single table fullscreen or via accessible in-page fallback. No autoplay, background renderer or unsolicited private-hand fetch. Return and transition behavior is decided in **revised Q263 A**; live/no-live affordance is decided in **Q300 B**. Desktop/large-screen live 3D remains mandatory.

### UI-D9. Honest live/reconnection/empty/terminal states — DECIDED (Q245 A)

Distinguish `Live`, `Reconnecting`, `Stale`, `Waiting for next Match`, `Stopped`, `Interrupted`, `Failed` and `Empty`. Show known historical snapshot labeled stale, not as current. Clear retry/next steps; stop relying on a generic indefinite spinner.

### UI-D10. One role-aware application navigation shell — DECIDED (Q246 A)

Across Home, Room, Lobby, Live, Admin, Benchmark and Replay, use one shared shadcn/**Base UI** + Tailwind component/tokens vocabulary with role-appropriate links, safe deep links and clear route context. This does not grant Admin information to non-Admin users; existing public/private game views remain separate.

### Recorded supersession / unresolved UX consequences

- **Q244 C supersedes the earlier generic "mobile fullscreen live table fallback" proposal for the default mobile Benchmark Run-detail surface.** Existing game/Room and Replay views are not banned on mobile by this choice. Mobile manual entry is expressly approved by **Q256 B**; **revised Q263 A** resolves closing/scroll/focus restoration.
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

Explicit user choice: **"複数の高度な分析グラフを標準表示"**. This **overrides Q241 A's earlier compact/single-graph limit**, but preserves the Q238 A layout: real 3D table + leaderboard remain desktop-first, advanced plots occupy supporting dashboard regions **below** rather than replacing the table. Existing rank/score/per-Match history can support richer ordinary Run visualizations; no A/B comparisons, Bootstrap CI, research study dashboards, invented inference latency or hidden mandatory per-model instrumentation. **revised Q259 C** selects four real-Run chart families, **revised Q260 C + Q274 C** define their hero/subplot hierarchy, and **revised Q261 C** permits honest provisional observations; final chart interactions are Q275–Q295. **Do not assume the rejected old Q252 B stacked-area example**.

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

Render honestly computable chart values even for a few recorded Matches, but **label small-history analyses as provisional** and indicate observed sample size. Do not imply statistical significance, invent missing Match rows or synthesize unknown data. After sufficient data, display normally; the shared provisional Analytics banner is selected by **Q278 B**. No Bootstrap CI.

### UI-D26. Mobile categories default-expanded; collapsible when desired — DECIDED (revised Q262 B)

On narrow screens, the default Run detail remains **chart-first** with multiple visible graphs grouped into explicitly labeled default-expanded sections such as **Performance** and **Consistency**, with optional collapse. Run progress and valid controls remain easy to reach; history follows. No inline or background-rendered 3D table. The manually opened live viewer remains separately allowed under Q256 B.

### UI-D27. On-demand mobile fullscreen overlay with restored context — DECIDED (revised Q263 A)

An explicit `Open Live Table` opens the **same single authorized real 3D table** above the Run-detail screen, fullscreen (or accessible in-page equivalent if native fullscreen fails). It can persist across automatically starting Matches. Closing returns to the **same Run-detail scroll position and keyboard focus**, and unmounts/stops rendering confidential hand projections from the viewer. No navigation to a new tab per Match.

### UI-D28. Home two-column function + character art — DECIDED (revised Q264 B)

Home uses a usable **Join Room form** in one column and existing character/game visuals in the other, balancing practical entry and personality. Do not gate Join behind an intro animation. On phones content flows responsively; the mobile order is **Join first** under **Q281 A**.

### UI-D29. Join Room as two clear steps — DECIDED (revised Q265 B)

After Room lookup, **Step 1** presents Room information and name entry; **Step 2** chooses a Character; then Join/confirmation as currently permitted. Keep inline errors and accessible previous/next behavior, without introducing account requirements. **Q282 A** retains both steps on a single URL; **Q299 A** defines browser Back separately from the in-app Back.

### UI-D30. Lobby is Participant list first — DECIDED (revised Q266 B)

Present **Participant name, character, selection status, presence/connection and readiness** as a semantically ordered collection of cards, not a centerpiece seat map. **Newer UI-D66** controls card visuals: mildly slanted portrait cards side by side on roomy screens, compact responsive arrangement on phones, passive state and self markers expressed through minimal SVG icons rather than repeated text. The state continues to come from the server. **Pre-Match Lobby has NO assigned East/South/West/North for Participants**; show `Seat assigned when Match starts` or omit the Seat field. Show current Room code and next allowed action; never synthesize Ready from mere connection or automatically fill absent Players. **Q283 B** chooses large Character/Participant cards; **Q296 A** retains its stable-order intent but its fixed compass-seat assumption is technically superseded by the verified seat-assignment behavior recorded below.

### UI-D31. Responsive normal Match action panel — PLACEMENT SUPERSEDED BY UI-D65 (historical revised Q267 B)

**Historical placement, now withdrawn:** desktop side Action Panel and mobile bottom Dock are NOT active requirements. **Latest owner UI-D65** returns to the shipped/Mahjong Soul-style pattern: **only when a Decision needs a button, display the relevant legal action buttons directly above the player's concealed hand on BOTH desktop and mobile.** Continue using the real 3D table, server-authoritative allowed actions, Decision timer, winds/scores, accessible keyboard/touch controls and player vs Admin hand privacy. Ordinary clickable tiles for valid discard choices remain. No extra always-visible side control region.

### UI-D32. Replay is navigated by Kyoku — DECIDED (revised Q268 B)

Use **Kyoku (局)** as the principal navigation unit (e.g., East 1 → East 2 → …), with event-step/playback controls **within** a selected Kyoku. Preserve real existing Replay frames and renderer, support longer Match browsing. **Q285 B** specifies desktop vertical Kyoku chapters and phone dropdown; **Q298 B** preserves the current playback state on jumping. Do not create a second Replay engine.

### UI-D33. Admin Rooms uses Master–Detail layout — DECIDED (revised Q269 A)

The common Admin shell shows a **Room index/rail at left and selected Room's controls and status at right**, allowing quick Room switching while displaying Participants, Bot Tokens where authorized, readiness and configuration. On mobile a safe single-pane transition is expected; **Q286 A** chooses a mobile Room list followed by a full-width Room detail. Protect role/private-hand boundaries.

### UI-D34. Shell + peripheral HUD components respond to theme — DECIDED (revised Q270 B)

Light/Dark applies to the app shell **and surrounding game HUD frame/action controls**, not to tiles, tabletop or the real Three.js/Pixi graphics themselves. Use accessible contrast, text labels and restrained semantic accents; no global recoloring of Mahjong assets. Theme integration and readable surrounding overlays are Q288.

### UI-D35. Per-Match results open a Detail Drawer — DECIDED (revised Q271 B)

Selecting a persisted Match in Run history reveals its **Participant placements, scores and Replay link** in a non-destructive side/bottom **Detail Drawer**, preserving the Run list and context. No Experiment comparison page. **Q288 A** specifies right-side desktop Drawer / mobile bottom Sheet with focus restoration.

### UI-D36. Real-time Ready Panel for Room setup — DECIDED (revised Q272 B)

After Room creation, show the live selected/required eligible Bot count, actual connection state and concrete blockers, and enable `Start Run` only when current/server-validated conditions are satisfied. Reuse the existing Room/Run flow, no wizard/additional Bots. **Q289 B** places it inline immediately before roster and Start.

### UI-D37. Toast for transient actions, inline for blocking states — DECIDED (revised Q273 B)

Use nonblocking **Toast** for short success confirmation; persistent errors, empty state, unauthorized/stale observations and blocking setup conditions appear **inline at the affected surface**, with Retry or next action. Never use a Toast as the only record of an important failure. Use the same shared shadcn/Base UI components throughout; **Q290 B** keeps transient success Toasts in the shared shell across route transitions; persistent blocking errors stay inline.

### UI-D38. Primary + secondary analytics under table — DECIDED (revised Q274 C)

Desktop priority: **large real 3D table and compact Leaderboard first**. Analytics begins below with one main **full-width** chart, then subordinate charts as a two-column layout; stack responsively. This reinforces UI-D24 and **Q252 B's multiple default-visible advanced graphs**. **Q292 A** selects cumulative table-relative net score as the full-width hero chart.

### Invariant summary after revised UI-3

- Main desktop: one real table + Leaderboard, below it **4 default-visible** genuine-result charts in a primary+secondary hierarchy and Drawer-based results/Replay history.
- Mobile: default **chart-focused**, grouped expanded Performance/Consistency sections; **no inline live table**. Explicit tap `Open Live Table` opens one real fullscreen overlay; close restores context. Keep User/Q244 C and Q256 B both operative.
- Public Home: two-column functional Join + existing game/character visuals; join is a **two-step** name then Character flow; Lobby uses a **Participant list**; normal 3D gameplay keeps **the 3D tabletop, clickable hand and action buttons appearing only when required just above the concealed hand on desktop AND mobile** (latest UI-D65 supersedes former side rail/Dock).
- Replay: **Kyoku-first**; Admin Rooms **Master–Detail**. App-shell & peripheral HUD respond to System/Light/Dark, assets are not recolored. **Toast + inline errors** by persistence.
- Breaking Benchmark Bot Protocol v2 and all A/B Experiment features remain **canceled**. Real unchanged MJAI/MCP/Built-in contracts, SQLite and existing Run semantics prevail.
- No implementation or UI source changes have been approved. **Q275–Q300 are all decided**; no new owner questionnaire is pending. Final owner design signoff is distinct from implementation permission.

## Owner decisions — Round UI-4 (2026-10-14)

**Authoritative owner answers:** `Q275 B; Q276 A; Q277 B; Q278 B; Q279 B; Q280 A; Q281 A; Q282 A; Q283 B; Q284 B; Q285 B; Q286 A; Q287 B; Q288 A; Q289 B; Q290 B; Q291 B; Q292 A`. **18/18 answered**, with 11 B and 7 A. The original A/B/C labels for UI-4 refer to the *realistic option definitions archived below*, not the rejected original UI-3.

### UI-D39. Recent 50 Matches default, optional whole Run — DECIDED (Q275 B)

Time-series analyses default to **most recent 50 durably completed Matches** (or all available if fewer), with an obvious **All Matches** switch. Switch does not change Run completion counts, percentages or stored results. **Q293 C** retains absolute Run-to-date totals on the recent 50 plotted points, with separate labeled local-window gain/loss.

### UI-D40. Rank distribution is Bot-wise 100%-stacked horizontal bars — DECIDED (Q276 A)

Each Participant gets a horizontal full-proportion bar segmented into rank 1/2/3/(4), using only completed persisted Match outcomes and honest per-Bot denominator. In a 3p Run, do **not** invent a fourth place or fourth Bot; accessible names/percentages must be available without relying on color alone.

### UI-D41. Per-Match score changes are selected-Bot bars — DECIDED (Q277 B)

Render signed **per-Match table-relative net-score bars** for one explicitly selected Participant, preserving Match order and current active range. Selection can switch to any participant in the roster. Do not portray the final raw score as a relative delta or confuse a seat reassignment with a different Participant; **Q294 A** initially selects the current average-rank Leaderboard leader when recorded standings exist.

### UI-D42. One shared provisional-data banner over Analytics — DECIDED (Q278 B)

When completed observations are too few to support stable interpretation, show a clear **Analytics-level provisional-data notice** containing the number of durable completed Matches, while still rendering mathematically valid observations. All charts retain accessible axes/denominator or visible sample descriptions; a screen-reader or scrolled graph must not quietly masquerade as statistically established. Threshold, copy and statistical caveats are implementation-owned, not a new owner questionnaire; do **not** assert arbitrary confidence bounds.

### UI-D43. Mobile default-open category sections with jump menu — DECIDED (Q279 B)

Group four charts into **Performance** and **Consistency** sections with headings and an accessible compact jump menu on mobile. **Both categories are expanded by default**; owner may collapse them independently. No default 3D table inline, even when categories fold; Q244 C remains operative.

### UI-D44. Portrait and landscape both supported for opt-in live overlay — DECIDED (Q280 A)

Explicit `Open Live Table` renders the same authorized single 3D table in an accessible fullscreen overlay on phone; controls reflow in portrait and landscape. No forced device rotation or landscape-only viewer. Manual exit returns to charts and restores scroll/focus as UI-D27 requires.

### UI-D45. Mobile Home shows Join first, character artwork below — DECIDED (Q281 A)

At smaller widths, put the functional 6-digit Room Join input/action **before** the existing decorative character/game visual section. Desktop retains the approved two-column Join+art. Do not hide Join behind branding/animation.

### UI-D46. Two-step Room Join stays at the same URL — DECIDED (Q282 A)

Under the existing Room Join path, use local **Step 1: Room confirmation/name** then **Step 2: Character**; an explicit `Back` returns to Step 1 without erasing accepted input. Do not add invented signup, extra routes or new Bot/game protocol. **Q299 A** deliberately makes browser Back leave the one-URL Join page; the explicit in-app Back returns to Step 1 and preserves fields.

### UI-D47. Lobby uses large character Participant cards — VISUAL LAYOUT REFINED BY UI-D66 (Q283 B)

Show Participant name, selection, actual presence/readiness and larger existing Character art in easy-to-scan **portrait cards**; no centerpiece Seat map. **Later owner UI-D66** replaces the old *vertical-only* visual layout with a lightly angled, side-by-side group on desktop and a compact, readable grid/stack on mobile. Replace repetitive Ready/Not Ready/You/Connected labels in the card itself with minimal, accessible SVG status/self glyphs. **Pre-Match Seat is not allocated**, so no East/South/West/North labels are shown or implied. Preserve stable per-Participant cards (e.g., current server snapshot's deterministic Participant ID order) and update presence/readiness in place. The historical Q296 A stable-compass ordering is replaced **only for this impossible pre-Match Seat assumption** by the verified implementation rule below.

### UI-D48. Gameplay action panel is contextual only — PLACEMENT SUPERSEDED BY UI-D65 (historical Q284 B)

**Still binding:** controls appear only for relevant, server-authorized actionable Decisions, remain reachable by keyboard/touch, distinguish expired/blocked choices, and must never leak Admin concealed information. **Superseded placement:** no desktop right side panel, no mobile separate bottom Dock and no reserved right-hand slide-in margin. **UI-D65** uses one contextual action-button strip immediately **above the local hand** at all supported viewport sizes, modeled on current gameplay placement and Mahjong Soul's general interaction pattern, not on copying its proprietary visual assets. Idle shows no row of action buttons; subtle timing/connection cues may remain.

### UI-D49. Replay navigation is vertical Kyoku chapters on desktop, dropdown on mobile — DECIDED (Q285 B)

Preserve one renderer and Replay data; on desktop put an accessible vertical Kyoku chapter list next to the Replay table, on phones use a labeled dropdown. Event-level controls stay inside selected Kyoku. **Q298 B** preserves playing/paused status after a Kyoku jump; do not create a competing Replay engine.

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
Lobby: LARGE gently slanted portrait Character cards, side-by-side on desktop, compact on mobile, stable Participant order, prominent name and only minimal SVG status/self icons; upcoming East/South/West/North Seats are UNASSIGNED before Start.
Normal Match: existing 3D table and clickable hand;
              ONLY relevant action buttons above player's concealed hand (PC + phone);
              no side panel, reserved right region or separate mobile bottom Dock.
              HUD semitransparent themed frame.
Replay: desktop vertical Kyoku chapter list; mobile Kyoku dropdown;
        event controls within selected Kyoku.
Admin Rooms: desktop Master–Detail, mobile List → Room Detail/Back.
Admin setup: inline Ready Panel immediately before roster + Start.
Global UI: short success Toast survives route; blocking errors inline.
```

**Authority and constraints:** The newly confirmed Q275–Q292 choices do NOT revive A/B Experiment, Breaking Benchmark Bot protocol v2, multi-table viewer, new scheduler or automatic unverified Bot retries. No code/CI/merge authorization. All B/A selections are the owner answers, not assistant recommendations.

## Owner decisions — Round UI-5 (2026-10-14)

**Authoritative owner answers:** `Q293 C; Q294 A; Q295 A; Q296 A; Q297 A; Q298 B; Q299 A; Q300 B`. **All eight answered.** This closes the previously approved short final UI/UX interview; no more independent UI preference frontier remains.

### UI-D57. Absolute cumulative y-value plus window gain/loss — DECIDED (Q293 C)

Recent-50 remains the default visible *x-range* for time-series charts, while the HERO cumulative relative net score retains **absolute total from the Run's start at each visible Match**, not a rebased-to-zero window total. Label the net delta **within the chosen window** separately in card header or tooltip. `All Matches` displays the full stored history; do not reset cumulative calculations or mislabel the chart's y-values as recent-only gains.

### UI-D58. Score-change graph initially selects current average-rank leader — DECIDED (Q294 A)

Upon initial Run-detail view, the per-Match relative-net bar chart selects the roster Participant currently **leading by ascending mean rank** (stable Participant ID ties). Later page visits may initialize to a different leader if standings changed. **Do not steal a manually selected Bot on a background scoreboard refresh**; per-chart selection stays independent of all other chart interactions. With no completed Matches, show a genuine no-results state, not a fabricated leader or score.

### UI-D59. Single-Match chart point opens existing Detail Drawer — DECIDED (Q295 A)

Click/tap/keyboard activation on a chart datapoint representing a specific persisted Match opens the already agreed **Match Detail Drawer** with that Match's placement, scores and Replay link. Reuse Q271 B/Q288 A (desktop right Drawer, phone bottom Sheet). Aggregated distribution bars have no single Match identity and must not dispatch an arbitrary Match Drawer; use no action or a clearly labeled filter instead. Preserve the Run chart/history scroll and focus on close.

### UI-D60. Stable Lobby participant cards — PRESTART SEAT-ORDER ASSUMPTION CORRECTED (Q296 A, verified source behavior)

**Technical correction to previously accepted Q296 A:** The ordinary Room does **not** assign East/South/West/North Seats in `RoomPhase::Lobby`. `RoomActor::build_match()` constructs a new `MatchMachine::with_time_control()` at Start Match, which calls `new()`/`new_internal(..., shuffle_seats=true)` and randomly shuffles selected Participants. Only `commit_match()` assigns `MatchRole::Player(seat)` and publishes a concrete `match_roster`/Seat mapping. `back_to_lobby()` clears `match_roster` and resets participant roles, selections and readiness. **Therefore do not sort or label ordinary pre-Match Lobby cards by fictitious compass Seats.** Preserve Q296 A's UX *intent* by keeping the large Character/Participant cards in a **deterministic stable Participant order** (the current server Snapshot sorts Participants by Participant ID), updating status in place. Display **unassigned** or no Seat until the server has assigned one for an active Match. In the post-Match preparation surface, a prior Match Seat, if exposed, must explicitly read **Previous Match Seat**, never a guaranteed next-Match Seat: Rematch creates a new shuffled Match. The 3p Lobby shows no fake fourth player/Seat. The owner explicitly identified this source-of-truth contradiction after Q300; this correction is a factual constraint, not a new UI preference round.

### UI-D61. Contextual action panel slides into reserved side region — SUPERSEDED BY UI-D65 (historical Q297 A)

**Historical implementation proposal withdrawn by latest owner UI-D65.** Do **not** reserve side space or slide a side Action Panel in; the owner wants the **current gameplay / Mahjong Soul-like action row** immediately **above the local concealed hand**, visible only for relevant authorized Decisions. Mobile is the same spatial relationship (hand + above-hand actions), not a separately anchored bottom Dock. Preserve timer, legal action focus/disabled/offline truth, adequate target sizes and unobscured tiles/score; keep the 3D table geometry stable when controls appear. No new Bot protocol or game decision semantics.

### UI-D62. Kyoku jump preserves playback state — DECIDED (Q298 B)

Selecting a Kyoku in desktop chapter list/mobile dropdown jumps playback to its start while **preserving the previous playing/paused state**. Playing Replay continues from the new Kyoku; paused Replay stays paused until Play. Retain within-Kyoku event navigation and existing Replay engine/renderer.

### UI-D63. Same-route Join uses explicit Back; browser Back exits — DECIDED (Q299 A)

Two-step Room Join stays **one URL**. Its visible in-app `Back` from Character selection to Room/name step preserves entered name and valid choices. The browser/system Back acts as normal navigation **out of the Join page**, not an internal step Back; no synthetic same-URL history entries or Back interception. Preserve accessible labels, warning about validation errors and return navigation as applicable without surprising state persistence claims.

### UI-D64. Disabled mobile live action with reason and Replay when inactive — DECIDED (Q300 B)

A mobile Run-detail screen continues to display the recognizable `Open Live Table` action **disabled** whenever no active live Match exists. Label the reason (e.g., `Waiting for next Match`, `Run Stopped`, `Run Completed`, `Run Interrupted`), distinguish transient between-Match periods from terminal Runs, and link `Watch latest Replay` **only if persisted Replay evidence exists**. Do not synthesize a live frame from stale/recorded results. When live and Admin authorized, explicit action opens the same one real fullscreen responsive overlay; closing returns to charts and removes private-hand viewer.

## Latest owner UI/UX override — AFTER Q300 (2026-10-14)

### UI-D65. Normal gameplay: context-sensitive action buttons ABOVE the concealed hand — DECIDED (latest explicit owner instruction)

**Owner wording:** `今の状態/ジャン魂の状態の、必要時にのみ手牌の上にアクションボタンを表示する形式に戻して` (minor input typo normalized in this record).

This is a **later and more specific owner directive**, overriding earlier layout decisions UI-D31 (Q267 B), UI-D48 (Q284 B), UI-D61 (Q297 A) **only to the extent they specified a separate right-side action panel, reserved side space, desktop slide-in animation or independent mobile bottom Dock**. Their requirements that actions be server-legal, only appear when needed, and retain responsive, keyboard/touch-accessible inputs remain active.

**Binding behavior on desktop and mobile normal Room gameplay:**
- Render available, authoritative **action buttons** such as `Ron`, `Tsumo`, `Chi`, `Pon`, `Kan`, `Riichi`, `Pass` (only when offered by the existing decision action list) in a **contextual row immediately above the local concealed hand**, close to where the user chooses tiles, never in an independent right-side or bottom-edge Dock.
- **Hide that contextual action row when there is no relevant action to choose**; do not display disabled placeholders for nonexistent operations. A server-authorized discard-only decision remains a **clickable/tappable local concealed hand**; it does not require rendering a redundant `Discard` button just to meet the layout rule.
- **Retain current candidate selection UX** for variants (Chi / Riichi discard / Kan), legal tile hit targets, deadline/decision timing, pending action feedback, rejection states and pointer/keyboard/touch accessibility. Adapt spacing/wrapping/scroll within the *above-hand* action area on smaller screens so buttons are usable and cannot cover the tiles, without assuming the currently shipped >=1024×600 viewport constraint remains forever.
- Render the existing Three.js Mahjong table, hands, discards, score, names, character portraits, result overlays and actual game logic as before. Surrounding frame can still use approved restrained semi-transparent Light/Dark HUD; do not clone Mahjong Soul's owned artwork, button graphics, sound, fonts or visual identity. **The influence is limited to the familiar contextual control placement and when it appears.**
- Do not introduce a persistent action panel, reserved right-hand vertical slot, mobile fixed bottom Dock, extra engine/renderer, or additional Bot protocol messages.
- This change applies to **interactive ordinary game play**. It does not make the Benchmark Admin read-only table interactive; desktop Benchmark remains large table + leaderboard, and mobile Benchmark remains charts-first with manual fullscreen viewing only.
- **No code has been changed yet.** Current `frontend/src/game/gameplay.tsx` already conditionally renders `ActionDeck` based on `decision` and `frontend/src/game/live-table.css` positions `.action-deck` above the hand; implementation should reuse and adjust these existing pieces rather than rebuild the action system. The old landscape >=1024×600 gate remains present in current implementation and is not silently claimed fixed.

**Consistency ruling:** This latest explicit directive is binding despite the Q237–Q300 interview being closed. The 64 historical answer records are preserved, but **UI-D65 is the authoritative normal gameplay control placement**. No new grilling round or final design signoff is implied. PR #23 remains Draft/docs-only.

## Latest owner Lobby visual refinement — after UI-D65 (2026-10-14)

### UI-D66. Slanted avatar cards with minimum SVG state markers — DECIDED

**Owner's image/reference decision:** Each normal pre-Match Lobby Participant appears in a **slightly slanted portrait box** containing a **large existing Character avatar and a clear display name**. The supplied reference is **structural only**: do not copy its art style, medals, titles, VS, branding, decorative motifs or exact visual assets.

**Latest follow-up:** `Readyとかyouとかの無駄なテキストを使うぐらいならsvgアイコンを使って ただし最低限に`.

**Binding visual/data behavior**
- **Desktop:** up to the actual 3 or 4 Participants are shown as an elegant horizontal group of **gently skewed portrait cards** rather than the old exclusively vertical list. **Mobile:** responsive compact two-column grid or readable stack if needed, mild/removed skew when space is too tight; no compressed/overlapping names, avatars or controls. Preserve stable Participant-ID order, not fictitious prestart East/South/West/North.
- The only default text *inside a Participant card* should be the **display name**. Large character illustration is the focus. Omit passive word badges such as `Ready`, `Not Ready`, `You`, `Connected`, `Selected` unless an exceptional explanatory error is necessary. Do not add ranks, titles, badges, additional graphic labels or VS text.
- Use **minimal inline SVG glyphs**, **maximum two per card** in a discreet corner: **one** mutually exclusive meaningful server-state indicator (`check-circle` for Ready; small hollow circle/clock for not ready; `wifi-off` instead for a disconnected Participant), and **one optional** subtle person marker only on the local Participant's card. Do not show separate glyphs for every state simultaneously. Real connected/selected/Ready data still governs legitimate `Set Ready` and Host/Admin actions.
- Each icon-only state needs a concise accessible name (`Ready`, `Not ready`, `Disconnected`, `Your participant`) in card semantics and hover/focus help where appropriate. SVG is decorative within a semantic labeled element; no color-only signaling, no extra decorative cluster. Accessible state changes should be announced appropriately without duplicated labels.
- **Skew card frame only**: avatar, display name, state icon and keyboard/touch focus targets stay straight. Preserve reasonable image cropping, responsive hit targets and reduced-motion semantics. Card art follows existing driichi character assets and the established neutral/shadcn/Base UI shell, not reference styling.
- **Preserve a readable `Set Ready` action label, errors and the Room code**, since those are actual actions/context, not redundant passive participant badges. Room rules and Player Seat lifecycle remain unchanged: no Seat before normal Match start; normal Start performs random assignment; post-Match previous Seat may be labeled only as previous.
- **Only normal human Lobby visual presentation changes**. Benchmark Setup Ready Panel, read-only Live Admin table, data/permissions, and Mahjong game actions remain governed by earlier decisions. No product code, SVG implementation, CI, deployment or merge is authorized in this docs-only PR.

**Supersession:** UI-D66 refines Q266 B/UI-D30 and Q283 B/UI-D47 **in card visual composition and passive status copy only**, retaining a stable ordered Participant collection; it does not supersede the technical correction to Q296 A/UI-D60, and does not reverse UI-D65's above-hand contextual action row. No further questioning required.

## Scope addition: in-game outcome UI redesign — UI-6 (2026-10-15)

### UI-D67. Redesign Win/Draw, Score Transfer and Final Standings — SCOPE DECIDED, PRESENTATION PENDING

**Owner request:** `ゲーム内UIも新デザインに変更したい`, referring directly to Kyoku end hand reveal / score movement and full Match final rankings. The old assumption to *keep these result overlays' current visual design* is superseded. The user has **not** chosen a specific result layout or authorized implementation.

**Grounded existing main branch `frontend/src/game/gameplay.tsx`:**
- `RoundWinSurface` exposes the authorized **winner** portrait/name, concealed winning tiles and melds, winning tile marker, legitimate Yaku/han/fu and win points.
- `SettlementPresentation`/`ScoreTransfer` renders settlement score movement from server `KyokuSettlement`; local Pause/Resume review and Skip affect visual animation, not the Match's authoritative timer.
- `KyokuEndStep` governs countdown and per-Participant `Continue` acknowledgements or spectator Dismiss. Keep deadline, action idempotence and `Waiting for players` semantics.
- `ResultsPanel` presents actual sorted ranks and final points when `room.phase === "post_match"`, with `Return to room`. CSS is largely in `frontend/src/styles.css`.
- Current live gameplay rejects non-landscape or viewport smaller than 1024×600; the proposed phone UI is **not yet implemented**. Also an earlier Seat↔score mismatch report remains unverified.

**Domain boundaries (already in `CONTEXT.md`, no glossary or ADR edits needed):** `Kyoku` = one dealt hand within a `Match`. Win/Draw/Score Transfer are events or outcomes of a Kyoku, **Final Standings** belong to the complete Match's **Post-Match** phase. `Replay` is not a live Win/Draw screen. Do not conflate these.

**Binding constraints even before Q301–Q309 answers:** Use existing Three.js Mahjong tabletop, actual winner-only authorized hand information (NOT all opponents' concealed tiles), existing SVG tile/character portrait assets, true 3p/4p Player-to-Seat mapping, Yaku and points, and server authoritative result/timer/Continue/Draw/Post-Match permissions. Cosmetic Skip/Replay animation never bypasses required server acknowledgements or invents draws, hands, scores or final ranks. Keep the current above-hand action-button placement UI-D65, angled Lobby UI-D66, Admin role/privacy, English-only modern UI, keyboard/touch support, reduced motion, and no A/B Experiment / breaking Benchmark Bot v2 protocol. Redesigning overlay components does not require another Mahjong engine, second table or new backend result schema. **No implementation/CI/merge is authorized.**

### UI-6 decision tree — entire currently unlocked independent frontier

```text
UI-D67: end-of-Kyoku + Post-Match visual redesign
├─ Win result
│  ├─ Q301 desktop overlay/table relationship
│  └─ Q302 visual hierarchy: hand vs portrait vs yaku
├─ Q303 Draw visual treatment
├─ Q304 Score Transfer visual presentation
├─ Q305 server-confirmed Continue/review controls
├─ Post-Match
│  ├─ Q306 Final Standings layout
│  └─ Q307 next navigation action(s)
├─ Q308 animation intensity
└─ Q309 mobile results surface pattern
```

All nine questions are **independent** at this stage, A/B/C are **viable**, recommendations vary and no owner answer is prefilled.

### Round UI-6 questions — A/B/C all awaiting owner

**Q301. Desktop Win Result placement**
- A: compact **floating table-edge panel**, preserving most 3D tabletop (continuity; denser information).
- B: **centered large overlay** over dimmed real table (strong result focus; more occlusion).
- **C (recommended):** **split result-stage panel** with winner/tiles on one side and scoring on the other, real table visible behind (roomy; needs responsive care).

**Q302. Win Result information priority**
- **A (recommended):** **winning tiles and highlighted winning tile** are primary, clear Yaku/han/fu/points adjacent, portrait secondary (most informative).
- B: **winner Character and total points** are primary, tiles/Yaku still clearly readable below (celebration focus).
- C: **Yaku/points calculation** first, portrait and tiles beneath (review focus).

**Q303. Draw (流局) presentation**
- A: compact **draw banner + settlement**, tabletop largely visible (fast, more distinct from Win).
- **B (recommended):** **same result shell** as Win, swapping winner elements for truthful Draw reason and settlement (consistent UI).
- C: **special Draw card** prioritizing draw type, reason and points movement (distinguishable, more variants).

**Q304. Score Transfer style**
- **A (recommended):** accessible **before → signed delta → after** per-Player animated scoreboard (readable, uses overlay space).
- B: **seat-anchored +/− around actual 3D table** with accessible summary (immersive, needs Seat mapping checks).
- C: **Participant portrait/name result cards** with signed delta and final totals (character focus, more space).

**Q305. Kyoku result controls**
- A: keep **separate restyled existing timer/Continue/Waiting, Pause/Resume and Skip** controls (all options explicit).
- **B (recommended):** one **compact result-footer bar** for Continue/Waiting/Skip, with Review expansion (less clutter).
- C: **Review and Continue side-by-side** and one compact animation control icon (fast access to details).
- All options must retain existing server confirmation/deadline logic, no optimistic advancement.

**Q306. Final Match standings**
- A: **winner large hero portrait + ranked list** (compact/celebratory).
- **B (recommended):** **equal-size portrait cards for all 3/4 Participants**, each clearly marked rank and final score (balanced).
- C: **podium composition** with prominent 1st and supporting lower-ranked panels (dramatic, responsive effort).

**Q307. Post-Match actions**
- A: only **Return to room** (minimal).
- **B (recommended):** Return primary + **Watch Replay** secondary iff persisted and authorized (fast review).
- C: Return primary + **Host/Admin-only Rematch Setup shortcut** if authorized, without auto-start (faster new Match).

**Q308. Results motion intensity**
- **A (recommended):** restrained brief panel reveal/score counting, respect reduced-motion (modern restrained).
- B: skippable ~1–2s staged portrait/tiles/points reveal, no impact to server timeouts (celebratory).
- C: instant result values, no automatic transition; optional replay of visual change (fast analytical).

**Q309. Phone Win/Draw/Final Standings presentation**
- A: **full-screen scrollable outcome layout** (legible, hides more table).
- **B (recommended):** **expandable bottom sheet** above real table, summary then detail (context, extra expansion).
- C: **small centered overlay** with Details expansion (compact, potentially crowded).

### Next steps — after owner UI-6 answers only

Recompute any truly dependent Win/Draw/Score/Final interaction branches, then consistency-audit alongside existing UI-D1–D66. Do not add decorative micro-questions or rewrite prior decisions. Owner approval of scope does NOT authorize code, CI, deployment or merge.

## Final authority map and consistency audit (2026-10-14)

**The earlier UI-1–UI-5 interview Q237–Q300 closed** (with later UI-D65/UI-D66 refinements). **Latest owner UI-D67 now opens a bounded new in-game outcome-screen visual interview UI-6: Q301–Q309 awaiting answers**; the old closing audit is historical for the prior scope. Only this later override supersedes the older Q267/Q284/Q297 side-panel/Dock arrangement. The main Benchmark architecture D1–D239 remains an audit trail, NOT a requirement to reinstate withdrawn Experiment or mandatory Bot-protocol migrations. This audit recorded no owner-level open questions *at that time*; the **later explicit scope change UI-D67 now has Q301–Q309 pending**. An **explicit final owner *design* signoff is still pending**, and it does **not** by itself authorize implementing, running CI, deploying or merging PR #23.

### Authority precedence

0. **Verified normal-Room lifecycle technical truth:** ordinary pre-Match Lobby has no assigned compass Seats. `MatchMachine::with_time_control()` shuffles at Start, `commit_match()` assigns, and `back_to_lobby()` clears. This supersedes the Q296 A fictitious pre-Match Seat order while preserving a stable Participant list.
1. **Latest owner cancellation:** Breaking Benchmark Bot Protocol v2 is withdrawn entirely; shipped MJAI/MCP/Built-in Bot auth/actions continue unchanged. No mandatory Bot capability handshake, required checkpoint attestation, Reset Ack, v1-v2 drain/migration phase, incompatible-protocol gates, or Experiment-dependent data.
2. **Previous owner cancellation Q234 C:** All research A/B Experiment functionality and its paired trials/seed/CI/bootstrap/research export remain completely out of scope. No dormant Experiment features.
3. **All five UI rounds Q237–Q300 plus later UI-D65/UI-D66:** Binding UX decisions; revised Q259–Q274 overrides biased draft. UI-D65 supersedes old normal-game side/Dock action placement; UI-D66 supersedes older Lobby vertical-only composition and passive state-word badges in favor of lightly angled portrait cards with at most two meaningful SVG glyphs. Other decisions survive. Q252 B replaces the old Q241 single-chart limit; Q244 C remains **Benchmark-mobile** chart-first with Q256 B / revised Q263 A defining only opt-in full-screen observation.
4. **Technical reality:** Game/Room/Replay/Rust/SQLite and existing authenticated Bot contracts are authoritative. New UI decisions must be implemented *without* presuming they were already shipped; any actual unimplemented data routing (e.g. list search/filter or Replay chapter view) needs an ordinary implementation design and verification, not a new owner questionnaire.
5. **No code authorization:** This Draft PR is only design documentation; no product changes, CI dispatch, deployment or merge.

### Cross-decision conflict resolution

| Topic | Binding decision | Audit result |
| --- | --- | --- |
| Desktop observation vs analytics | ONE real authorized desktop Three.js table (~2/3), Leaderboard (~1/3), then FOUR default-visible ordinary-Run analytics charts. Full-width hero is cumulative table-relative net; three subordinate charts sit below. | **Consistent**: extra charts do not replace/shrink tabletop or create another renderer. |
| Recent 50 vs cumulative totals | Q275 B defaults the *visible* window to latest 50 persisted completed Matches; Q293 C retains the *absolute cumulative Run-to-date y-values* for plotted Matches and separately labels the **50-Match gain/loss**. `All Matches` changes visible range only. | **Consistent**: no zero-rebase or reset of stored cumulative totals. Label local gain so the window isn't mistaken for a 50-Match total. |
| Ranking and selected-Bot deltas | Leaderboard uses ascending average rank with stable Participant ID tiebreak; Q294 A uses the **current Leaderboard leader only as initial selection on Run-detail open**. Once Admin manually chooses another Bot, ordinary leaderboard refreshing must not silently seize that chart's focus. Q291 B keeps all four chart selections independent. | **Consistent**: fixed Participant identity is not its rotating Seat. With zero completed Matches show `No results yet`, no invented leader. The initial fallback presentation is implementation-owned. |
| Chart drilldown | Q295 A clicking a data point representing a **specific completed Match** opens the *existing* Q271 B/Q288 A Match Detail Drawer (desktop right / phone bottom), with placed scores and Replay action. Keyboard/accessible chart-data-table equivalent required. Rank-distribution aggregate bars have **no single Match ID**; they must not open a random Match Drawer. | **Consistent**: one reusable detail surface, no Experiment navigation. |
| Provisional data vs persistence | Q261 C provisional analysis from real completed records; Q278 B one clear Analytics-wide `Early data · N completed Matches` banner, including sample count/readable caveat. No fabricated missing results or statistical certainty. | **Consistent**: sample thresholds/legends are implementer-level UX and must distinguish small N from missing/no results. |
| Mobile charts vs live 3D | Q244 C/Q262 B/Q279 B: default mobile Run detail displays chart categories (both initially expanded, jump menu) and no inline or background table. Q256 B/Q263 A/Q280 A: **only** explicit `Open Live Table` mounts the **one real authorized** portrait/landscape responsive fullscreen overlay, across Match boundaries, with close restoring chart scroll/focus and releasing confidential viewer state. Q300 B: when no live Match is available, show disabled `Open Live Table` with honest reason (e.g. `Waiting for next Match`, `Run Stopped`) and latest Replay link when persisted. | **Consistent**: no default invisible 3D renderer, second table, leaked hands or stale frame falsely labeled live. A brief between-Match transition must be differentiated from terminal Stopped/Completed. |
| Lobby Participant ordering/appearance | **UI-D66:** large subtly slanted Character portrait cards with names, side-by-side on desktop, at most two discreet meaningful SVG glyphs for passive ready/offline and self identity; no repeated state-word chips. **Verified engine behavior:** ordinary pre-Match Lobby uses **stable Participant ID order**, not fictitious East/South/West/North; old Seat (if ever shown) is `Previous Match Seat`. | **Consistent with newer owner direction and verified Seat lifecycle**: skew only outer frame, not art/name/focus targets; status updates never reorder or invent Seats; accessible icon semantics. |
| Game control availability | **LATEST UI-D65** supersedes Q267 B/Q284 B/Q297 A **placement only**: legal action buttons appear **immediately above the local concealed hand on both PC and phone when relevant**, matching the existing gameplay arrangement / familiar Mahjong Soul interaction. No reserved desktop side panel or separate mobile bottom Dock; the hand itself remains a valid discard target. Decision clocks, pending/rejections, accessibility and privacy still apply. | **Consistent after explicit supersession**: one local, contextual control strip with no extraneous action panel; preserve actual table/score visibility and do not equate normal interactive gameplay with Benchmark read-only viewing. |
| Replay chapters | Q268 B/Q285 B: Kyoku-first replay (desktop vertical chapter list, phone dropdown). Q298 B: jumping to a Kyoku start **preserves play/pause**: playing continues, paused stays paused. Playback controls operate inside selected Kyoku. | **Consistent**: one Replay engine/rendering path. |
| Two-step Join browser Back | Q265 B/Q282 A: same URL, name then Character, explicit in-app Back preserving input. Q299 A: **browser Back navigates away as a normal same-URL page**; it is not intercepted or promoted to step Back. | **Consistent**: document distinction clearly in UX acceptance, avoid phantom history entries. |
| Stop/Resume vs legacy Bots | Stop after current Match needs confirmation and only server-acknowledged pending feedback. Safe manual Resume for Interrupted only, not Stopped, **without new Bot v2 protocol** or invisible reset assumptions. | **Needs verified implementation path**: design settled; actual crash reconstruction/reconnect correctness is a release blocker if unsafe, not a new owner preference question. |
| Theme / whole-app cutover | shadcn/ui Base UI + Tailwind, slate neutral, semantic accents; themed semi-transparent HUD around **unchanged tile/table materials**, shared Admin/public role-aware shell, English-only. Stage changes internally, publish all routes at once, **no prepared old-UI rollback** by owner choice. | **Operational release risk remains**: proper all-route E2E/a11y/visual/privacy/performance Go/No-Go and maintenance/forward-fix plan mandatory. |

### Implementation verification gates (not unresolved owner design questions)

- **Lobby/normal-Room Seat lifecycle and SVG status glyphs**: test pre-Match cards never fabricate East/South/West/North; only authoritative Match creation assigns Seats, prior Seats cannot masquerade as next Seat, and reconnect/Ready changes do not reorder cards. Cards remain stable by immutable Participant ID. Ensure passive Ready/You/Offline SVG markers have accessible names and tooltip/focus explanations; never convey semantic meaning by hue alone. Check that slanted frame keeps art, text, hit targets legible and unobscured across desktop/phone.
- **Real graphical Seat/score regression**: a previously reported normal Room live score-to-Seat mismatch is **unverified**, not already fixed. Verify real rendered pixels and correctness of `participant_id → Seat → score` across **3p/4p normal Room, Benchmark, Replay and any Worker/renderer path**; no fake static data as proof.
- **3p MJAI restriction**: existing 3p MJAI participation is unsupported. New Run Ready Panel must show accurate eligibility/no phantom Bot; do not claim 3p MJAI support or create missing Bots.
- **Run/SQLite/Replay durability and reconnect**: completed Match count advances only with fully persisted result + Replay; exactly once, stable per-Participant scores. Crash/Interrupted Resume must reconcile stored current vs in-flight Match and verify connected roster under **existing** protocol; if safety uncertain, explicitly block instead of auto-retrying an unsafe Attempt.
- **Observation privacy & truthfulness**: Admin-only all concealed hands, role-gated deep links, revoked auth hides cached confidential projections; `Live` vs `Reconnecting` vs `Stale` vs between Matches are accurate. Existing live frontend currently polls HTTP ~1s; SSE remains optional, neither a live guarantee nor new Bot protocol.
- **Data-backed dashboard**: all four charts derived from persisted per-Match rank/score/Participant metadata; correct 3p/4p denominators, absolute cumulative totals in recent-50, visible all-run window, per-Match Drawer/Replay navigation, keyboard/touch accessibility and zero-result states.
- **All-page experience**: Home responsive Join, one-URL two-step form navigation, **subtly tilted portrait Character Lobby cards in stable Participant order with minimal SVG status/self markers, no repeated Ready/You copy or fictitious pre-Match compass Seat**, **UI-D65 action buttons above concealed hand when relevant at every gameplay viewport (not desktop side panel/mobile Dock), without tile occlusion**; Kyoku Replay playback semantics, Admin Rooms Master–Detail/mobile list→detail, theme/contrast, protected Toast/inline errors, mobile one-table on-demand rendering and teardown.
- **Release gate**: cross-route manual/automated acceptance where separately authorized; no prepared rollback means stronger prelaunch evidence and scoped forward fixes/maintenance. This is NOT evidence such tests already ran.
- **Non-goals**: no A/B Experiment or breaking Bot Protocol v2 can be hidden in an implementation milestone. No distributed server, independent Mahjong engine/renderer or speculative research instrumentation.

### Outcome / handoff

**No material unsolved UX/product preference remains after Q300 and the latest UI-D65 owner placement correction.** The only required further owner action is **explicit final DESIGN SCOPE SIGNOFF**, not another grilling round. If signed off, plan work under separately requested authorization; keep PR #23 Draft, docs-only, no CI/deployment/merge. Implementation details such as pixel breakpoints, format of React state, exact chart library, API naming, simple provisional count threshold and test fixture plumbing belong to implementing engineers within these invariants. The per-Run crash recovery safety check and known score/Seat visual bug are real **verification blockers**, not open-ended owner decision questionnaires.

## Historical decision tree — Round UI-2 (ARCHIVE ONLY; owner choices recorded above)

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

## Historical deferments recorded before UI-2 answers (ARCHIVE ONLY; superseded by final Q237–Q300 decisions)

- Exact state/interaction for mobile chart/no-table choice (Q256); no invented mobile live view until decided.
- Typography/spacing tokens and light/dark treatment guided by selected palette; concrete state mockups not fake backend screenshots.
- Design layout and real interactions for other app surfaces: Home/Room join/Lobby/Live/Replay/Admin, using the shared shell and unchanged game renderers.
- Testing and acceptance: real rendered Seat/score mapping, keyboard navigation, visual regression, stale authenticated data; no old-UI rollback.
- Limit questioning to substantive remaining UX trade-offs; delegate cosmetic constants and framework wiring. Owner asked not to overengineer.
