# Benchmark Mode v2 — Grill with Docs (working design)

Date: 2026-10-14
Status: **Design interview in progress — Round 1 decided; Round 2 unanswered**
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

Preserve the semantics of existing v1 Benchmark Runs and existing API consumers. Additive versioned capabilities are preferable to silently changing v1's contract. This does **not** mandate a new `Experiment` entity; its necessity and shape are open in Round 2.

### D3. Primary deployment — DECIDED

**Choice:** Self-hosted.

Prioritize one operator controlling a self-hosted instance, rather than building a hosted leaderboard, multi-tenant service, or distributed cluster. Public hosting and collaboration are not goals for the initial v2 scope.

## Verified v1 baseline (not v2 decisions)

- A Benchmark Run has a fixed participant roster, rotating seats, and 1–1,000 completed and durably recorded Matches.
- Its statistics are mean placement, first-place rate, and cumulative score relative to the table mean.
- Run history survives process restarts, but running Runs become interrupted and are not resumed automatically.
- Reproducible seed input and cross-Run ranking are explicitly excluded by the v1 design.
- MJAI, MCP, and Built-in Bots are existing participant kinds; Benchmark Mode does not admit Human players.
- Current Run identity and results belong to Participants, not to the Bot Token or displayed name.

## Candidate ideas — NOT APPROVED

1. Group Runs into comparative experiments with immutable configurations and model provenance.
2. Record deterministic setup / version hashes and support controlled replay of random deals; distinguish *repeatability* from *fair comparisons* across different agent actions.
3. Paired comparisons across controlled initial conditions, with confidence intervals and explicit limits of inference.
4. Record invalid action, response latency, timeout, disconnect, and persistence-failure evidence separately from game outcome.
5. Compare named model versions or checkpoints and identify regressions.
6. Offline decision evaluation on the same Player View states, explicitly different from full-match counterfactual claims.
7. Export machine-readable experiment manifests, Replay-derived examples and results with provenance.
8. A bounded crash-resume mechanism with unambiguous policy for partially played Matches.
9. Optional local command-line/API automation while retaining the Admin UI.

## Decision tree

```
Benchmark v2
├── D1: Research platform                         [DECIDED]
├── D2: Additive/backward-compatible evolution    [DECIDED]
└── D3: Self-hosted first                          [DECIDED]
    ├── Q4: Unit of experimental comparison        [FRONTIER]
    │   └── Later: configuration and Run linkage, IDs, lifecycle, schema
    ├── Q5: Reproducibility/fairness level          [FRONTIER]
    │   └── Later: RNG streams, seed ownership, repeat controls, validation
    ├── Q6: Which evidence is primary?             [FRONTIER]
    │   └── Later: data collection, statistics, exports and visualizations
    ├── Q7: Error/recovery policy                  [FRONTIER]
    │   └── Later: idempotency, restart, retries, outcome provenance
    └── Q8: Primary research interface            [FRONTIER]
        └── Later: manifest/API schema, operator UX, automation
```

### Round 2 questions — awaiting owner

- **Q4, comparison unit:** Keep standalone Runs only; group multiple Runs into a new Experiment; or define a configurable automated experiment workflow?
- **Q5, fairness:** Record provenance only; fixed-seed deterministic repeatability; or paired/common-random-condition comparisons with fairness safeguards?
- **Q6, evidence:** Outcome statistics first; outcome plus runtime/reliability metrics; or full outcome + decision-level analysis in the first delivery?
- **Q7, faults:** Preserve fail-closed interruption with explicit manual rerun; allow only verified boundary-level resumption; or automatically recover and resume whenever safe?
- **Q8, interface:** Admin UI first; headless manifest/API first; or both as first-class frontends to the same evaluation model?

Questions that depend on these decisions (statistical methodology, paired-deal design, config immutability, maximum workload, model identity, storage/exports, concurrency, acceptance tests) are deferred to later rounds.

## Decision discipline

- Do not implement v2 features, run migrations, publish releases, or merge until all relevant decisions are settled and the owner confirms shared understanding.
- Update `CONTEXT.md` only with approved domain vocabulary; keep implementation options in this working document.
- Create an ADR only for a decided, consequential and non-obvious trade-off that would be costly to reverse.
- Preserve original v1 documentation unchanged; distinguish implemented facts, owner decisions and unapproved suggestions.
