# driichi UI/UX — Grill with Docs (Rounds UI-1–UI-4)

Date: 2026-10-14
Status: **UI-1 Q237–Q246, UI-2 Q247–Q258, and revised UI-3 Q259–Q274 decided; UI-4 Q275–Q292 awaiting owner. Documentation only.**
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

## Decision tree — Round UI-4 independent frontier

```text
Q259 C four persisted-game charts
├─ Q275  Time window/readable moving-average rank
├─ Q276  Rank distribution visualization
└─ Q277  Per-Match score variation visualization
Q260 C + Q274 C primary/subordinate layout
└─ Q292  Which one of four plots is the hero?
Q261 C provisional/established distinction
└─ Q278  Where/how to mark provisional data?
Q262 B mobile grouped categories
└─ Q279  Mobile category interaction
Q263 A manually opened fullscreen overlay
└─ Q280  Portrait/landscape interaction
Q264 B functional+visual Home
└─ Q281  Mobile Home hierarchy
Q265 B two-step Join
└─ Q282  Two-step navigation/state persistence
Q266 B lobby list
└─ Q283  Lobby Participant row density
Q267 B desktop side panel, mobile bottom Dock
└─ Q284  Legal-action panel dynamics
Q268 B Kyoku-first Replay
└─ Q285  Kyoku jump/navigation form
Q269 A Room Master–Detail
└─ Q286  Mobile Admin Room detail navigation
Q270 B shell + game HUD theme
└─ Q287  Desktop/phone HUD surface emphasis
Q271 B Detail Drawer
└─ Q288  Drawer interaction/detail layout
Q272 B real-time Ready Panel
└─ Q289  Ready Panel position next to roster
Q273 B Toast + blocking inline
└─ Q290  Action feedback when leaving a page
Q259 C + Q260 C dashboard analytics
└─ Q291  Interaction across multiple charts
```

**All 18 questions Q275–Q292 are independent once revised Q259–Q274 have been decided.** Each A/B/C is a plausible UX alternative. Recommendations are diverse and explicitly argued; none requires canceled Experiments, a breaking Bot protocol, unsafe permissions, fictitious data or rewriting the engine. The previous rejected UI-3 draft is **not** a source of selected options. Cosmetic pixels and technical schemas are implementer-level concerns.

## Round UI-4 — awaiting owner (practical alternatives)

**Q275 — Time-range controls for rank trend and other time series**
- A: show the **entire Run** by default, with optional zoom/range selector for long Runs (best for context; more visual detail at 1,000 Matches).
- **B (recommended):** default to **recent 50 Matches**, with an obvious `All Matches` switch (best for short-term changes; hides earlier history until selected).
- C: default to full Run plus an optional 10/25/50-match rolling view toggle (offers flexible insight but has one extra control).
- Why B: current bot form over recent matches is often easier to interpret than a dense 1,000-point timeline; `All` is still one click away.

**Q276 — Placement distribution visual**
- **A (recommended):** **per-Bot horizontal 100%-stacked bar**, segments for 1st/2nd/3rd/(4th) rank (space-efficient, relative proportions easy).
- B: grouped side-by-side vertical bars for rank counts per Bot (absolute counts clearer, comparison harder with 4 Bots).
- C: a **rank × Bot heatmap** with written counts or percentages (information-dense, relies on legend/accessible text).
- Why A: 3p/4p differences read clearly without many axes.

**Q277 — Per-Match score variation chart**
- A: per-Match **scatter plot** of final score per Bot (shows outliers and chronological points, can crowd on 1,000 Matches).
- **B (recommended):** vertical **per-Match net-score-change bars** for selected Bot, with legend to switch participant (clear score gains/losses but only one Bot at a time).
- C: per-Bot **box/violin-like distribution** of per-Match relative score (compact stability view, hides sequence).
- Why B: highlights which individual Matches caused major increases or decreases to the existing relative-net progression.

