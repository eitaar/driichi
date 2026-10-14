# Benchmark Mode v2 — Grill with Docs (working design)

Date: 2026-10-14
Status: **Design interview in progress — Rounds 1–5 decided; Round 6 unanswered**
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

Preserve the semantics of existing v1 Benchmark Runs and existing API consumers. Additive versioned capabilities are preferable to silently changing v1's contract. Round 2 selected an additive `Experiment` concept grouping Benchmark Runs. Round 3 approved both attaching historic Runs and creating new Runs under an Experiment. Round 4 limits Experiment registration to Runs whose relevant comparison conditions can be verified; the evidence schema and Experiment lifecycle remain open.

### D3. Primary deployment — DECIDED

**Choice:** Self-hosted.

Prioritize one operator controlling a self-hosted instance, rather than building a hosted leaderboard, multi-tenant service, or distributed cluster. Public hosting and collaboration are not goals for the initial v2 scope.

## Decisions — Round 2 (2026-10-14)

### D4. Experiment groups Benchmark Runs — DECIDED (Q4: B)

Introduce **Experiment** as a research-level concept for organizing and comparing multiple existing Benchmark Runs. Preserve Run identity, fixed-roster semantics, recorded Match data, and existing v1 API behavior. Round 3 decided that an Experiment supports both adding previously completed Runs and creating/running new Runs through the Admin interface. This does not authorize a general workflow pipeline.

### D5. Paired Evaluation — DECIDED IN PRINCIPLE (Q5: C)

Support comparisons using controlled shared random conditions to reduce the effect of chance. This selects **paired evaluation**, not a specific seeding algorithm. Round 3 requires recorded fixed seeds and equitable rotation of Seats. Round 4 allows the Admin to supply a root seed or let the server generate one; both record the actual chosen seed. Exact expansion/stream ownership, pair identification, and interpretation of diverging decisions remain open. A common seed alone does not establish equivalent observed game states.

### D6. Outcome + reliability + statistical evidence — DECIDED (Q6: B)

Report and assess game outcomes, runtime/reliability evidence, and statistical uncertainty. Round 3 requires Match-level summary data and optional diagnostic logs. Round 4 requires server-observed latency for all eligible bot decisions and optionally collects agent-reported inference time **separately**. Round 5 selected both first-response and accepted-legal-action latency, separate invalid/timeout evidence, and quota/expiry-managed diagnostics. Exact clock/framing semantics, failure denominators, and confidence-interval methods remain open. This does not select full decision-level trace collection for the initial v2 release.

### D7. Recovery at Match boundaries — DECIDED IN PRINCIPLE (Q7: B)

Support resuming eligible interrupted v2 experiments/runs **only at a verified Match boundary**, preserving already-durable outcomes. No replay-based mid-Match continuation is authorized. Round 3 requires an explicit Admin action to resume after interruption (no automatic resume). Round 4 requires recording failed attempts separately and allowing a new Match attempt with the original planned seed and fixed roster after Admin Resume. Round 5 bounds each scheduled Match to three Attempts including the original (two retries); eligibility checks, Bot state reset verification, and idempotency conditions remain open. Keep v1's existing fail-closed interrupted-Run behavior unchanged.

### D8. Web UI is the primary interface — DECIDED (Q8: A)

Make the Admin Web UI the primary mechanism for configuring, observing, and comparing v2 experiments. Do not require an editable headless manifest or CLI as a v2 design prerequisite; existing APIs remain compatible.

### D9. Admin live-observation focus — DECIDED USER REQUIREMENT

During a running Benchmark, authenticated Admins can inspect **all bots' concealed hands** and switch which player's hand is the observation focus. Focus changes are strictly read-only: they neither impersonate a Participant nor alter a seat, Controller, legal-action visibility, RNG state, or Run progress.

