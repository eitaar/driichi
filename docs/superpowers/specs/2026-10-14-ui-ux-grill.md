# driichi UI/UX — Grill with Docs (Round UI-1)

Date: 2026-10-14
Status: **Owner requested new UI/UX interview; Q237–Q246 pending, no design selection yet**
Scope: shadcn/ui (Base UI) + Tailwind-based English-only redesign for **all** public/admin pages, starting with Run-first Benchmark Dashboard. One public all-pages cutover; no planned old-UI rollback. Dedicated existing Three.js/Pixi Mahjong renderer remains.

## Dependencies and explicit cancellations

- Owner canceled **all A/B Experiment features** (previous Q234 C).
- Owner now also canceled the **breaking Benchmark Bot protocol v2** and its handshake/required checkpoint/reset/migration/preflight infrastructure. This is a UI/core Benchmark product refresh with the **existing MJAI/MCP/Built-in Bot protocols unchanged**.
- Use the current Rust RoomActor/SQLite, existing Run history/stats and live Admin projection. The existing frontend `frontend/src/benchmark.tsx` provides a Run list, Run details, table, history and current 1-second HTTP polling; `frontend/src/routes.ts` maps `/admin/benchmark` and `/admin/benchmark/runs/:id`. No shadcn/Tailwind/Base UI dependency exists yet. UI designs are NOT implemented.
- Baseline v1 semantics: 1–1,000 consecutive successfully persisted Matches per Run, selected 3p OR 4p ruleset, fixed selected Bot roster, round-robin seat rotation, average rank / first-place rate / accumulated table-relative net score. Preserve current 3p MJAI constraint. `Stopped` vs `Completed` vs `Interrupted` vs `Failed` must be clear.
- Previously approved UI invariants: clear English copy, shared primitives/tokens, keyboard/screen-reader support, real Room/Live/Replay/Benchmark score-to-Seat visual correctness, single live 3D table fullscreen across consecutive Matches, role-aware private-hands viewing, responsive desktop/mobile, existing renderer preserved; actual-data not fabricated; system/light/dark; user selected no old-UI rollback.
- **Do not overengineer:** don't create new generic design systems, multiple renderers, multi-table viewers, Experiment panels, protocols or wire-level affordances. This is design-only and Draft PR-only; no code, CI or merge.

## Existing UX observations (grounded in main branch)

- Current Run list also includes a room-creation form and then navigates to Admin Room for roster selection; it is **not** already a one-click create-and-start flow.
- Run details put status, stats, history and an expandable live `<details>` region on one vertical page. Live observer fetches `getBenchmarkLive` every **1 second** while expanded and renders `ThreeTable`.
- Run stats are already average rank, first-place rate and cumulative net scores; per-Match history links to Replays. No need for a research dashboard.
- UI/UX should respect real permission and connection states, not introduce a false `Live` indicator when a stream/poll is stale. The prior normal Room seat-to-score visual mismatch remains unverified.

## Provisional IA / layout hypothesis (NOT owner decided)

```text
DESKTOP /admin/benchmark
Shared Admin shell (Home / Rooms / Benchmark / Replays / Settings)
[Run list | status | progress | mode | roster | Open]    [New Benchmark]

DESKTOP /admin/benchmark/runs/:id
[Run name · Running · 40/100 · 4p Red East] [Stop after Match]
+--------------------------------+--------------------------+
| ONE real 3D Live Table          | Participant leaderboard  |
| [Fullscreen]  [focus controls]  | Avg rank / 1st% / Net    |
| Status: Live or Stale           | Current Match / status   |
+--------------------------------+--------------------------+
[Simple cumulative net-score trend] [Recent Matches & Replay links]
```

This is a **starting strawman**, not implementation code or a locked layout. A mobile solution must keep the same real current table readable, keep Stop/Resume safe, preserve focus/seat identity, and provide explicit full-screen or in-page fallback.

## Decision tree — Round UI-1 frontier