**Q278 — Provisional analytics indicators**
- **A (recommended):** small **`Early data · N completed Matches` badge + explanation beside each chart**, continuing to render valid data (clear and local, repeated labels).
- B: one shared **Analytics-wide early-data banner** plus exact sample count on chart hover (cleaner, weaker outside full viewport).
- C: subtly tinted or dotted **provisional chart background/series** with a legend (visual but needs noncolor accessible meaning).
- Why A: a chart viewed alone still states its sample limitation.

**Q279 — Mobile default-expanded category controls**
- A: **accordion sections**, Performance and Consistency both open initially; each can be collapsed independently (compact when revisiting).
- **B (recommended):** **anchored vertical sections** with a small sticky jump menu, both visible in normal scroll; optional collapse in each header (easy to jump among several plots).
- C: **two persistent category cards**, each displaying a summary figure plus its already-open charts; collapse card details manually (more summary context, more visual weight).
- Why B: several charts remain visible by default while navigation through them is fast.

**Q280 — Manually opened phone live viewer in portrait and landscape**
- **A (recommended):** responsive **portrait/landscape supported equally**, with controls reflowed and optional browser fullscreen; never force rotation (most predictable).
- B: portrait allowed but offer an optional **`Rotate for larger table`** hint on narrow screens; no forced orientation (more guidance, slight chrome).
- C: open with **landscape-optimized viewing area** and allow pinch/pan in portrait, with a visible close action (big table focus, more gesture learning).
- Why A: browser orientation constraints vary; keep one safe, accessible viewer.

**Q281 — Mobile Home ordering after two-column desktop Hero**
- **A (recommended):** **Join form first**, followed by a compact character/game visual strip; users can enter Room code immediately.
- B: compact hero image/title first with **Join form directly below** (more branding at first glance).
- C: character art and Join in an **alternating swipeable two-panel carousel** with a permanently accessible Join shortcut (expressive, more interaction).
- Why A: the Home entry task is joining a Room, especially from phones.

**Q282 — Navigation/state for two-step Room Join**
- **A (recommended):** **two steps in one route**, retaining entered name and current character in component state, with explicit Back; no additional URL surface.
- B: **separate URLs for name and character** under the Room join path, with safe state handoff/restore (better deep links and browser Back).
- C: first step as normal page, second **Character chooser sheet/modal** retaining underlying form and Back (few route changes, sheet space constraints).
- Why A: simple with current route layout and fewer edge cases.

**Q283 — Lobby Participant-list row treatment**
- **A (recommended):** compact **Seat + Avatar + Display Name + Ready/Connected** rows with a separate short Room summary; ideal for 4 slots.
- B: full **Participant cards** in one column with larger character art and detailed status; more personality, longer mobile scroll.
- C: **grouped `Ready` and `Waiting` lists** retaining Seat labels, visually emphasizing progress but reorders rows as state changes.
- Why A: stable Seat identity and quick readiness check.

**Q284 — Desktop side Action Panel when no decision is pending**
- **A (recommended):** **persistent compact right panel** showing status/timer, expanding legal actions when available; mobile bottom Dock only appears when useful (stable context).
- B: **contextual panel only while an action is pending**, otherwise use small table-level status chip (maximizes game table).
- C: **collapsible side panel** on desktop with status always visible elsewhere and mobile Dock remaining fixed (customizable attention).
- Why A: reduces position jumps when a new call/ron/discard Decision becomes available.

**Q285 — Kyoku-first Replay jump UI**
- A: **horizontal segmented `East 1 / East 2 / ...` tabs**, with event-level scrub controls below (fast for short Match; overflows long Half games).
- **B (recommended):** **vertical Kyoku chapter list** beside Replay on desktop, collapsible dropdown on mobile (scalable to longer Matches).
- C: single **timeline with Kyoku section markers**, where clicking markers jumps to hand start (video-editor feel; more dense navigation).
- Why B: Kyoku is the main unit; chapters preserve long-match readability.