**Existing implementation:** `Audience::BenchmarkAdmin` already exposes all current hands to an authorized Admin. `frontend/src/benchmark.tsx` currently polls the Admin live endpoint at one-second intervals and renders `ThreeTable` with `surface="replay"`; there is no explicit focus selector in the inspected implementation. Round 3 decided that selecting a bot both changes the 3D table's viewing perspective and enlarges that bot's hand. The intended choice is a read-only presentation transformation (no privileged Player View impersonation). Round 4 requires selecting the Participant identity, not the seat, so the view tracks that bot across seat rotations. Round 5 selected authenticated SSE updates with reconnect snapshots and ongoing authorization checks plus a polling fallback; view defaults, camera transitions and event granularity remain open.

## Decisions — Round 3 (2026-10-14)

### D10. Existing and newly created Runs in an Experiment — DECIDED (Q9: C)

Experiment supports **both** attaching previously completed Benchmark Runs for retrospective comparison and creating/running new Benchmark Runs from its Admin Web UI. A Run remains independently addressable and its committed Match results must not be silently rewritten when linked. Round 4 restricts admission to Runs whose relevant experiment conditions can be verified; historical Runs without sufficient evidence are not retroactively treated as reproducible or admitted. Round 5 requires trustworthy code/build and checkpoint provenance alongside verified game conditions; how to attest the actual running artifact and whether linking is reversible remain open.

### D11. Fixed random seed and balanced seats — DECIDED (Q10: B)

For paired comparisons, record/fix the random seed used for evaluation and **rotate the seats evenly** among Participants across the comparison schedule. Preserve exact pairing metadata; equal seed identifiers are not sufficient proof of equivalent draw trajectories once decisions diverge. Round 4 allows either an Admin-supplied or server-generated seed, and requires preserving the actual seed. Round 5 selected deterministic per-Match seed derivation with separately labeled RNG streams. The derivation/stream protocol and unit of a paired comparison remain to be specified.

### D12. Match-level reliability evidence with optional diagnostics — DECIDED (Q11: B)

Store outcome and reliability measurements at **Match granularity** by default; expose **detailed logs only when needed**. Diagnostic logging must be controllable and must not be assumed on by default. Server-observed latency is required while optional bot-reported inference time is recorded separately and clearly labeled; first-response and accepted-legal-action timing boundaries, quota/expiry-managed diagnostics, collection overhead, and statistical estimators still require precise technical contracts.

### D13. Admin-initiated Match-boundary recovery — DECIDED (Q12: A)

Resumption of eligible interrupted v2 work requires the **Admin to choose Resume** in the UI. Already committed Matches remain valid; an incomplete Match cannot be marked completed and there is no mid-Match restoration. Round 4 requires retaining a failed attempt record and restarting its scheduled Match from the beginning with the **same seed and roster** using a **new Match ID** following Admin Resume. The original failed attempt stays excluded from result statistics. Round 5 permits at most two retries per scheduled Match; proving environment/model compatibility, resetting state correctly, and atomic/idempotent accounting remain open. The legacy v1 interruption policy is unchanged.

### D14. Focused 3D spectator view plus enlarged hand — DECIDED (Q13: C)

For a running Benchmark, Admin may select a bot to **rotate the 3D viewpoint toward that bot** and **enlarge/show that bot's current concealed hand**. Switching observation focus changes presentation only, not projection authorization, who is acting, game state, legal actions, or private information available to bots. The display must maintain correct Player/Seat/score correspondence in 3p/4p while focusing or rotating; the reported normal-Match center-score association defect remains unverified as fixed. Round 4 makes Observation Focus follow the selected **Participant ID** across Seat rotations. Selecting a different bot changes presentation only; defaults, live synchronization and 3D transition style are open.

## Decisions — Round 4 (2026-10-14)

### D15. Strict Experiment registration — DECIDED (Q14: C)

Only a Benchmark Run whose **relevant experiment conditions are fully verifiable** can be admitted to an Experiment, whether it was created by that Experiment or previously completed. In particular, missing or merely invented seed/agent provenance must never be backfilled and presented as verified. Existing v1 history is **preserved and still accessible outside Experiment**, regardless of eligibility. Verifiability is a product eligibility gate, not a claim of mathematical equality of different bot trajectories. What evidence counts as sufficient verification is the next decision.

