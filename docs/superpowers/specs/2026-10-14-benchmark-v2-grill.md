# Benchmark Mode v2 — Grill with Docs (working design)

Date: 2026-10-14
Status: **Design interview in progress — Rounds 1–3 decided; Round 4 unanswered**
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

The goal is broader than ranking bots by playing strength. The platform should support rigorous AI-related experiments and the evaluation of changes to software participants. The broad evidence categories and Experiment purpose have now been selected in Rounds 2–3; exact statistical methods, data contracts, and Experiment state transitions remain open.

### D2. Compatibility strategy — DECIDED

**Choice:** Extend existing Benchmark Mode with backward compatibility.

Preserve the semantics of existing v1 Benchmark Runs and existing API consumers. Additive versioned capabilities are preferable to silently changing v1's contract. Round 2 selected an additive `Experiment` concept grouping Benchmark Runs. Round 3 approved both attaching historic Runs and creating new Runs under an Experiment; matching/compatibility rules and exact lifecycle remain open.

### D3. Primary deployment — DECIDED

**Choice:** Self-hosted.

Prioritize one operator controlling a self-hosted instance, rather than building a hosted leaderboard, multi-tenant service, or distributed cluster. Public hosting and collaboration are not goals for the initial v2 scope.

## Decisions — Round 2 (2026-10-14)

### D4. Experiment groups Benchmark Runs — DECIDED (Q4: B)

Introduce **Experiment** as a research-level concept for organizing and comparing multiple existing Benchmark Runs. Preserve Run identity, fixed-roster semantics, recorded Match data, and existing v1 API behavior. Round 3 decided that an Experiment supports both adding previously completed Runs and creating/running new Runs through the Admin interface. This does not authorize a general workflow pipeline.

### D5. Paired Evaluation — DECIDED IN PRINCIPLE (Q5: C)

Support comparisons using controlled shared random conditions to reduce the effect of chance. This selects **paired evaluation**, not a specific seeding algorithm. Round 3 requires recorded fixed seeds and equitable rotation of Seats. The exact seed derivation/pair identification, RNG ownership, and interpretation of diverging bot decisions remain open. A common seed alone does not establish equivalent observed game states.

### D6. Outcome + reliability + statistical evidence — DECIDED (Q6: B)

Report and assess game outcomes, runtime/reliability evidence, and statistical uncertainty. Round 3 requires Match-level summary data and optional diagnostic logs. The exact measurement point for latency, treatment of timeouts/disconnects, confidence-interval method, and log retention remain open. This does not select full decision-level trace collection for the initial v2 release.

### D7. Recovery at Match boundaries — DECIDED IN PRINCIPLE (Q7: B)

Support resuming eligible interrupted v2 experiments/runs **only at a verified Match boundary**, preserving already-durable outcomes. No replay-based mid-Match continuation is authorized. Round 3 requires an explicit Admin action to resume after interruption (no automatic resume). Eligibility checks, the seed/retry handling of an incomplete Match, and idempotency conditions remain open. Keep v1's existing fail-closed interrupted-Run behavior unchanged.

### D8. Web UI is the primary interface — DECIDED (Q8: A)

Make the Admin Web UI the primary mechanism for configuring, observing, and comparing v2 experiments. Do not require an editable headless manifest or CLI as a v2 design prerequisite; existing APIs remain compatible.

### D9. Admin live-observation focus — DECIDED USER REQUIREMENT

During a running Benchmark, authenticated Admins can inspect **all bots' concealed hands** and switch which player's hand is the observation focus. Focus changes are strictly read-only: they neither impersonate a Participant nor alter a seat, Controller, legal-action visibility, RNG state, or Run progress.

**Existing implementation:** `Audience::BenchmarkAdmin` already exposes all current hands to an authorized Admin. `frontend/src/benchmark.tsx` currently polls the Admin live endpoint at one-second intervals and renders `ThreeTable` with `surface="replay"`; there is no explicit focus selector in the inspected implementation. Round 3 decided that selecting a bot both changes the 3D table's viewing perspective and enlarges that bot's hand. The intended choice is a read-only presentation transformation (no privileged Player View impersonation). Focus identity across Match seat rotations, view defaults, and live-update transport remain open.

## Decisions — Round 3 (2026-10-14)

### D10. Existing and newly created Runs in an Experiment — DECIDED (Q9: C)

