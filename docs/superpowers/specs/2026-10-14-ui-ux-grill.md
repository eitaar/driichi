# driichi UI/UX — Grill with Docs (Rounds UI-1–UI-3)

Date: 2026-10-14
Status: **Rounds UI-1 (Q237–Q246) and UI-2 (Q247–Q258) decided; Round UI-3 (Q259–Q274) awaiting owner. Documentation only, no implementation approved.**
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

## Decision tree — Round UI-3 complete newly unlocked frontier

```text
UI-2 decisions → next material owner-level questions
├── Q252 B: multiple advanced charts by default
│   ├── Q259 Which chart families?
│   ├── Q260 Where do multiple default-visible plots live?
│   └── Q261 What data can charts genuinely claim?
├── Q244 C + Q256 B: mobile chart-first, opt-in live table
│   ├── Q262 Mobile multi-chart grouping
│   └── Q263 Mobile viewer close/return & Match transitions
├── Q247 A + Q258 A: neutral shared shell / role-aware navigation
│   ├── Q264 Home entry and Admin discovery
│   ├── Q265 Room join / character flow
│   ├── Q266 Lobby readiness UX
│   ├── Q267 Normal active Room game/HUD
│   ├── Q268 Replay viewer navigation/timeline
│   ├── Q269 Admin Rooms management density
│   └── Q270 Theme and semantic contrast in real table pages
├── Q251 A / Q253 A: leaderboard + paginated history
│   └── Q271 Per-Match result drilldown
├── Q250 A / Q254 A: setup + guarded actions
│   └── Q272 Setup/progress feedback after create and failed start
├── Q257 A: Stale/reconnection truthfulness
│   └── Q273 Error state placement across app
└── Q238 A / Q248 A: adaptive desktop+table and chart density
    └── Q274 Wide-screen hierarchy while multi-charts default visible
```

Every question below is independently decidable from the answered rounds; **there are no new underlying Bot wire protocol or Experiment questions**. Later layout microdetails and actual color token names are implementer-owned, not new Grill questions.

## Round UI-3 questions — awaiting owner (Q259–Q274)

**Q259 — Which advanced Benchmark plots are default-visible?** Owner selected multiple advanced charts in Q252 B, but canceled all A/B Experiments.
- **A (recommended):** use evidence already persisted per Match to show **cumulative table-relative net score**, **moving average rank / average-rank trend**, **rank-position distribution**, and **first-place rate by elapsed Matches**; include honest small-sample/early-Match labels.
- B: include model inference-time distribution, model-specific compute efficiency and Bootstrap CI even if no verified instrumentation or research feature exists.
- C: display only one net-score chart, contradicting Q252 B.

**Q260 — How to organize several plots while keeping the 3D table first?**
- **A (recommended):** Run detail starts with 3D table/Leaderboard on desktop; below is a titled Analytics section with **multiple plots visible by default** in a responsive grid, and optional collapse controls for compact browsing.
- B: replace most of the 3D table area with a plot dashboard.
- C: place plots behind a tab that allows only one default-visible chart despite Q252 B.

**Q261 — Source-of-truth and unavailable metrics for advanced plots?**
- **A (recommended):** derive only from **durably recorded per-Match ranks/scores/participant IDs**, label complete-sample denominators and unavailable/insufficient history; no fabricated inference latency or A/B confidence intervals.
- B: estimate network/compute timings from Replay scores and label them actual inference timing.
- C: synthesize plausible per-Match datapoints to fill gaps.

**Q262 — Mobile layout of multiple default-visible advanced charts?**
- **A (recommended):** compact Run header/progress, then **stacked responsive advanced plots and leaderboard** in natural scrolling order; keep history lower and no inline 3D renderer.
- B: squeeze a four-chart desktop grid into phone width, requiring tiny labels.
- C: load and display the live 3D table automatically above plots, overriding Q244 C.

**Q263 — What happens on closing manually opened mobile Live Table?**
- **A (recommended):** `Open Live Table` opens one authorized fullscreen/in-page fallback only on tap, stays open between Matches, and `Close` returns to the **same Run detail scroll/chart context**, restoring keyboard focus; unmount private-hand viewer when closed.
- B: open a new tab for each Match and forget the Run page position.
- C: force the Live Table to reopen itself automatically after exit.

**Q264 — Home entry page hierarchy?**
- **A (recommended):** short, understandable English entry page featuring six-digit Room code **Join** as primary action, with guest/public links appropriate to session and a discreet Admin sign-in, preserving character visual identity without cinematic overhead.
- B: showcase a long decorative story/animation before the Join action.
- C: show an Admin-only dashboard to everyone on the Home page.