**Code constraint:** Current v1 `BenchmarkRunRecord` captures roster/seat/results, status and game mode but does not itself contain a recorded random seed or immutable bot/checkpoint and engine provenance; therefore existing stored v1 Runs do not automatically satisfy this gate.

### D16. Admin-entered or auto-generated seed — DECIDED (Q15: C)

The Admin UI supports either **specifying a seed** or requesting that the **server generate a seed**. Whichever source is chosen, the effective seed and derivation/provenance must be persisted with the planned comparison conditions; never confuse an omitted/default seed with a recorded reproducible seed. The algorithm for deriving per-Match or per-Kyoku randomness and tying equivalent scheduled blocks together is not decided.

### D17. Separately labeled server and bot timing — DECIDED (Q16: B)

The server **must record its own observed response-latency measurements**; instrumented bots **may additionally supply inference-only timing**. The two are not interchangeable; do not treat a bot's self-reported timing as independently verified or as the primary cross-protocol metric. Exact decision start/end boundaries, missing/invalid responses, and per-Match aggregation are to be specified later.

### D18. Retry interrupted Match from start — DECIDED (Q17: A)

After Admin-triggered Resume, preserve the failed attempt and retry its originally scheduled Match **from the start** using the same recorded seed and Participant roster, with a **new Match ID**. Never count an incomplete attempt as a successfully completed Match; never mutate previous committed results. The replay/Match/attempt identity model and reset/verification checks remain open. The retry budget is one original Attempt plus two retries.

**Safety edge case:** A connected Bot may retain in-memory state across attempts or be replaced with a different checkpoint. The same seed and Participant ID are insufficient to guarantee equivalence. Resumption must not silently claim valid pairing until this is addressed.

### D19. Follow Participant through Seat rotations — DECIDED (Q18: A)

The Admin's Observation Focus is anchored to the selected software **Participant**, not its current Seat. When the next Match rotates seats, keep the same bot in focus, change the table viewpoint to its new Seat, and enlarge that bot's hand. The score displayed near any Seat must always refer to the actual Participant occupying that Seat. Do not conflate viewer perspective and authoritative game state.

### D20. Sequential Runs per Experiment — DECIDED (Q19: A)

Within each Experiment, at most **one Experiment-managed Benchmark Run** is active at once; its planned Runs execute in sequence. **Do not restrict independent legacy v1 Runs** because of this v2 rule. Round 5 requires freezing comparison-defining conditions at the first Match start; exactly how the Run queue starts, pauses, advances and completes remains open.

## Decisions — Round 5 (2026-10-14)

### D21. Verified agent build and model artifact evidence — DECIDED (Q20: B)

A Run is eligible for strict Experiment membership only if **both** its game conditions (rules/mode, effective seeds, engine identity/version and time control) **and** each participating agent's executable/build identity and, when applicable, exact model checkpoint identity have trustworthy recorded evidence. A Participant ID, display name, Bot Token, user-typed hash, or unauthenticated model metadata alone is insufficient. Existing v1 Runs with absent evidence remain accessible as v1 history but do not gain a fabricated verification status.

**Unresolved verification boundary:** An external MJAI/MCP process can report a hash without actually running that artifact. How the operator establishes the identity of the executable/checkpoint that actually served a response is **not yet decided**. Do not treat self-declared strings as independently verified proof.

### D22. Deterministic per-Match seeds and independent RNG streams — DECIDED (Q21: B)

Persist an Admin-supplied or server-generated **Root Seed**, derive per-Match seeds deterministically, and use separately labeled RNG streams for logically independent randomness domains. This reduces accidental dependence on unrelated control-flow changes. Exact derivation algorithm, stream labels, per-Kyoku behavior, engine revision constraints and the unit of a paired sample are still open.

Equal seeds do **not** imply identical game trajectories after bots make different choices. Replaying a failed scheduled Match must reuse its assigned seed/schedule rather than drawing a new fixture.

### D23. Two server-observed latency measures — DECIDED (Q22: C)

Measure **first response arrival latency** and **accepted legal action latency** separately. Record malformed/invalid actions, timeout and missing-response evidence separately rather than inventing a valid-action time. Optional bot-supplied inference time remains self-reported and non-equivalent to either server-observed metric. Clock start points, protocol framing for MJAI/MCP/Built-in Bots and aggregate estimators remain open.