Experiment supports **both** attaching previously completed Benchmark Runs for retrospective comparison and creating/running new Benchmark Runs from its Admin Web UI. A Run remains independently addressable and its committed Match results must not be silently rewritten when linked. Whether old Runs missing seed/provenance can participate in *paired* comparisons, and whether linking is reversible, are open decisions.

### D11. Fixed random seed and balanced seats — DECIDED (Q10: B)

For paired comparisons, record/fix the random seed used for evaluation and **rotate the seats evenly** among Participants across the comparison schedule. Preserve exact pairing metadata; equal seed identifiers are not sufficient proof of equivalent draw trajectories once decisions diverge. The RNG stream protocol, seed schedule and the definition of valid pairing are not yet chosen.

### D12. Match-level reliability evidence with optional diagnostics — DECIDED (Q11: B)

Store outcome and reliability measurements at **Match granularity** by default; expose **detailed logs only when needed**. Diagnostic logging must be controllable and must not be assumed on by default. Latency semantics, log limits and retention, collection overhead, and statistical estimators remain undecided.

### D13. Admin-initiated Match-boundary recovery — DECIDED (Q12: A)

Resumption of eligible interrupted v2 work requires the **Admin to choose Resume** in the UI. Already committed Matches remain valid; an incomplete Match cannot be marked completed and there is no mid-Match restoration. Confirming that the roster, model implementations, and run configuration remain sufficiently compatible, and deciding how an interrupted scheduled pairing is retried, remain open. The legacy v1 interruption policy is unchanged.

### D14. Focused 3D spectator view plus enlarged hand — DECIDED (Q13: C)

For a running Benchmark, Admin may select a bot to **rotate the 3D viewpoint toward that bot** and **enlarge/show that bot's current concealed hand**. Switching observation focus changes presentation only, not projection authorization, who is acting, game state, legal actions, or private information available to bots. The display must maintain correct Player/Seat/score correspondence in 3p/4p while focusing or rotating; the reported normal-Match center-score association defect remains unverified as fixed. Selection persistence and live synchronization are open.

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

## Decision tree — Round 4 frontier

```text
Benchmark v2
├── D1–D3 Research, additive compatibility, self-hosted          [DECIDED]
├── D4 & D10 Experiment grouping + historic/new Runs             [DECIDED]
│   └── Q14 Historic Run comparability and evidence labels      [FRONTIER]
│       └── Later: model provenance, immutable snapshots, linking rules, migration
├── D5 & D11 Paired evaluation with fixed seed + seat rotation   [DECIDED]
│   └── Q15 Seed ownership and reproducible schedule            [FRONTIER]
│       └── Later: RNG streams, block generation, paired estimators, verification
├── D6 & D12 Outcomes/reliability + Match data, optional logs    [DECIDED]
│   └── Q16 Definition/source of decision latency               [FRONTIER]
│       └── Later: timeout categories, sample statistics, trace quotas/retention
├── D7 & D13 Boundary-only, Admin-triggered recovery            [DECIDED]
│   └── Q17 Failed scheduled Match recovery policy              [FRONTIER]
│       └── Later: transactional integrity, attempt ID, bounded retry, resume gates
├── D8–D9 & D14 Admin UI, focus rotation and enlarged hand       [DECIDED]
│   └── Q18 Which identity does viewer focus track?             [FRONTIER]
│       └── Later: presentation transform, score orientation tests, live transport
└── D3 & D10 Self-hosted execution of multiple Runs             [DECIDED]
    └── Q19 Concurrency of active Runs                          [FRONTIER]
        └── Later: limits, scheduling, backpressure, CPU/GPU resource isolation
```

### Round 4 questions — awaiting owner