```text
driichi UI/UX
├── Q237 Visual personality: restrained utility vs cinematic/terminal
├── Q238 Primary desktop Run detail layout
├── Q239 Benchmark index/list organization
├── Q240 New Run creation and roster selection flow
├── Q241 Statistics/graph default density
├── Q242 Run Stop/Resume command placement
├── Q243 Admin focus interaction on 3D table
├── Q244 Mobile Run detail hierarchy
├── Q245 Loading/stale/empty/failure feedback design
└── Q246 Shared application navigation shell across routes

NEXT FRONTIER depends on owner choices:
- precise visual compositions, responsive breakpoints, table/leaderboard drilldowns,
  create-flow validation states, keyboard/touch focus, error recovery, light/dark token samples,
  remaining app pages (Home/Room/Lobby/Live/Replay/Admin).
- Avoid questions about backend protocol/Experiment and cosmetic constants.
```

## Round UI-1 — ask whole independent frontier

All options are **proposals**, not decisions until owner answers.

**Q237. Visual direction** — Overall feeling across the app?
- **A (recommended):** restrained, polished control-room/modern software aesthetic, typography and spacious but useful density, quiet transitions, no decorative pseudo-poetry.
- B: arcade/cinematic with heavy animations and glowing effects across all routes.
- C: ultra-dense terminal/IDE-like admin grids throughout the entire app.

**Q238. Desktop Run-detail layout** — How to balance table and stats?
- **A (recommended):** big live 3D table taking ~2/3 width, pinned information/leaderboard side rail ~1/3; detailed graph/history lower.
- B: full-width graph and charts above, with small live table below.
- C: table only; statistics on another page.

**Q239. Benchmark index / Run list** — Which overview?
- **A (recommended):** clear searchable recent Runs table with status, progress, mode, roster and Open; prominent New Run.
- B: large visual cards per Run in a gallery.
- C: route only contains a giant New Run form; history elsewhere.

**Q240. Create Run UX** — How much guidance without complex wizard?
- **A (recommended):** short New Run sheet/dialog for name/mode/time control, then a clear existing Admin Room roster/connect/select step and Start Run (reflect actual backend preconditions).
- B: long multi-step wizard with many advanced options and auto-generated Bot placeholders.
- C: raw JSON editor/manual API calls.

**Q241. Statistics density by default** — What's essential?
- **A (recommended):** visible leaderboard (average rank, 1st-place rate, cumulative table-relative net score), one small trend plot and collapsible complete Match history/Replay links.
- B: multiple analytic plots, confidence intervals and research views before the table.
- C: raw per-Match output only, no standings.

**Q242. Stop & interrupted Resume actions** — Where?
- **A (recommended):** persistent but nonintrusive contextual Run header actions; confirmation for Stop-after-current-Match, Resume only for safely resumable `Interrupted`, never Stopped; show in-progress feedback.
- B: destructive Stop at every corner without confirmation; one-click Resume any terminal status.
- C: hide all Run controls inside a global Settings page.

**Q243. Admin-focused 3D table UX** — How select the viewpoint?
- **A (recommended):** click/tap an explicitly labeled Participant in a compact focus control; keyboard alternative; keep focus on that Participant when Seats rotate. Fullscreen remains one table.
- B: viewpoint cycles automatically between Bots continuously.
- C: fixed arbitrary Seat with no focus controls.

**Q244. Mobile Run detail priority** — Small screens?
- **A (recommended):** readable one-table live panel first, progress and core controls visible, leaderboard below/in collapsible section, accessible fullscreen or in-page fallback.
- B: show dense desktop two-column layout shrunk down.
- C: hide the live table, show only charts on mobile.

**Q245. Live and failure status UX** — How honest about status?
- **A (recommended):** distinct Live / Reconnecting / Stale / Waiting for next Match / Stopped / Interrupted / Failed / Empty; preserve last snapshot *labeled stale* and provide clear recovery; never display cached state as current.
- B: a single spinner with no underlying status or retry guidance.
- C: always label the table Live even when disconnected.

**Q246. Navigation for the whole app** — How unified?
- **A (recommended):** one shared shell/navigation for Home, Rooms, Lobby/Live, Admin, Benchmarks and Replays, showing role-appropriate links and restoring authorized deep links.
- B: every page has a different menu and unrelated styles.
- C: separate Benchmark brand/site without the shared design system.

## Decision discipline

Await answers such as `全部A、Q237:B、Q238:C`. Then record chosen decisions in this document and the main current scope only if needed; evolve only decisions unlocked by those answers. Glossary is for stable domain terms, not CSS/page placement. Preserve historical choices without treating canceled old Bot-v2 protocol obligations as active. Keep Draft PR docs-only; every docs commit uses `[skip ci]`. No UI code, CI, deployment or merge until separately approved.