### D24. Maximum three Match Attempts — DECIDED (Q23: B)

For each planned Match there may be **one original Attempt and at most two retries**. Each retry needs Admin-initiated Resume, preserves the previous failed Attempt, uses the same planned seed/roster and creates a new Match ID. Once all three Attempts fail, no further retry is permitted under that plan. The terminal/blocked state and remediation remain open. Do not claim comparability if the Bot build/checkpoint or reset state cannot be trusted.

### D25. SSE-powered Admin live observation — DECIDED (Q24: B)

Admin live observation uses an **authenticated SSE stream** with initial/reconnect snapshots, revision consistency and revocation/expiry enforcement; **polling remains a fallback**. Do not leak concealed hands to other audiences, future wall order, or opponents' private legal-action menus. Bounded event/update policy, slow-client handling and connection limits remain open.

### D26. Freeze Experiment comparability configuration on start — DECIDED (Q25: B)

When the Experiment's **first Match starts**, freeze its comparability-defining conditions (roster/builds, rules, seeds/derivation, seat/fixture plan). Changes to notes/annotations and administrative pause/cancel are still permitted. Do not silently edit prior results or combine differently defined experimental cohorts. Precise pause/cancel lifecycle remains open.

### D27. Quota/expiry-controlled optional diagnostics — DECIDED (Q26: B)

Detailed diagnostic logs have **Admin-configurable retention time and capacity quotas** with conservative self-hosted defaults. Durable Match-level outcome and reliability summaries and failure/Attempt records survive expiry or quota enforcement. The precise quota scope, quota-exceeded behavior, redaction and export/archival policy remain open.

## Verified v1 baseline (not v2 decisions)

- A Benchmark Run has a fixed participant roster, rotating seats, and 1–1,000 completed and durably recorded Matches.
- Its statistics are mean placement, first-place rate, and cumulative score relative to the table mean.
- Run history survives process restarts, but running Runs become interrupted and are not resumed automatically.
- Reproducible seed input and cross-Run ranking are explicitly excluded by the v1 design.
- MJAI, MCP, and Built-in Bots are existing participant kinds; Benchmark Mode does not admit Human players.
- Current Run identity and results belong to Participants, not to the Bot Token or displayed name.

## Optional extensions — NOT APPROVED

These remain candidates, not implementation mandates:

1. An optional central model registry UI (verified agent artifact identity is required for Experiment eligibility; a registry product UI is not).
2. Pre-generated wall/deal fixtures and bit-for-bit replay checks beyond the selected derived-seed/split-stream approach.
3. Model regression thresholds, rankings, and publication of evidence reports.
4. Offline decision evaluation and detailed per-decision traces, which are not selected for initial v2.
5. Dataset/replay export and reproducible experiment manifests; Web UI is the selected primary interface.
6. Parallel workload scheduling and resource quotas beyond the existing self-hosted architecture.
7. A generalized automatic recovery engine, beyond the agreed *boundary-only* resumption capability.

## Decision tree — Round 6 frontier