- **Q14, retrospective compatibility and comparability:** A — allow all old Runs and treat comparisons as equivalent; B — allow all old Runs but clearly distinguish paired, unpaired and incompatible comparisons according to verified provenance; C — only accept fully verifiable paired Runs. **Recommendation B:** preserve exploratory analysis without misrepresenting scientific comparability.
- **Q15, seed authority:** A — Admin enters a single raw seed manually; B — server pre-generates and persists a fixed sequence of per-block seeds; C — allow manual root seed or generated root seed, both expanded into a recorded immutable schedule. **Recommendation C:** reproducibility plus approachable Admin UI; exact RNG stream ownership is a later design question.
- **Q16, latency definition and source:** A — server-observed request-to-response latency only; B — server-observed latency always, optional bot-supplied inference timing recorded separately; C — rely on bot-reported inference time. **Recommendation B:** compare transport/protocol latency fairly while preserving optional profiling data.
- **Q17, interrupted scheduled Match:** A — keep an incomplete attempt as failure evidence and reissue the scheduled Match with the same seed and roster, a new Match ID and explicit Admin confirmation; B — skip its planned seed/block as invalid and continue; C — let Admin select retry/skip at each resumption. **Recommendation A:** retained pairing plan plus auditable failed attempts; exact eligibility and retry bounds are deferred.
- **Q18, observation-focus persistence:** A — track a chosen **Participant ID** across seat rotations; B — track the numbered Seat even when another bot occupies it; C — reset focus every Match. **Recommendation A:** a named bot remains selected even as its Seat changes, with the visual score labels bound to actual seats.
- **Q19, Experiment-managed Run concurrency:** A — sequential Runs inside each v2 Experiment, without restricting standalone v1 Runs; B — bounded parallel Runs inside each Experiment; C — unconstrained parallel Runs. **Recommendation A initially:** preserve v1 compatibility and provide predictable Experiment scheduling; controlled concurrency can be added later.

### Deferred decisions

Once the Round 4 choices are settled, clarify: model/version identity and engine compatibility, what counts as a valid paired sample, complete fairness treatment of bot-dependent draws, recovery eligibility and idempotent transactions, detailed statistical methods and confidence intervals, storage quotas and retention, polling versus SSE/WebSocket for Admin observation, UI fallback and camera transitions, experiment/Run lifetime and caps, schema/API versioning, and precise visual acceptance tests.

## Reported visual defect — status UNVERIFIED (2026-10-14)

**User report:** The central table score display was buggy at some point; the user is unsure whether it has since been fixed. Preserve this as an **unconfirmed existing UI regression**, not a verified current defect and not a completed fix.

**Current code evidence:** `frontend/src/game/table-center-pixels.ts` paints per-seat score and round facts into a 2048px Canvas; `three-table-renderer.tsx` memoizes these pixels by score and table facts and forwards a center bitmap to a dedicated Worker; `three-table-worker.tsx` loads the bitmap; `three-table.tsx` separately supplies an accessible score list. `three-table.test.tsx` checks that score changes trigger canvas paint (e.g. 25,000 to 27,600), but that unit test does not prove correct position/orientation or final output pixels on the live GPU.

**User clarification (2026-10-14):** Symptom category **B1 — the displayed score appeared at another player's seat**, rather than being associated with its rightful player's position. This is a *reported Seat-to-score presentation mismatch*, not evidence of incorrect authoritative scoring. The report predates this check; whether it still reproduces on the current build is unknown.

**Surface confirmed by user (2026-10-14):** The B1 score-to-seat mismatch was observed in **ordinary live play (normal Room Match)**. The user has **not verified whether the same issue appears in Replay or Benchmark Admin live observation**; neither may be asserted buggy or fixed. The user has not specified 3p vs 4p.

**Unknown:** Whether the mismatch is a consistent Seat swap or intermittent, whether it occurs after orientation changes, and whether it still reproduces on the current build. Replay and Benchmark Admin view remain untested by the user.

**Required verification before accepting an Admin-focus UI:** Reproduce or explicitly mark unreproduced on the current build. Assign distinct, unmistakable scores to each Seat and verify every displayed score sits adjacent to the correct Participant for all seats in both 3p and 4p; compare against authoritative Match/Room state. Repeat on live, replay and Benchmark Admin surfaces, after seat rotation, after a score update, and when Admin Observation Focus changes. Check visual output pixels—not just text paint calls—in the GPU/Worker renderer and make sure the read-only fallback/text view reports the same mapping. Preserve Admin visibility boundaries. A passing CI result alone is not a visual-fix claim.

**Next clarification (only if needed later):** Determine whether the ordinary live Match was 3p or 4p and whether the mismatch was consistent; **do not ask again which surface**, as normal live play is confirmed. The canvas painter and 3D layout both use `tableSeatGeometry(projection.mode, projection.viewer_seat)` in the current code, which merits visual verification but does **not** establish a root cause. Do not change product code during the design interview.

## Decision discipline

- Do not implement v2 features, run migrations, publish releases, or merge until all relevant decisions are settled and the owner confirms shared understanding.
- Update `CONTEXT.md` only with approved domain vocabulary; keep implementation options in this working document.
- Create an ADR only for a decided, consequential and non-obvious trade-off that would be costly to reverse.
- Preserve original v1 documentation unchanged; distinguish implemented facts, owner decisions and unapproved suggestions.
