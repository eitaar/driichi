# driichi UI/UX — Grill with Docs (Rounds UI-1–UI-2)

Date: 2026-10-14
Status: **Round UI-1 (Q237–Q246) decided; Round UI-2 (Q247–Q258) pending owner answers. No implementation approved.**
Scope: shadcn/ui (Base UI) + Tailwind-based English-only redesign for **all** public/admin pages, starting with Run-first Benchmark Dashboard. One public all-pages cutover; no planned old-UI rollback. Dedicated existing Three.js/Pixi Mahjong renderer remains.

## Dependencies and explicit cancellations

- Owner canceled **all A/B Experiment features** (previous Q234 C).
- Owner now also canceled the **breaking Benchmark Bot protocol v2** and its handshake/required checkpoint/reset/migration/preflight infrastructure. This is a UI/core Benchmark product refresh with the **existing MJAI/MCP/Built-in Bot protocols unchanged**.
- Use the current Rust RoomActor/SQLite, existing Run history/stats and live Admin projection. The existing frontend `frontend/src/benchmark.tsx` provides a Run list, Run details, table, history and current 1-second HTTP polling; `frontend/src/routes.ts` maps `/admin/benchmark` and `/admin/benchmark/runs/:id`. No shadcn/Tailwind/Base UI dependency exists yet. UI designs are NOT implemented.
- Baseline v1 semantics: 1–1,000 consecutive successfully persisted Matches per Run, selected 3p OR 4p ruleset, fixed selected Bot roster, round-robin seat rotation, average rank / first-place rate / accumulated table-relative net score. Preserve current 3p MJAI constraint. `Stopped` vs `Completed` vs `Interrupted` vs `Failed` must be clear.
- Previously approved UI invariants: clear English copy, shared primitives/tokens, keyboard/screen-reader support, real Room/Live/Replay/Benchmark score-to-Seat visual correctness, **one desktop/large-screen live 3D table** with fullscreen across consecutive Matches, role-aware private-hands viewing, existing renderer preserved; actual-data not fabricated; system/light/dark; user selected no old-UI rollback. **Owner Q244 C now overrides any older claim that the mobile Run-detail dashboard must show or prioritize a 3D table: mobile is chart-first with the live table hidden.** Whether an explicit optional mobile entry point to live view exists is unresolved Q253, not silently assumed.
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

### UI-D5. Focused ordinary Benchmark statistics — DECIDED (Q241 A)

Visible Leaderboard per software Participant: **average rank, first-place rate and cumulative score relative to table mean**, with one compact cumulative trend chart. Allow collapsing the full per-Match results/Replay history. No canceled Experiment/Bootstrap confidence metrics.

### UI-D6. Contextual Run action UX — DECIDED (Q242 A)

Run header holds unobtrusive **Stop after current Match** with a confirmation and pending feedback; **Resume** appears only if an `Interrupted` Run is verifiably safe to resume using existing Bot/Room protocols, **never** from terminal `Stopped`. Actions must remain server-authorized, revision-safe and non-duplicating; the UI cannot promise impossible recovery.

### UI-D7. Explicit Participant focus in desktop live 3D — DECIDED (Q243 A)

Select a Participant with a compact labeled focus control, accessible by pointer/touch/keyboard, not a cycling camera. Follow Participant ID as Seat rotates; protect private all-hand Admin data. An in-game camera rotation is not an identity switch.

### UI-D8. **Mobile Run detail is chart-first; hide live 3D table** — DECIDED (Q244 **C**, overrides A suggestion)

**On small-screen Run detail, the live 3D Mahjong table must NOT be displayed by default.** The content focuses on performance charts/Run statistics rather than an automatic/shrunken table. Essential Run progress, valid Admin operations, status and accessibility remain available; hiding a table may **not** hide required Stop/Resume or show a fake Live indicator. This is the owner's intentional deviation from the previous proposed mobile-table-first fallback. Any **on-demand** mobile live-table route/Fullscreen entry is not yet decided and is specifically Q253; do not silently include it or silently assume it is forbidden. Desktop/large-screen live 3D remains mandatory.