\`\`\`text
Benchmark v2
├── D1–D3 Research, additive v1, self-hosted                          [DECIDED]
├── D4,D10,D15,D21 Strict verified Runs, historic + new              [DECIDED]
│   └── Q27 Attestation of actual Bot executable/checkpoint         [FRONTIER]
│       └── Later: schema/evidence chain, eligibility test, legacy migration
├── D5,D11,D16,D22 Paired seeds, balanced seats, split RNG          [DECIDED]
│   └── Q28 Pairing block and shared fixture schedule                [FRONTIER]
│       └── Later: statistical sample units, derivation contract, RNG tests
├── D6,D12,D17,D23 Dual timing metrics + reliability evidence       [DECIDED]
│   └── Q29 Treatment of invalid/missing actions in stats            [FRONTIER]
│       └── Later: estimators, event clocks, statistical confidence
├── D7,D13,D18,D24 Admin Resume, 3 Attempts, same seed/roster       [DECIDED]
│   └── Q30 If clean Bot reset cannot be verified                   [FRONTIER]
│       └── Later: reset mechanism, transactions, exhausted status
├── D8,D9,D14,D19,D25 Admin SSE view, bot-focused 3D + hand         [DECIDED]
│   ├── Q31 SSE snapshot vs delta update policy                     [FRONTIER]
│   │   └── Later: authorization, slow clients, bounded revisions
│   └── Q34 Focus transition and accessibility                      [FRONTIER]
│       └── Later: scoreboard Seat correspondence, visual regression tests
└── D10,D20,D26,D27 Sequential frozen Experiment, log quotas       [DECIDED]
    ├── Q32 Pausing/canceling active Experiment                      [FRONTIER]
    │   └── Later: durable queue transitions and Run status model
    └── Q33 Quota exhaustion behavior                                [FRONTIER]
        └── Later: retention policy defaults, export rules
\`\`\`

### Round 6 questions — awaiting owner

- **Q27, trust boundary for Bot artifact verification:** A — allow only Bots under a locally managed runner that can verify the executable/checkpoint used; B — support those local runners plus external MJAI/MCP agents **only if** trusted attestation of the actually running artifact is available; C — accept externally supplied hashes as verified. **Recommendation B** as a compatibility goal, but reject external Bots until a real trust chain is present; signed self-assertion alone is not independent verification.
- **Q28, unit of Paired Evaluation:** A — repeat a single shared Match seed and rotate the Seats; B — use a seat-balanced 3/4-Match *block* with a fixed set of distinct derived Match seeds, repeating the same plan across compared versions; C — permit arbitrary Matches under a shared root seed. **Recommendation B:** a verifiable comparison block without assuming identical bot-dependent draws.
- **Q29, failures in response statistics:** A — replace timeouts/invalid actions with a penalty duration inside an average; B — report latency distributions for observed responses/accepted actions and **separate** invalid action, timeout, missing-response and disconnect rates with explicit denominators; C — discard failures from statistics. **Recommendation B:** do not imply an unsuccessful decision had a legal-action latency.
- **Q30, insufficient Bot reset evidence before retry:** A — block resumption until matching builds/checkpoints and a clean Match-level reset are verified; B — allow an Admin override but mark the result unpaired/noncomparable; C — trust reconnection as proof of reset. **Recommendation A:** preserve strict Experiment evidence without depending on guesswork.
- **Q31, SSE update semantics:** A — full private snapshot for every revision; B — snapshot on connect/reconnect plus bounded coalesced latest-state snapshots with revision IDs and polling fallback; C — incremental deltas requiring client reconstruction. **Recommendation B:** fewer staleness and backpressure problems, plus explicit authorization before sensitive sends.
- **Q32, Pause/Cancel boundary:** A — apply pause/cancel only **after the current Match is durably finalized**; B — abort immediately; C — wait until the current entire Run finishes. **Recommendation A:** preserves Match result integrity and avoids modifying v1 lifecycle.
- **Q33, optional diagnostic log quota exceeded:** A — stop the Experiment to preserve complete traces; B — stop optional detail collection, retain mandatory Match summaries and failure evidence, and visibly mark diagnostics truncated; C — silently discard the oldest details. **Recommendation B:** mandatory results remain durable even when optional detail storage fills.
- **Q34, Admin observation transition:** A — instantly switch the selected bot's table orientation; B — short smooth transition with reduced-motion support, maintaining authoritative Seat/score association; C — automatically cycle between Bots. **Recommendation B:** user-friendly without compromising accessibility or correct score-to-Seat labels.

### Deferred decisions

After Round 6, define artifact attestation and model-reset proof; exact RNG and seed-derivation versioning; paired statistical unit, 3p-vs-4p comparability and confidence intervals; accepted-action timing denominators; retry idempotency and terminal status; SSE auth/revisions/slow viewer behavior; Experiment linking/removal, queue status and UI; diagnostics limits/redaction/export; API/schema upgrades and runtime compatibility; and actual pixel-level acceptance checks for the already reported ordinary-Match score-to-Seat misassignment.

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
