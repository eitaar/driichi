# Benchmark Mode v2 — Grill with Docs (working design)

Date: 2026-10-14
Status: **Design interview in progress — Rounds 1–2 decided; Round 3 unanswered**
Owner: eitaar
Scope: research-oriented improvements to driichi's Benchmark Mode, not implementation approval.

## Sources and invariants

- Existing vocabulary: `CONTEXT.md`.
- Shipped v1 specification: `docs/superpowers/specs/2026-09-29-benchmark-mode-design.md`.
- Existing Run state: `crates/double_riichi_core/src/room.rs`.
- Persistent Run records and statistics: `crates/double_riichi_server/src/storage.rs`, `migrations/0006_benchmark.sql`.
- Existing UI: `frontend/src/benchmark.tsx`.

Keep v1's currently shipped behavior as the compatibility baseline. A design idea below is **not** a requirement or implemented feature until the owner decides it.

## Decisions — Round 1 (2026-10-14)

### D1. Product goal — DECIDED

**Choice:** Research and experimentation platform.

The goal is broader than ranking bots by playing strength. The platform should support rigorous AI-related experiments and the evaluation of changes to software participants. The specific metrics, evidence standards, and experiment lifecycle are **not decided**.

### D2. Compatibility strategy — DECIDED

**Choice:** Extend existing Benchmark Mode with backward compatibility.

Preserve the semantics of existing v1 Benchmark Runs and existing API consumers. Additive versioned capabilities are preferable to silently changing v1's contract. Round 2 selected an additive `Experiment` concept grouping Benchmark Runs. Its precise lifecycle and comparison rules are still to be decided.

### D3. Primary deployment — DECIDED

**Choice:** Self-hosted.

Prioritize one operator controlling a self-hosted instance, rather than building a hosted leaderboard, multi-tenant service, or distributed cluster. Public hosting and collaboration are not goals for the initial v2 scope.

## Decisions — Round 2 (2026-10-14)

### D4. Experiment groups Benchmark Runs — DECIDED (Q4: B)

Introduce **Experiment** as a research-level concept for organizing and comparing multiple existing Benchmark Runs. Preserve Run identity, fixed-roster semantics, recorded Match data, and existing v1 API behavior. Whether an Experiment schedules new Runs, attaches completed Runs, or does both remains open; do not assume this is an automated pipeline.

### D5. Paired Evaluation — DECIDED IN PRINCIPLE (Q5: C)

Support comparisons using controlled shared random conditions to reduce the effect of chance. This selects **paired evaluation**, not a specific seeding algorithm. Exact tile-wall equivalence, seat-balance schedule, RNG ownership, and how different bot decisions cause trajectories to diverge remain open. A common seed alone does not establish equivalent observed game states.

### D6. Outcome + reliability + statistical evidence — DECIDED (Q6: B)

Report and assess game outcomes, runtime/reliability evidence, and statistical uncertainty. The exact measurement point for latency, treatment of timeouts/disconnects, sampling granularity, confidence-interval method, and collection overhead/retention remain open. This does not select full decision-level trace collection for the initial v2 release.

### D7. Recovery at Match boundaries — DECIDED IN PRINCIPLE (Q7: B)

Support resuming eligible interrupted v2 experiments/runs **only at a verified Match boundary**, preserving already-durable outcomes. No replay-based mid-Match continuation is authorized. Startup behavior, manual vs automatic resumption, seed/retry handling for an incomplete attempt, and idempotency conditions remain open. Keep v1's existing fail-closed interrupted-Run behavior unchanged.

### D8. Web UI is the primary interface — DECIDED (Q8: A)

Make the Admin Web UI the primary mechanism for configuring, observing, and comparing v2 experiments. Do not require an editable headless manifest or CLI as a v2 design prerequisite; existing APIs remain compatible.

### D9. Admin live-observation focus — DECIDED USER REQUIREMENT

During a running Benchmark, authenticated Admins can inspect **all bots' concealed hands** and switch which player's hand is the observation focus. Focus changes are strictly read-only: they neither impersonate a Participant nor alter a seat, Controller, legal-action visibility, RNG state, or Run progress.

**Existing implementation:** `Audience::BenchmarkAdmin` already exposes all current hands to an authorized Admin. `frontend/src/benchmark.tsx` currently polls the Admin live endpoint at one-second intervals and renders `ThreeTable` with `surface="replay"`; there is no explicit focus selector in the inspected implementation. The design choice still open is how visual focus behaves (hand panel, camera orientation, or both) and how live updates should be transported.

## Verified v1 baseline (not v2 decisions)

- A Benchmark Run has a fixed participant roster, rotating seats, and 1–1,000 completed and durably recorded Matches.
- Its statistics are mean placement, first-place rate, and cumulative score relative to the table mean.
- Run history survives process restarts, but running Runs become interrupted and are not resumed automatically.
- Reproducible seed input and cross-Run ranking are explicitly excluded by the v1 design.
- MJAI, MCP, and Built-in Bots are existing participant kinds; Benchmark Mode does not admit Human players.
- Current Run identity and results belong to Participants, not to the Bot Token or displayed name.

## Optional extensions — NOT APPROVED

These remain candidates, not implementation mandates:

1. Model registry with version/checkpoint/provenance hashes, beyond the agreed Experiment grouping.
2. Exact deterministic replay and RNG stream design, beyond the agreed Paired Evaluation goal.
3. Model regression thresholds, rankings, and publication of evidence reports.
4. Offline decision evaluation and detailed per-decision traces, which are not selected for initial v2.
5. Dataset/replay export and reproducible experiment manifests; Web UI is the selected primary interface.
6. Parallel workload scheduling and resource quotas beyond the existing self-hosted architecture.
7. A generalized automatic recovery engine, beyond the agreed *boundary-only* resumption capability.