**Q265 — Room join + character selection journey?**
- **A (recommended):** preserve existing Room lookup → eligible join → name + character selection → clear success/next step, with direct inline errors and keyboard-friendly, non-blocking character previews.
- B: require an unrelated new account or complicated onboarding wizard.
- C: remove character selection entirely from Room gameplay.

**Q266 — Lobby UX for seated guests/Bots?**
- **A (recommended):** show who is in each Seat, Participant names and ready/waiting statuses, valid next action and Room code clearly, with change announcements accessible and no false start when seats are missing.
- B: focus on decorative animations and hide who is ready or present.
- C: automatically fill seats and start the game without legitimate readiness.

**Q267 — Normal active Room Match HUD and controls?**
- **A (recommended):** keep **actual Mahjong table as the primary surface**, show only legal hand/action controls and clear timing/wind/score cues, preserve spectator/public visibility boundary, with responsive controls and no extra dashboard chrome obscuring play.
- B: show the Benchmark Admin's all-hands privileged view to every guest.
- C: replace the table with an admin metrics grid.

**Q268 — Replay viewer information architecture?**
- **A (recommended):** preserve real renderer and timeline with obvious Play/Pause, previous/next significant event, speed and current hand/Match context, plus accessible chronological event list, without new Replay data model.
- B: hide playback behind raw JSON, no timeline or shortcut hints.
- C: develop a second independent Replay renderer with a duplicate rule engine.

**Q269 — Desktop Admin Rooms workspace hierarchy?**
- **A (recommended):** same compact Admin sidebar, Rooms list/search rail and selected Room detail with concise Participant/connection, Bot Token, rule configuration and authorized actions; don't hide critical Room state in multiple modal layers.
- B: full-screen multi-tab animations before showing connected Bots.
- C: make destructive global actions the largest default controls.

**Q270 — Light/Dark surface and game renderer contrast?**
- **A (recommended):** neutral light/dark app shells, semantically colored state badges, accessible text contrast and non-color indicators; keep 3D Mahjong table colors and assets stable rather than recoloring gameplay art with CSS themes.
- B: recolor all Mahjong tiles and discard surfaces automatically when switching theme.
- C: use accent color as the only status indicator without text.

**Q271 — Per-Match historical drilldown within a Run?**
- **A (recommended):** select a Match entry to show compact **persisted placements/points per Bot** and `Watch Replay` navigation; keep the Run context on return; no extra comparison/report module.
- B: open a separate research Experiment builder whenever clicking a Match.
- C: only present an opaque Match ID without result or Replay navigation.

**Q272 — Room created but roster/Run cannot start yet?**
- **A (recommended):** explicit `Room created → Connect and select eligible Bots → Start Run` setup progress; show count, compatibility constraint and actionable next step, with inline server errors, no fabricated auto-fill.
- B: hide the created Room and let the user guess where it went.
- C: silently start with insufficient connected Bot participants.

**Q273 — Error/empty/connection feedback consistency across pages?**
- **A (recommended):** common nonintrusive inline status patterns for loading/empty/stale/error, with an explicit Retry or next action when safe, while auth errors immediately protect private views; no misleading "Live".
- B: one silent indefinite spinner for every failure.
- C: show internal Bot credentials/hidden hands inside support errors.

**Q274 — Preserve desktop hierarchy when charts multiply?**
- **A (recommended):** keep a **large single live table** and a compact fixed-content leaderboard in the first viewport when reasonable, with multiple advanced charts directly below; avoid a huge sticky plot sidebar that shrinks the live table.
- B: make every chart always pinned above the table even when the table becomes a thumbnail.
- C: hide all analytics behind a separate site/app.

## Deferred until UI-3 selections

Only genuinely dependent issues remain: what individual plots show and how to visualize advanced-but-observable stats; how mobile on-demand viewer opens/closes; per-route precise desktop/mobile page mockups under the selected roles/navigation, including the known ordinary-live score-to-Seat regression acceptance. No sprawling new infrastructure/protocol/Experiment design or microconstant questionnaire. No code/CI/merge authorized.

## Historical decision tree — Round UI-2 (now answered)

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

## Historical deferments recorded before UI-2 answers (superseded by current UI-3 frontier)

- Exact state/interaction for mobile chart/no-table choice (Q256); no invented mobile live view until decided.
- Typography/spacing tokens and light/dark treatment guided by selected palette; concrete state mockups not fake backend screenshots.
- Design layout and real interactions for other app surfaces: Home/Room join/Lobby/Live/Replay/Admin, using the shared shell and unchanged game renderers.
- Testing and acceptance: real rendered Seat/score mapping, keyboard navigation, visual regression, stale authenticated data; no old-UI rollback.
- Limit questioning to substantive remaining UX trade-offs; delegate cosmetic constants and framework wiring. Owner asked not to overengineer.