**Q286 — Admin Rooms Master–Detail on narrow screens**
- **A (recommended):** **list screen → selected Room detail screen** with clear Back to list; keep split view on large desktop (easy for mobile).
- B: keep Room detail open with a **collapsible Room picker drawer** on mobile; fewer hard route transitions, possible hidden navigation.
- C: **stacked list and selected Room detail on same mobile scroll page**, with an anchored selection header (simple URL, more scrolling).
- Why A: selected Room actions deserve full mobile width.

**Q287 — HUD surrounding surface under Light/Dark themes**
- A: **opaque token-colored control surfaces** matching shell theme while 3D table stays unchanged (maximum text contrast, visible containers).
- **B (recommended):** compact **semi-translucent HUD/control frame** adjusting to Light/Dark with solid focus/status affordances (subtle game visual cohesion, requires contrast checks).
- C: **separate always-dark gameplay HUD palette** even if shell is Light, with token-colored outer controls (stable table look, theme discontinuity).
- Why B: blends the management shell with a game without recoloring core table artwork; accessibility remains mandatory.

**Q288 — Match-history Detail Drawer interaction**
- **A (recommended):** **right-side Drawer on desktop, bottom sheet on mobile**, with compact placement/score table and Replay action; restore focus/scroll on close.
- B: **side Drawer on all breakpoints**, narrow on phone but keeps consistent slide direction.
- C: **anchored panel within Match history section** that pushes nearby content, not an overlay (good keyboard document flow, taller page).
- Why A: familiar adaptive behavior that preserves the current history context.

**Q289 — Room setup Ready Panel placement**
- A: **sticky sidebar panel** beside eligible roster with selected/required Bot count, blockers and Start (always in view on desktop).
- **B (recommended):** **inline Ready Panel immediately above roster and Start**, updated with current server status; stacks naturally on mobile.
- C: **compact ready summary at top of Room detail**, with a jump to roster and a second Start button near participants (good cross-section overview, duplicate action).
- Why B: no sticky/sidebar state to maintain and setup steps appear together.

**Q290 — Toast+Inline feedback across route changes**
- A: **route-local toasts** vanish when leaving the page; persistent blocking inline errors remain at source (simple semantics).
- **B (recommended):** **app-shell-level temporary toasts** survive a normal route change for successful actions; blocking errors remain tied to the relevant panel (useful after create→Room redirect).
- C: **small action-activity list** in Admin navigation holds recent operation successes and inline errors (more traceability, greater complexity).
- Why B: a successful Room creation can remain confirmed when immediately navigating to the new Room.

**Q291 — Cross-chart Bot selection**
- A: **shared Participant filter/legend** highlights the chosen Bot consistently in all four plots while still allowing comparison overlays (one control to learn).
- **B (recommended):** **independent legend and interaction per chart**, with consistent Bot colors/names but no synchronized selection (simple, different chart questions stay separate).
- C: **single `Compare bots` toggle** to switch all graphs between one-Bot focus and all-Bot overview modes (clear global mode, extra control).
- Why B: each graph may have a different best interaction; consistency comes from participant labels/colors.

**Q292 — Which plot is primary full-width under desktop table?**
- **A (recommended):** **cumulative table-relative net score** is the hero plot; average rank, rank distribution, and Match variation occupy secondary cards.
- B: **moving-average rank** is hero; score and placement-distribution plots are secondary (placement emphasizes competitive results).
- C: **per-Match score variation** is hero; cumulative results and placement metrics are secondary (highlights run volatility).
- Why A: relative score accumulates across entire Run and benefits from a full-width time axis.

## Deferred UI-5 branches (only if choices genuinely unlock them)

Match/Replay accessible focus flows within selected page interactions; responsive chart labels and linked legend semantics, final copy and practical route transition validation; a concise visual acceptance checklist with actual renderer captures for normal Room + Replay + Benchmark. **No further owner questions for trivial Tailwind constants, library props, internal API names or canceled Bot Protocol/Experiment designs.**

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