## Decision tree — Round 3 frontier

```text
Benchmark v2
├── D1 Research platform                         [DECIDED]
├── D2 Additive, v1 backward compatibility       [DECIDED]
├── D3 Self-hosted first                          [DECIDED]
├── D4 Experiment groups multiple Runs           [DECIDED]
│   └── Q9 Experiment composition / comparison   [FRONTIER]
│       └── Later: Run immutability, lifecycle, model identity, schemas
├── D5 Paired evaluation                          [DECIDED IN PRINCIPLE]
│   └── Q10 What conditions are paired?          [FRONTIER]
│       └── Later: RNG architecture, shuffle streams, seat balance, fairness tests
├── D6 Outcome + reliability + statistical data  [DECIDED]
│   └── Q11 Evidence granularity and retention   [FRONTIER]
│       └── Later: precise metrics, clocks, inferential method, exports
├── D7 Resume only at verified Match boundaries  [DECIDED IN PRINCIPLE]
│   └── Q12 Who initiates verified resumption?   [FRONTIER]
│       └── Later: transaction proofs, failed attempt semantics, limits
└── D8 Admin Web UI primary + D9 view focus       [DECIDED]
    └── Q13 What does switching focus change?    [FRONTIER]
        └── Later: polling vs push, navigation, focus reset, rendering tests
```

### Round 3 questions — awaiting owner

- **Q9, Experiment composition:** Are Runs manually attached for retrospective comparison, created/scheduled as a new Experiment, or both? Recommendation: both, while retaining independently usable v1 Runs. Clarify which Runs can be paired before designing identifiers.
- **Q10, pairing semantics:** Pair whole Matches by precommitted seed/deal conditions plus complete seat balancing; attempt a stronger decision-independent RNG deal model; or pair offline observations (without claiming match-level counterfactual fairness)? Recommendation: seeded Match blocks and full seat balance first, with recorded divergence limitations.
- **Q11, reliability evidence:** Save only per-Run summary counters; persist per-Match aggregates; or persist detailed per-action timing/error events with controlled retention? Recommendation: per-Match metrics with optional bounded diagnostic traces; distinguish server-observed latency from inference-only time.
- **Q12, boundary resume:** Require explicit Admin approval to restart at a verified boundary; safely auto-resume after restart; or allow a configurable choice? Recommendation: explicit Admin-triggered resume by default.
- **Q13, Admin observation focus:** Switch only a foreground hand/info panel; rotate the virtual camera to the selected seat; or both? Recommendation: both as selectable controls while retaining a read-only all-hands overview.

Defer run limits, concurrency, storage quotas, pairing failure semantics, exact statistics, transport (existing 1 s polling vs SSE), and schema versioning until the decisions above settle their prerequisites.

## Reported visual defect — status UNVERIFIED (2026-10-14)

**User report:** The central table score display was buggy at some point; the user is unsure whether it has since been fixed. Preserve this as an **unconfirmed existing UI regression**, not a verified current defect and not a completed fix.

**Current code evidence:** `frontend/src/game/table-center-pixels.ts` paints per-seat score and round facts into a 2048px Canvas; `three-table-renderer.tsx` memoizes these pixels by score and table facts and forwards a center bitmap to a dedicated Worker; `three-table-worker.tsx` loads the bitmap; `three-table.tsx` separately supplies an accessible score list. `three-table.test.tsx` checks that score changes trigger canvas paint (e.g. 25,000 to 27,600), but that unit test does not prove correct position/orientation or final output pixels on the live GPU.

**User clarification (2026-10-14):** Symptom category **B1 — the displayed score appeared at another player's seat**, rather than being associated with its rightful player's position. This is a *reported Seat-to-score presentation mismatch*, not evidence of incorrect authoritative scoring. The report predates this check; whether it still reproduces on the current build is unknown.

**Unknown:** Which mode and surface exhibited the misassignment (3p/4p, live/replay/Admin), whether the mismatch is a consistent Seat swap or intermittent, whether it occurs after orientation/focus changes, and whether it still reproduces on the current build.

**Required verification before accepting an Admin-focus UI:** Reproduce or explicitly mark unreproduced on the current build. Assign distinct, unmistakable scores to each Seat and verify every displayed score sits adjacent to the correct Participant for all seats in both 3p and 4p; compare against authoritative Match/Room state. Repeat on live, replay and Benchmark Admin surfaces, after seat rotation, after a score update, and when Admin Observation Focus changes. Check visual output pixels—not just text paint calls—in the GPU/Worker renderer and make sure the read-only fallback/text view reports the same mapping. Preserve Admin visibility boundaries. A passing CI result alone is not a visual-fix claim.

**Next clarification:** Ask which surface/game mode displayed the mismatch and whether it happened consistently; the reported class is now **wrong player-seat association (B1)**. The canvas painter and 3D layout both use `tableSeatGeometry(projection.mode, projection.viewer_seat)` in the current code, which merits visual verification but does **not** establish a root cause. Do not change product code during the design interview.

## Decision discipline

- Do not implement v2 features, run migrations, publish releases, or merge until all relevant decisions are settled and the owner confirms shared understanding.
- Update `CONTEXT.md` only with approved domain vocabulary; keep implementation options in this working document.
- Create an ADR only for a decided, consequential and non-obvious trade-off that would be costly to reverse.
- Preserve original v1 documentation unchanged; distinguish implemented facts, owner decisions and unapproved suggestions.