### UI-D9. Honest live/reconnection/empty/terminal states — DECIDED (Q245 A)

Distinguish `Live`, `Reconnecting`, `Stale`, `Waiting for next Match`, `Stopped`, `Interrupted`, `Failed` and `Empty`. Show known historical snapshot labeled stale, not as current. Clear retry/next steps; stop relying on a generic indefinite spinner.

### UI-D10. One role-aware application navigation shell — DECIDED (Q246 A)

Across Home, Room, Lobby, Live, Admin, Benchmark and Replay, use one shared shadcn/**Base UI** + Tailwind component/tokens vocabulary with role-appropriate links, safe deep links and clear route context. This does not grant Admin information to non-Admin users; existing public/private game views remain separate.

### Recorded supersession / unresolved UX consequences

- **Q244 C supersedes the earlier generic "mobile fullscreen live table fallback" proposal for the default mobile Benchmark Run-detail surface.** Existing game/Room and Replay views are not banned on mobile by this choice. The optionality of launching a single live table manually on mobile from Run detail is Q253.
- No change to abandoned **breaking Bot v2 protocol** or canceled **A/B Experiment** design.
- The new **Run index** and **Run detail** use currently existing routes and data; additional UX can be a refactor, not a new backend scheduler.
- "Live" status represents fresh authorized server data, not a promise that a connection exists or that the Run is currently Playing.
- No UI/frontend/backend code, visual snapshot or CI was generated/executed by this documentation interview.

## Existing UX observations (grounded in main branch)

- Current Run list also includes a room-creation form and then navigates to Admin Room for roster selection; it is **not** already a one-click create-and-start flow.
- Run details put status, stats, history and an expandable live `<details>` region on one vertical page. Live observer fetches `getBenchmarkLive` every **1 second** while expanded and renders `ThreeTable`.
- Run stats are already average rank, first-place rate and cumulative net scores; per-Match history links to Replays. No need for a research dashboard.
- UI/UX should respect real permission and connection states, not introduce a false `Live` indicator when a stream/poll is stale. The prior normal Room seat-to-score visual mismatch remains unverified.

## Confirmed Dashboard wireframe — post Round UI-1

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
[Single cumulative relative-score line plot]
[Expandable Match results + Replay history]

MOBILE /admin/benchmark/runs/:id   (Q244 C, NOT previous A)
[Run title, progress, status, safe contextual actions]
[Performance graph / Run statistics]
[History/replay navigation]
NO inline/autoplay/shrunken live 3D table.
Q253 still determines whether a separately requested live view is permitted.
```

All positions are **logical UX composition**, not a functioning preview, frontend code, tested breakpoint, or assertion that a particular backend SSE endpoint exists. All matches/rosters/graph values shown elsewhere are illustrative unless backed by real storage.

## Decision tree — Round UI-2 frontier (Q247–Q258)

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

Questions below are **the complete newly unlocked independent frontier**; later questions on all-page refinements, exact accessibility/focus and smaller graphical tokens depend on choices here. **There is no product-code implementation approval.**

## Round UI-2 questions — awaiting owner

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

## Later UI/UX branches (wait for UI-2 owner answers)

- Exact state/interaction for mobile chart/no-table choice (Q256); no invented mobile live view until decided.
- Typography/spacing tokens and light/dark treatment guided by selected palette; concrete state mockups not fake backend screenshots.
- Design layout and real interactions for other app surfaces: Home/Room join/Lobby/Live/Replay/Admin, using the shared shell and unchanged game renderers.
- Testing and acceptance: real rendered Seat/score mapping, keyboard navigation, visual regression, stale authenticated data; no old-UI rollback.
- Limit questioning to substantive remaining UX trade-offs; delegate cosmetic constants and framework wiring. Owner asked not to overengineer.
