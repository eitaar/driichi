# Benchmark Mode v2 — Grill with Docs (working design)

Date: 2026-10-14
Status: **Design interview in progress — Rounds 1–14 decided; Round 15 unanswered**
Owner: eitaar
Scope: research-oriented improvements to driichi's Benchmark Mode, not implementation approval.

## Sources and invariants

- Existing vocabulary: `CONTEXT.md`.
- Shipped v1 specification: `docs/superpowers/specs/2026-09-29-benchmark-mode-design.md`.
- Existing Run state: `crates/double_riichi_core/src/room.rs`.
- Persistent Run records and statistics: `crates/double_riichi_server/src/storage.rs`, `migrations/0006_benchmark.sql`.
- Existing UI: `frontend/src/benchmark.tsx`.

Keep v1's currently shipped behavior as the compatibility baseline. A design idea below is **not** a requirement or implemented feature until the owner decides it.

## Owner clarification — Core Benchmark vs Experiment (2026-10-14)

**Benchmark Mode itself is first and foremost an environment for continuous, sequential Mahjong competition among explicitly selected MJAI, MCP, and Built-in Bots**. Admins select the exact 3p *or* 4p game mode and roster, configure a Run, then watch the engine repeatedly start the next Match after the previous Match is durably recorded, **until the Run's configured target or a stop/failure condition is reached** (per the current v1 1–1,000 Match contract). The Admin dashboard must offer **ongoing statistics, per-Match results and a live table preview**. Live observation is read-only, shows all present hands only to the Admin, and must not change game progression. The existing v1 Run workflow already includes these concepts.

**An Experiment is an optional additional layer** for controlled A/B evaluations, provenance, paired statistics and grouping/sequencing multiple Runs. An ordinary Benchmark Run must not require an Experiment, A/B candidates, a research seed, or complete paired Blocks to keep Bot-vs-Bot Matches running with dashboard/live view. **However, Round 12 Q99 B later requires v2 capability compliance even for standalone Benchmark v2 Bots; v1 legacy migration remains a separate open decision.** All previously selected statistical/reproducibility requirements are **Experiment-scoped**, unless deliberately and separately approved as additive non-breaking features for ordinary Runs. No plan to replace Benchmark Mode with an A/B-only comparison product was approved.

The user confirmed that **3p Benchmark runs only 3p and 4p Benchmark runs only 4p**; never auto-run both. The phrase "continuous" means automatic Match-to-Match progression in an active Run, **not automatically endless runs or infinite autonomous scheduling**. Whether to add an optional unattended follow-on Run workflow is an open Round 12 decision; current v1 bounded targets stay unchanged.

## New UI design requirement and Q99 migration conflict — OWNER NOTES (2026-10-14)

**Requested:** Remove obscure/unnecessary pseudo-poetic and marketing UI copy ("謎ポエム") and redesign the user interface using a modern component library (e.g., shadcn/ui). The exact library and extent of the copy audit have **not yet been selected**. Keep functional instructions, security/privacy/accessibility text and errors. The existing React 19 + Vite 8 + TypeScript frontend uses bespoke CSS and has no Tailwind/shadcn dependency in its inspected package manifest; its Admin/Benchmark views embed a Three.js table that need not be rewritten. Official Vite integration guidance: https://ui.shadcn.com/docs/installation/vite .

**Recommendation awaiting Round 13 answer:** Implement the shared UI foundation and migrate the current **Run-first Benchmark dashboard** before coding new v2 Experiment pages, using a separate approved UI PR rather than combining a sweeping style refactor with backend v2 logic. No frontend code changes are authorized here.

**Q99 B supersedes the previous v2 adapter opt-in assumption:** All MJAI/MCP/Built-in Bot Participants admitted to *Benchmark v2*, including ordinary standalone Runs, must support its v2 capability version. They still **do not need an A/B Experiment** to compete and use the Run dashboard. Earlier commitments to preserve deployed v1 compatibility conflict with applying this change directly to existing v1 endpoints; the migration/release boundary is **unresolved**, not permission to break legacy clients. No independent runtime attestation is introduced.

## Round 13 decision impact / supersession ledger (2026-10-14)

- **Q110 B overrides D2** for legacy **Benchmark Bot admission compatibility** on the planned v2 release. Do not extend that decision to unrelated Room/game protocols without explicit owner scope. Old Bot integrations need migration; no breaking release has been executed.
- **Q114 B changes the UI roll-out** from previously recommended visible per-section releases to **one user-visible whole-app UI cutover**. Q112 A still sets the **internal** implementation order (UI foundation + existing Benchmark dashboard before v2 feature screens). Internal incremental branches/reviews are permitted.
- **Q113 A** selects shadcn/ui with Tailwind/tokens, not a Three.js or engine replacement.
- **Q117 A / Q125 B** confirms one-page 3D table + live stats, and **one 3D Mahjong table expandable to fullscreen** (the intended meaning of "3卓"). There is **no three-table simultaneous view** and no extra Match concurrency.
- **Q123 A** demands a separately attributed real-rendered regression investigation/bug fix for the previously observed center-score/Seat mismatch; the issue is **not known fixed**.
- All product and UI code changes, CI and merge remain forbidden until future owner authorization.

## Benchmark game-mode invariant — CONFIRMED BY OWNER (2026-10-14)

A Benchmark Room/Run/Experiment evaluation is configured for **one specific game mode**. A `4p` Benchmark plays **only four-player Mahjong** with four seats; a `3p` Benchmark plays **only three-player Mahjong** with three seats. **Creating or running one NEVER automatically starts or also executes the other**. A/B candidates, fixed reference Bots, seed fixtures, Paired Blocks, metrics and the target count all stay within the chosen mode. Round 9's 6-Match (3p) and 8-Match (4p) comparison-block sizes describe **alternative modes**, not combined workloads. The existing three-player MJAI prohibition remains in force.

These constraints hold for the existing v1 behavior and must not be weakened by v2. **Round 10 confirms exact ruleset/game-mode equality**: red-east and red-half are different fixtures even within the same player count. An Experiment may not automatically run or mix 3p and 4p, nor east and half. (D75)

## Decisions — Round 1 (2026-10-14)

### D1. Product goal — DECIDED

**Choice:** Preserve a general continuous Bot-vs-Bot Benchmark environment with dashboard/live observation, and extend it with optional research-grade Experiments.

The owner explicitly clarified in Round 11 that the essential Benchmark is sequential automated Matches between selected MJAI/MCP/Built-in Bots with statistics and an Admin live table preview. A/B pairing and Bootstrap statistics add a separate optional research workflow, **not a replacement or mandatory prerequisite** for ordinary benchmarking. This refines the Round 1 'research platform' goal without undoing any A/B design decisions.

### D2. Benchmark compatibility strategy — SUPERSEDED IN PART (Q99 B / Q110 B)

**Original:** additive Benchmark v2 preserving previous v1 Bot admission/wire compatibility. **Explicit Round 13 override:** Q110 B chooses a **deliberately breaking new Benchmark Bot protocol cutover**; existing Benchmark Bot clients must upgrade and v1-only admissions will not remain a guaranteed parallel option after the new release. This applies to **Benchmark bot admission**, not silently to unrelated Room/MJAI/MCP services. Historical recorded Runs/Replay/results must be preserved; all actual cutover work remains unauthorised until separate implementation/release signoff. Migration route inventory, error contracts and rollout plan remain open.

### D3. Primary deployment — DECIDED

**Choice:** Self-hosted.

Prioritize one operator controlling a self-hosted instance, rather than building a hosted leaderboard, multi-tenant service, or distributed cluster. Public hosting and collaboration are not goals for the initial v2 scope.

## Decisions — Round 2 (2026-10-14)

### D4. Experiment groups Benchmark Runs — DECIDED (Q4: B)

Introduce **Experiment** as an **optional** research-level concept for organizing and comparing multiple existing Benchmark Runs. Preserve the independent, continuously progressing Bot-vs-Bot Run, Run identity, fixed-roster semantics, recorded Match data, dashboard/live preview and existing v1 API behavior. Round 3 decided that an Experiment supports both adding previously completed Runs and creating/running new Runs through the Admin interface. This does not authorize a general workflow pipeline.

### D5. Paired Evaluation — DECIDED IN PRINCIPLE (Q5: C)

Support comparisons using controlled shared random conditions to reduce the effect of chance. This selects **paired evaluation**, not a specific seeding algorithm. Round 3 requires recorded fixed seeds and equitable rotation of Seats. Round 4 allows the Admin to supply a root seed or let the server generate one; both record the actual chosen seed. Exact expansion/stream ownership, pair identification, and interpretation of diverging decisions remain open. A common seed alone does not establish equivalent observed game states.

### D6. Outcome + reliability + statistical evidence — DECIDED (Q6: B)

Report and assess game outcomes, runtime/reliability evidence, and statistical uncertainty. Round 3 requires Match-level summary data and optional diagnostic logs. Round 4 requires server-observed latency for all eligible bot decisions and optionally collects agent-reported inference time **separately**. Round 5 selected both first-response and accepted-legal-action latency, separate invalid/timeout evidence, and quota/expiry-managed diagnostics. Exact clock/framing semantics, failure denominators, and confidence-interval methods remain open. This does not select full decision-level trace collection for the initial v2 release.

### D7. Recovery at Match boundaries — DECIDED IN PRINCIPLE (Q7: B)

Support resuming eligible interrupted v2 experiments/runs **only at a verified Match boundary**, preserving already-durable outcomes. No replay-based mid-Match continuation is authorized. Round 3 requires an explicit Admin action to resume after interruption (no automatic resume). Round 4 requires recording failed attempts separately and allowing a new Match attempt with the original planned seed and fixed roster after Admin Resume. Round 5 bounds each scheduled Match to three Attempts including the original (two retries); eligibility checks, Bot state reset verification, and idempotency conditions remain open. Keep v1's existing fail-closed interrupted-Run behavior unchanged.

### D8. Web UI is the primary interface — DECIDED (Q8: A)

Make the Admin Web UI the primary mechanism for starting/monitoring ordinary Bot-vs-Bot Benchmark Runs **and**, optionally, configuring/comparing v2 Experiments. The Run dashboard must keep live progress, match-level and accumulated statistics, and read-only live table preview as first-class experiences. Do not require Experiment creation, a CLI or headless manifest for the ordinary Benchmark workflow; existing APIs remain compatible.

### D9. Admin live-observation focus — DECIDED USER REQUIREMENT

During a running Benchmark, authenticated Admins can inspect **all bots' concealed hands** and switch which player's hand is the observation focus. Focus changes are strictly read-only: they neither impersonate a Participant nor alter a seat, Controller, legal-action visibility, RNG state, or Run progress.

**Existing implementation:** `Audience::BenchmarkAdmin` already exposes all current hands to an authorized Admin. `frontend/src/benchmark.tsx` currently polls the Admin live endpoint at one-second intervals and renders `ThreeTable` with `surface="replay"`; there is no explicit focus selector in the inspected implementation. Round 3 decided that selecting a bot both changes the 3D table's viewing perspective and enlarges that bot's hand. The intended choice is a read-only presentation transformation (no privileged Player View impersonation). Round 4 requires selecting the Participant identity, not the seat, so the view tracks that bot across seat rotations. Round 5 selected authenticated SSE updates with reconnect snapshots and ongoing authorization checks plus a polling fallback; view defaults, camera transitions and event granularity remain open.

## Decisions — Round 3 (2026-10-14)

### D10. Existing and newly created Runs in an Experiment — DECIDED (Q9: C)

An **optional** Experiment supports both attaching previously completed Benchmark Runs for retrospective comparison and creating/running new Benchmark Runs from its Admin Web UI. The normal dashboard and standalone Match-loop do not require Experiment membership. A Run remains independently addressable and its committed Match results must not be silently rewritten when linked. Round 4 restricts admission to Runs whose relevant experiment conditions can be verified; historical Runs without sufficient evidence are not retroactively treated as reproducible or admitted. Round 6 adopts token-authenticated Bot self-declarations as sufficient artifact identity under the owner's trust policy. Round 7 binds an immutable model declaration to each Participant for the Run and requires matching declarations upon resume; linking reversibility remains open.

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

Only a Benchmark Run whose **relevant experiment conditions are fully verifiable** can be admitted to an Experiment, whether it was created by that Experiment or previously completed. In particular, missing seed/agent provenance must not be fabricated or backfilled. Authenticated Bot self-declarations are accepted within the selected trust model, but are not independently measured execution evidence. Existing v1 history is **preserved and still accessible outside Experiment**, regardless of eligibility. Verifiability is a product eligibility gate, not a claim of mathematical equality of different bot trajectories. What evidence counts as sufficient verification is the next decision.

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

### D21. Trusted agent build and model artifact declarations — DECIDED (Q20: B, refined by Q27: C)

Experiment eligibility requires recorded game conditions, engine revision, effective seed, and an identity declaration for each Bot executable/build and model checkpoint. Under the operator's full-trust Bot Token policy selected in Round 6, **an authenticated Bot's own declared hashes are sufficient**. A Participant ID, display name, or token alone is not a checkpoint identity. Do not fabricate missing legacy evidence.

The server records the issuer of the claim and the claimed hash. **A trusted declaration does not independently prove that the claimed executable actually ran.** Do not add independent remote attestation, measured local launchers, binary signatures, or other heavyweight proof requirements. This resolves the previously undecided artifact-attestation boundary.

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

Detailed diagnostic logs have **Admin-configurable retention time and capacity quotas** with conservative self-hosted defaults. Durable Match-level outcome and reliability summaries and failure/Attempt records survive expiry or quota enforcement. Round 7 selects a global self-hosted diagnostics cap with optional tighter per-Experiment caps. Quota exhaustion drops optional details with visible truncation; specific cap defaults, redaction, and export policy remain open.

## Decisions — Round 6 (2026-10-14)

### D28. Admin-issued Bot Token implies trusted artifact declarations — DECIDED (Q27: C)

The operator issues Bot Tokens only to trusted parties. An authenticated MJAI/MCP Bot's self-declared code/build hash and checkpoint hash are **sufficient for Experiment admission** under this chosen local trust policy. Do not create a remote attestation or mandatory launcher layer. Because one Token may authenticate several Participants, the declared metadata must still identify the corresponding Participant. **This is a claim from a trusted operator, not independent proof of which model was executed.** The phrase "Verified Benchmark Run" means *accepted under the operator's trust policy and complete recorded conditions*, not independently measured binaries.

### D29. Balanced seed/seat comparison blocks — DECIDED (Q28: B)

Each **candidate leg** of a comparison block has 3 Matches for a 3p Benchmark or 4 Matches for a 4p Benchmark, with the Candidate visiting each Seat once. Round 9 requires **two legs** (A and B) against the same fixed Reference Bots, yielding 6 Matches for a 3p comparison block or 8 Matches for 4p. Use the matching planned seed/Seat fixture correspondence in each leg; one selected mode per Experiment, never automatic execution of both. Divergent Bot decisions still prevent claims of identical full game trajectories. Round 7 selects one A/B candidate slot against an unchanged roster of 2/3 reference Bots; only fully completed paired blocks contribute to paired estimates, while partial Match results remain descriptive. Effect-size methodology remains open.

### D30. Valid-response timing and failures are separate — DECIDED (Q29: B)

Summarize only actual observed first responses and accepted legal actions in timing distributions. Separately show invalid-action rate, timeout rate, missing-response rate and disconnection rate, with explicit denominators and missingness; never convert failed decisions into fictional successful timing observations.

### D31. Strict Match reset gate before retries — DECIDED (Q30: A)

Before Admin resumes an interrupted scheduled Match, check the same declared Bot build/checkpoint identities and confirm a **fresh, clean Match-level Bot state**. Reconnection alone is not clean-reset evidence. Round 7 requires an explicit authenticated Bot clean-Match reset acknowledgement for the same declared checkpoint before retries; the handshake shape and idempotent processing remain open. Existing maximum of 3 Attempts remains.

### D32. SSE current-state snapshots — DECIDED (Q31: B)

An authenticated Admin receives a complete current snapshot on SSE connect or reconnect. Later updates contain bounded/coalesced **latest-state snapshots** with revisions, rather than an assumption that the viewer receives every intermediate action or must reconstruct all state from deltas. Polling is retained as a fallback. Admin authorization, revocation and private hand safety remain mandatory.

### D33. Pause/Cancel after persisted current Match — DECIDED (Q32: A)

Allow the active Match to finish and save its result before fulfilling an Experiment Pause or Cancel. Never abort a Match simply because the Admin presses these controls. Round 7 makes Pause resumable with immutable scheduled Runs preserved, and Cancel terminal with committed results retained and partial blocks labeled; exact API/state transitions remain open.

### D34. Quota exhaustion truncates optional diagnostics — DECIDED (Q33: B)

When optional detailed diagnostic storage reaches its quota, stop collecting **only optional details**, keep durable Match statistics and failure/Attempt evidence, and display that diagnostic data is incomplete. Do not silently drop durable results or stop scoring merely for optional log exhaustion.

### D35. Smooth Admin focus and reduced-motion handling — DECIDED (Q34: B)

Selecting a different Bot animates a short smooth 3D perspective change while showing that Bot's enlarged hand. Honor Reduced Motion by switching without animation. Observation Focus follows the **Participant identity** across rotated Seats. The center scoreboard must continue to map each score to the correct Seat throughout the transition; the reported normal-Match wrong-seat visual defect is still unverified as fixed.

## Decisions — Round 7 (2026-10-14)

### D36. Per-Participant immutable Bot identity declaration — DECIDED (Q35: B)

Each software **Participant**, not the shared Bot Token, declares its own executable/build hash and model checkpoint identifier **before admission to a Run**. Persist the claimed identity along with the authenticated declarant/issuer and the Run participation, and require **the same declaration at Resume**. The declaration becomes immutable for that Run and cannot be silently changed by reusing a Bot Token or changing Display Name. The operator explicitly trusts authenticated declarations; this does **not** require independent executable measurements or cryptographic runtime attestation (D28). The metadata transport and failure behavior on a changed declaration remain open.

### D37. One A/B candidate slot against fixed reference Bots — DECIDED (Q36: A)

The default controlled comparison contrasts **Candidate A and Candidate B** one at a time in the same evaluation slot, with the **other two Bots in 3p or three Bots in 4p unchanged** as a reference roster. Compare A's and B's results using matched seed/Seat fixture blocks, not arbitrary mixtures of changing opponents. A and B may correspond to separate Participant identities and separate Runs; do not conflate a Participant ID with a model version. A general multi-changing-roster study is not selected for the initial v2 default. Round 8 selects mean placement delta as primary outcome, with centered relative score and first-place-rate differences secondary.

### D38. Complete blocks only for paired effect estimates — DECIDED (Q37: A)

A Paired Block contributes to **paired A/B effect statistics only when its required scheduled Matches are all completed and durably recorded for the compared versions**. If any required Match is missing, the block is incomplete and **excluded entirely from paired-effect estimation**; already completed constituent Matches still appear in descriptive Run statistics, with visible failure/missingness evidence. Never silently replace the missing fixture with a new seed. Round 8 permits fully verified but non-paired historical Runs for clearly separated descriptive-only comparisons, never paired effect/CIs.

### D39. Versioned RNG derivation and engine contract — DECIDED (Q38: B)

Store the **Root Seed**, deterministically derived per-Match seed/fixture schedule, a **versioned seed derivation algorithm**, labeled RNG domains/streams, and the **game engine and RNG version** needed to interpret or reproduce an Experiment. An arbitrary matching numeric seed is insufficient to assert compatibility across different RNG or rules-engine behavior. Round 8 requires exact recorded engine/RNG/seed-derivation version matches for Paired Evaluation, not inferred cross-version equivalence. PRNG/hash selection and canonical encodings remain open; v1 history does not acquire missing fields.

### D40. Controller-dispatch-started server-observed latency — DECIDED (Q39: A)

Start the server's Bot latency clock when an action request is **dispatched to the Bot Controller**, not when the Decision is initially created. Record separate elapsed times for the first received response and the first accepted complete legal Game Action. Network/protocol overhead is included in external Bot metrics; label synchronous Built-in Bot timings as server-observed rather than inference-only. Round 9 requires count, mean, median and p95 for both first-response and accepted-legal-action latency where observed, plus separately denominated error rates. Exact MJAI/MCP framing and retry/invalid response attribution remain open.

### D41. Explicit authenticated clean Match reset acknowledgement — DECIDED (Q40: A)

Before retrying a scheduled Match, each affected external Bot must explicitly **acknowledge a fresh Match-level reset under its authenticated token and matching declared build/checkpoint identity**. A reconnect alone does not count. As with artifact declarations, the operator trusts this acknowledgement; it is not independent proof of the Bot's internal memory. Internal Built-in Bot reset evidence and the specific handshake/correlation contract remain open. This is a lightweight protocol acknowledgement, not a mandatory process restart.

### D42. Exhausted scheduled Match halts the Experiment — DECIDED (Q41: A)

After all **three permitted Attempts** for one scheduled Match fail, halt Experiment progress and show that Match and Experiment as explicitly **Blocked/Failed**; do not skip the fixture, silently progress to further blocks, or fabricate a complete paired sample. Retain failed Attempts, earlier completed Matches, and their evidence. Changing immutable settings requires a **new Experiment** rather than reopening this exhausted schedule. Round 9 distinguishes recoverable `blocked` (pending prerequisites) from terminal `failed` (exhausted 3 Attempts or unrecoverable integrity error); `cancelled` is separate and terminal. The exact persisted transition schema remains open.

### D43. Configurable bounded SSE cadence with mandatory terminal state — DECIDED (Q42: B)

Coalesce intermediate Admin live state revisions into a **configurable capped update rate**. The stream may skip intermediate animation frames; it must still deliver an authoritative terminal/Match-completed view (or allow recovery by a fresh snapshot if connectivity fails), without hiding the final result. Admin SSE auth/expiry and polling fallback remain mandatory. Precise default cap, slow client policy, reconnect UI and retained revision budget remain open.

### D44. Pause preserves queue; Cancel terminates without deleting results — DECIDED (Q43: A)

On **Pause**, finish/persist the current Match, retain the immutable Experiment plan and its queued Runs, and allow explicit Admin resumption. On **Cancel**, finish/persist the current Match and transition to a terminal cancelled state; preserve committed results, failed-Attempt evidence, and clear labels for unfinished Paired Blocks. Neither operation rewrites the initial Experiment configuration, erases history or changes standalone v1 Run behavior. Round 8 requires idempotent reconciliation of uniquely persisted Match results and Replays after a crash; exact state-machine/transaction boundaries remain open.

### D45. Global diagnostic capacity with optional per-Experiment caps — DECIDED (Q44: B)

Enforce a **self-host-wide cap for optional detailed diagnostic storage**, with optional tighter caps assigned to individual Experiments. Preserve durability of Match summaries and failed-Attempt evidence outside the optional-detail quota. When the cap is reached, stop optional detail collection and mark trace gaps, rather than terminating successful game progress. Exact defaults, disk reservation for mandatory records, retention timing and redaction rules remain open.

## Decisions — Round 8 (2026-10-14)

### D46. Mean placement difference is the primary A/B effect — DECIDED (Q45: B)

Use the difference in **average finishing placement** for Candidate A versus Candidate B as the main playing-strength outcome. Display **centered final score relative to the Match's table mean** and **first-place rate** as secondary comparisons, in addition to raw Match outcomes. Lower placement number means stronger observed performance; the precise difference sign convention and weighting of Paired Blocks remain open. Never blend 3p and 4p directly (D57).

### D47. Paired-block bootstrap confidence intervals — DECIDED (Q46: B)

Resample **whole, complete corresponding Paired Blocks**, not independently sampled individual Match placements, when calculating uncertainty for the A/B difference. Display the number of complete paired blocks and clearly distinguish a confidence interval from a certainty guarantee. Never use incomplete blocks as independent matched observations; their finished constituent Matches remain available descriptively (D38). Round 9 chooses a 95% percentile bootstrap over whole completed A/B comparison blocks and suppresses numerical CIs below a still-undecided minimum count, while retaining the observed effect, count and insufficient-evidence warning. Bootstrap replicate count and statistical RNG policy remain open.

### D48. Fixed planned Paired Block target — DECIDED (Q47: A)

Admin selects a fixed target **number of complete Paired Blocks before the Experiment begins**. Do not adapt the sample size or automatically stop when a statistical-significance threshold is reached. Pauses, explicit cancellation and bounded failures may prevent the planned number from being completed; the system must show planned/completed/missing blocks without manufacturing data. Round 9 fixes 3 Matches per A or B candidate leg for 3p and 4 per candidate leg for 4p; the 1–1,000 completed Matches **per Run** v1 limit must remain. Splitting an Experiment's workload across multiple sequential Runs is a separate Round 10 decision.

### D49. Verified historic Runs are eligible for unpaired description — DECIDED (Q48: B)

A historical Run with **complete qualifying metadata under the selected trusted Bot declaration policy** may be attached to an Experiment even if its actual Seed/Seat plan is **not** compatible with the paired schedule. Such a Run must be **explicitly labeled descriptive / unpaired** and excluded from paired effect estimation and paired confidence intervals. Missing provenance is still disqualifying (D15); do not silently upgrade a descriptive Run to Paired.

### D50. Changed checkpoint prevents continued Run — DECIDED (Q49: A)

If an existing Participant reports a different immutable build/checkpoint identity after the Run has begun, **block further execution of that Run**. Preserve all previously durable results and provenance; the changed model must be evaluated in a **new Experiment**, not appended to the existing frozen comparison. Mere token reconnection or a new credential is not permission to change the claimed model. Round 9 permits credential rotation with a freshly Admin-issued valid Token only when Participant identity and frozen build/checkpoint declaration match. Revoked Tokens are never accepted; incompatible checkpoint changes still block the Run.

### D51. Exact engine and RNG contract match for pairing — DECIDED (Q50: A)

A paired comparison requires **exactly matching engine, RNG, and seed-derivation versions** across the compared conditions, as well as compatible declared rules and fixture/Seat schedule. Do not approve cross-version paired eligibility solely on matching Root Seed or a claim of similar behavior. A separately verified but non-pairable Run may be descriptive-only under D49.

### D52. Durable Match reconciliation, never double counting — DECIDED (Q51: B)

After a crash between Match persistence and Run progress bookkeeping, compare **authoritative durable Replay/Match results and scheduled Match identity**, reconcile the completed Match **at most once**, and continue with the correct sequence/count. Retry a Match only if there was genuinely no valid completed, durably recorded result. Do not overwrite the earlier result, create duplicate points, or leave a completed Match incorrectly counted as an interrupted Attempt. Round 9 counts Match completion only if the final results and valid Replay are both durable and refer to the **same Match ID**, then reconciles exactly once; the atomic persistence protocol remains open.

### D53. Stale-state indication and reconnect for Admin viewer — DECIDED (Q52: A)

When the authorized live SSE connection is lost, the Admin viewer must **visibly mark that the displayed view is stale/disconnected**, perform authenticated resubscription, and use the existing polling fallback if needed. Maintain read-only privacy boundaries and a last-known revision marker; do not falsely label previously received hands as current. Exact stale timeout, UI messaging and polling transition remain open.

### D54. Versioned JSON/CSV research export — DECIDED (Q53: B)

Provide export of **Experiment, Run and Match results, statistics, Paired Block IDs, relevant declared provenance and Seed schedule** as versioned JSON and CSV. This supports independent downstream analysis without requiring a headless-first authoring API or exporting every Decision/private future-wall detail. Round 9 chooses versioned canonical JSON plus normalized CSV tables joined by Experiment/Run/Match/Paired Block/Participant IDs, with paired/descriptive labels and no Bot Token or secrets; Replay references and packaging remain open.

### D55. Optional redacted protocol diagnostics — DECIDED (Q54: B)

Keep routine diagnostic collection **metadata-only**. Allow Admin to explicitly enable collection of **redacted MJAI/MCP protocol message content** for troubleshooting, always subject to global/per-Experiment diagnostic quotas and retention rules. Never persist credentials/secrets or unredacted sensitive bodies in this optional diagnostic store. Round 9 requires dropping any optional message body whose secrets cannot be reliably redacted, keeping only a non-secret failure/truncation marker; redaction allowlist and opt-in lifecycle remain open.

### D56. Freeze retrospective Experiment on first attach or explicit start — DECIDED (Q55: A)

For an Experiment created from historical Runs, freeze its intended comparison plan **at the first eligible historical Run attachment or explicit retrospective comparison start, whichever comes first**. Previously completed Matches cannot supply a future first-live-Match trigger. Round 9 confirms a **Draft selection of all historical Runs followed by one atomic first attachment** that freezes the retrospective comparison. Do not silently append, edit, or remove fixture-defining membership after this boundary.

### D57. Separate 3p and 4p comparison cohorts — DECIDED (Q56: B)

Treat 3-player and 4-player Mahjong as **distinct analysis cohorts**. Compute placement, score-based results, pairing and uncertainty separately; no combined mean placement/first-place-rate statistic, pooled paired test, or unsupported single shared leaderboard. Mixed-mode organization or filter UI is still permitted if it never misrepresents those results as statistically interchangeable. Mode-specific Run rules, including the existing 3p MJAI restriction, remain unchanged.

## Decisions — Round 9 (2026-10-14)

### D58. Full A/B comparison block = six 3p or eight 4p Matches — DECIDED (Q57: A)

A **complete comparison Paired Block** contains exactly two candidate legs within **one selected Benchmark game mode**, using the same seed/Seat fixture plan and fixed reference-opponent roster:

- **3p Benchmark:** Candidate A plays three Matches, visiting each of the three Seats; Candidate B plays the corresponding three Matches. **6 Matches total** in the block, **3 per candidate Run**.
- **4p Benchmark:** Candidate A plays four Matches, visiting each of the four Seats; Candidate B plays the corresponding four Matches. **8 Matches total** in the block, **4 per candidate Run**.

The operator's explicit invariant: creating a 3p Benchmark never executes 4p, and creating 4p never executes 3p, including automatic Experiment scheduling. Do not mix 3p and 4p fixture blocks, results or rankings; the 6/8 numbers describe separate alternatives, not aggregate workloads. All required Matches in **both legs** must be durably complete before the block contributes to paired statistics.

**Compatibility boundary:** v1 Benchmark Run targets remain 1–1,000 completed Matches *per Run*. A single A/B pair of Runs fits up to 333 complete 3p comparison blocks (999 Matches per candidate Run) or 250 complete 4p blocks (1,000 Matches per candidate Run). Round 10 authorizes splitting a larger fixed Experiment into multiple **sequential candidate Runs**, each no more than 1,000 completed Matches, while preserving a single Experiment-level paired fixture schedule. No v1 Run-cap change (D70).

### D59. 95% percentile block bootstrap — DECIDED (Q58: A)

Estimate uncertainty for the **paired A/B mean-placement delta** with a **95% percentile Bootstrap confidence interval**, sampling *whole complete comparison blocks with replacement*. Record analysis method/version, number of blocks and the effective statistical random seed to support repeatability; Round 10 fixes 10,000 whole-block bootstrap replicates using a recorded analysis-specific seed distinct from gameplay randomness; precise estimator/quantile implementation and treatment of secondary metrics remain open (D74). CI ≠ proof that one model is superior.

### D60. Insufficient-evidence CI suppression — DECIDED (Q59: B)

When fewer than a **minimum supported number of complete Paired Blocks** is available, show the observed mean-placement effect, number of complete Blocks and an **insufficient-evidence** label, but **do not display a numeric Bootstrap confidence interval**. Do not withhold the actual completed Match results. Round 10 sets **20 complete A/B comparison blocks** as the operational floor for showing a numeric 95% bootstrap interval. This threshold does **not** ensure statistical power or accuracy; smaller datasets still show observed effect/count and an insufficient-evidence warning (D72).

### D61. Draft selection + atomic retrospective first attachment — DECIDED (Q60: A)

For a retrospective-only Experiment, the Admin first assembles **all** intended eligible historical Runs in a provisional Draft selection. The initial registration performs **one atomic attachment of that complete set** and freezes the comparison plan according to D56. Validate all Runs for metadata eligibility, chosen game mode and compatible grouping rules before committing. Failed validation must not leave a partially attached/frozen Experiment. Later descriptive annotations remain editable, but fixture-defining Run membership is immutable.

### D62. Descriptive-only Runs have their own results section — DECIDED (Q61: B)

A fully verified but unpaired historical Run may appear under a clearly identified **Descriptive-only** section of its Experiment. Do not merge its results into paired A/B estimates, paired Bootstrap CI, or a common paired leaderboard and do not label it paired. Its own results and comparison metadata remain accessible.

### D63. Credential rotation may preserve Participant + model identity — DECIDED (Q62: A)

An external Bot may reauthenticate with a **new valid Admin-issued Bot Token** if it is still the **same Participant** and presents a declaration matching that Run's immutable executable/build/checkpoint identity. A revoked/expired previous credential never remains valid. Changing credentials does not grant permission to alter the model version, skip reset confirmation, or change seats. Round 10 chooses optional versioned v2-only metadata/reset extensions for MJAI/MCP; the mapping from a new Token to an existing authorized Participant and wire schema still require validation (D77).

### D64. Built-in Bot reset is server-managed and recorded — DECIDED (Q63: A)

Before a Match retry, the server **reinitializes each relevant Built-in Bot's Match state and records a reset result**, rather than inventing a Bot Token or blindly assuming reset. External Bots continue to provide explicit token-authenticated clean-Match reset acknowledgements. The exact reset hook/state boundary and failure handling remain open, without authorization to change general v1 Built-in Bot behavior.

### D65. Both matching Replay and results are required for completion — DECIDED (Q64: B)

A scheduled Match becomes *completed* only when **both its final results and valid Replay are durable and agree on the same Match ID** (and scheduled fixture identity). Reconcile idempotently after crashes before advancing the Run counter or scheduling the next Match. If either authoritative durable artifact is missing/inconsistent, never mark completion just because an HTTP response or UI counter advanced. This preserves the shipped v1 completion invariant; exact SQLite/filesystem transaction/commit ordering remains open.

### D66. Blocked (recoverable) vs Failed (terminal) — DECIDED (Q65: A)

Use distinct Experiment states: **Blocked** means progress cannot continue because a **potentially recoverable prerequisite** is missing (e.g., waiting on a trusted Bot, valid credentials or reset acknowledgement). Admin may remedy it and explicitly Resume if eligible. **Failed** is terminal for exhausted three Attempts or unrecoverable integrity failure; do not silently restart the same frozen schedule after terminal failure. **Cancelled** is another distinct terminal state that retains committed results. Exact persistence and queue transition conditions remain open.

### D67. Per-Match latency count, mean, median and p95 — DECIDED (Q66: B)

Store and report **sample count, arithmetic mean, median, and p95** of genuinely measured server-observed first-response durations and accepted-valid-Game-Action durations **separately** for each Match. Also preserve denominated invalid-action, timeout, missing-response and disconnect statistics. Missing accepted-action timings must not become zeros or fabricated timeout penalties. Specific quantile convention and whether per-Decision raw samples are retained by default remain open.

### D68. Drop unredactable optional payloads; retain marker — DECIDED (Q67: A)

If a debug MJAI/MCP message body cannot be safely redacted, **discard the optional body** and keep only a non-secret **redaction-failure/truncation marker** with applicable metadata. Do not persist secret-bearing traffic or stop the Experiment; mandatory Match and failure records remain durable under quota/expiry policy.

### D69. Versioned canonical JSON + normalized CSV export — DECIDED (Q68: A)

Research export consists of **versioned canonical JSON** and **multiple normalized CSV tables**, keyed across Experiment/Run/Match/Paired Block/Participant identities. Include all permitted results, outcome aggregates, declared provenance and seed/fixture metadata with explicit **paired vs descriptive-only** labels. Do not export Bot Tokens, credentials, or private future-wall states. References to authorized Replays may be identifiers/links, not automatic dumps of unrevealed wall information. The exact file-manifest and compatible schema versioning rules remain open.

## Decisions — Round 10 (2026-10-14)

### D70. Split large Experiments across multiple sequential capped Runs — DECIDED (Q69: B)

A fixed Experiment may exceed the v1 **1–1,000 completed Matches per Run** constraint by creating multiple sequential **Benchmark Runs**, each targeting at most 1,000 completed Matches. Preserve one immutable Experiment plan and its Paired Block/fixture identity across Run boundaries, tracking which candidate leg/Match belongs to which Run. Do **not** increase or silently change v1's limit. Never automatically combine or schedule 3p and 4p: all Runs in the same paired comparison use the one exact game mode selected by Admin. With single-candidate Runs, 333 complete 3p blocks fit into 999 Matches per candidate Run; 250 complete 4p blocks fit into 1,000. Chunk boundaries and A/B execution order remain open.

### D71. Identical Reference Bot identities/Seats across A and B fixtures — DECIDED (Q70: A)

For each paired fixture, the **same reference Participant identities occupy the same Seats** in both A and B candidate legs. Only the candidate identity/model changes in the corresponding candidate Seat. Across each mode-specific block, the candidate visits every Seat once, with a corresponding balanced reference-seat schedule. Do not independently reshuffle reference opponents between the paired A and B fixtures. Persist the seat map and fixture ID, not merely the Root Seed. The deterministic rotation method remains open.

### D72. At least 20 complete Blocks for numeric confidence intervals — DECIDED (Q71: B)

Display a numeric 95% percentile block Bootstrap confidence interval only with **20 or more completed full A/B Paired Blocks** (not 20 Matches or 20 single-candidate legs). Below 20, retain the observed effect, raw/descriptive results and exact complete-block count with an insufficient-evidence warning; omit the numeric CI. The threshold is a **product display floor, not a guarantee of precision, evidence of superiority, or statistically powered experimental design**. At or above 20, intervals may still be wide.

### D73. Positive effect means Candidate A advantage — DECIDED (Q72: A)

Define the primary signed playing-strength comparison as:

```text
A_advantage_in_placement = mean_placement(B) - mean_placement(A)
```

A **positive** result means A achieved a **lower (better) finishing placement**; a negative result favors B; zero indicates no observed difference in this metric. Calculate means from eligible fully paired blocks, not descriptive-only or incomplete fixtures; preserve both candidate means and sample sizes in reports. This is an observed effect, not a causal guarantee. Exact unweighted block-vs-match aggregation remains open.

### D74. Separate recorded analysis RNG and 10,000 Bootstrap replicates — DECIDED (Q73: B)

Statistical analysis uses a **recorded analysis-specific random Seed** and **10,000 whole-Paired-Block bootstrap replicates**. Never consume game RNG state for uncertainty analysis; keep analysis method/version and the input complete-block set available to reproduce results. The exact pseudorandom generator, seed encoding, quantile rounding, deterministic tie and invalid-sample conventions remain open. Display the 95% percentile interval only once D72's minimum is met.

### D75. Exact Benchmark game-mode match (including red-east/red-half) — DECIDED (Q74: A)

An Experiment's paired comparison runs **one exact game mode**, such as `3p-red-east`, `3p-red-half`, `4p-red-east`, or `4p-red-half`. Red East and Red Half do not share paired fixtures even with matching player counts. A 3p Benchmark runs only 3p Matches; a 4p Benchmark runs only 4p Matches; never auto-run the other player count. Preserve the existing 3p-MJAI exclusion, selected time control, reference roster and engine/RNG/derivation compatibility checks. Treat mismatched modes as not pair-eligible, not as another workload to append.

### D76. SQLite is authoritative for Experiment orchestration — DECIDED (Q75: A)

**SQLite is the source of truth** for Experiment identity/configuration/freeze boundary, selected exact game mode, planned Paired Blocks and fixtures, Run queue/links, statuses, Attempts and durable result pointers. In-memory workers or Room orchestration derive their execution view from persisted state and cannot independently invent progress. Existing v1 Run/Room behavior and APIs remain unchanged. After crash/restart, reconcile scheduled identity with persistent Replay and final results before any advancement. Exact transaction/queue state machine and filesystem+DB commit protocol remain open.

### D77. Versioned v2 Bot metadata/reset extension — DECIDED (Q76: A; Q99 B and Q110 B refine admission)

Versioned MJAI/MCP capability negotiation conveys trusted Participant-specific build/checkpoint declarations and correlated clean-reset acknowledgements. Every Bot in **Benchmark v2, including standalone Runs**, must implement its v2 capability (D101). Round 13 Q110 B selects **migration of existing Benchmark Bot entry points at a future authorised v2 release**, rather than perpetual v1 Benchmark protocol support; it intentionally supersedes the previous additive-only legacy Benchmark guarantee. Existing live routes are not changed by this design interview. Built-in Bots implement native capabilities, without pretending to need external Bot Tokens. Trust is operator-issued, not binary attestation.

### D78. Continuous authorization of Admin-only SSE projections — DECIDED (Q77: B)

Authenticate the Admin when opening an SSE stream and **recheck authorization/expiry during its lifetime before sending further concealed-hand snapshots**. Close/reject streams on logout, revocation or expiry; reconnects require fresh authorization. Ordinary Spectator/Bot Tokens never grant all-hands observations. Continue independent Admin-only authorization for polling fallback and Replay references. Exact checking cadence/notification source and slow-client policy remain open.

### D79. 4 Hz default capped Admin SSE refresh — DECIDED (Q78: B)

Default to **up to four snapshots per second** for an active authorized Benchmark Admin live viewer, with an Admin-configurable setting constrained by server-side bounds. Coalesce intermediate state revisions rather than promising delivery of every action; **terminal state is never dropped** and initial/reconnect snapshots remain authoritative. Enforce read-only semantics and honor stale/disconnected indicators. Precise maximum, backpressure and limit-on-client-count policy remain open.

### D80. 1 GiB / 7 days default optional diagnostic retention — DECIDED (Q79: B)

Start with **1 GiB of optional detailed diagnostics storage server-wide and a 7-day expiry**, both Admin-adjustable; optional per-Experiment caps must not exceed global constraints. These are defaults, **not a promise of sufficient physical disk capacity**. Durable Match result/Replay and failed-Attempt evidence are not eligible for optional-log expiry or quota truncation. On quota exhaustion, stop optional detail collection, mark gaps and preserve required records. Filesystem full and retention scheduling behavior remain open.

### D81. Rendered visual regressions for scoreboard/focus — DECIDED (Q80: A)

Require **real rendered screenshot/pixel-level regression tests plus seat-mapping logic tests** for 3p/4p table scenes: each Seat with distinguishable scores, player identity mapping, score changes, each viewer orientation, and ordinary live, Replay and Benchmark Admin observer views including focus animation and Reduced Motion. This directly addresses the user-observed center-score-at-wrong-Seat regression **without asserting that the bug is currently fixed or even currently reproducible**. Existing painter mock tests alone do not prove the final 3D canvas/Worker output. Baseline platform/threshold and fixture harness are still to be determined; actual test implementation is NOT authorized by this design decision.

### D82. Export authorized Replay references, not Replay payloads — DECIDED (Q81: A)

Research exports include stable **authorized Replay IDs or references** associated with Match/result identities, but **do not embed Replay contents by default**. An exported reference does not grant public access: Replay authorization rules remain enforced when the referenced Replay is fetched. No Bot credentials/tokens, concealed future-wall states or secret protocol traffic are included. Exact link/identifier format, access-control tests and schema version remain open.

## Decisions — Round 11 (2026-10-14)

### D83. Split Experiment Runs only at whole Paired Block boundaries — DECIDED (Q82: A)

To exceed a candidate Run's 1,000-Match limit, split the **Experiment** into whole-Block-aligned chunks. Each 3p candidate Run covers no more than **333 complete Paired Blocks × 3 Matches = 999 Matches**; each 4p candidate Run covers at most **250 × 4 = 1,000 Matches**. Create corresponding A/B candidate Runs for the same chunk. Never cut a candidate leg mid-Block, exceed the v1 Run limit, or switch to the other game mode. Each Run itself remains a normal automated sequence of Bot-vs-Bot Matches; an **ordinary standalone Run** is not split into A/B chunks without its Admin explicitly choosing the Experiment workflow. The per-Experiment chunk planner/queuing schema remains to specify.

### D84. Sequential A-then-B chunk execution — DECIDED (Q83: A)

Within a paired chunk, run the complete Candidate **A Run**, then its corresponding Candidate **B Run**; then advance to the next paired chunk. The continuous Match progression *within each Run* is unchanged. Record actual wall-clock/environment/provenance data so possible time-of-day or drift confounding can be disclosed; sequential A then B does not itself eliminate environmental bias. Automated execution between Experiment-managed Runs stays bounded by the fixed Experiment plan.

### D85. Canonical cyclic reference seating shared by A and B — DECIDED (Q84: A)

Within one exact 3p/4p game mode, define a canonical fixed order of Reference Participant IDs and cyclically place them in the Seats not occupied by the Candidate. Rotate the Candidate through every Seat once per Paired Block, then use **the exact same Participant-to-Seat reference mapping** for corresponding B-leg Matches as A-leg Matches. Persist the seed, fixture identity and resulting seating plan. The canonical starting anchor, direction and deterministic algorithm/version are still to be specified.

### D86. Equal-weight complete-Block mean-placement difference — DECIDED (Q85: A)

For every **complete matched A/B Paired Block**, calculate that Block's `mean_placement(B) - mean_placement(A)`, and take the **arithmetic mean of the Block-level differences with equal Block weight** for the primary observed effect. Incomplete Blocks and descriptive-only Runs are excluded from this estimator; raw completed constituent Matches remain available descriptively. The already-approved 95% confidence interval bootstraps these same complete Block-level differences.

### D87. Pinned statistical PRNG, input ordering and quantile convention — DECIDED (Q86: A)

Define and persist a **versioned analysis PRNG and its seed format**, canonical input order, number of replicates (**10,000**), and the exact 95% percentile-interval quantile/interpolation method. Statistical RNG is separate from Mahjong game RNG. The particular standardized PRNG and interpolation specification are still open; host-language unspecified defaults must not decide reproducibility.

### D88. Durable Replay before one atomic SQLite completion commit — DECIDED (Q87: A)

Persist a valid Replay durably first (via a safe staging/finalization protocol), then **atomically in SQLite** commit final result, Replay reference, fixture/Attempt association, and Run progress. Replay files and SQLite are not one cross-store ACID transaction, so recovery must reconcile Replay files and DB records, clean orphaned staging artifacts only when safe, and never count an absent/invalid Replay. The exact staging/finalize/cleanup ordering and file durability primitive remain an implementation-design frontier.

### D89. Persisted Experiment recovery requires explicit Admin Resume — DECIDED (Q88: A)

After server restart, reconstruct the saved Experiment plan, successful Match results, remaining Run queue, and fixture identities from SQLite and durable Replays. Mark previously **in-flight work as Interrupted/Blocked pending reconciliation**; do **not** automatically resume an active Match, Run or Experiment. Admin must explicitly request Resume after valid identity/reset checks. Standalone legacy v1 interruption remains fail-closed/read-only under its existing lifecycle.

### D90. Immutable scheduled fixture and Attempt idempotency identities — DECIDED (Q89: A)

Assign durable identity for `Experiment / Paired Block / scheduled Fixture / Attempt`. Each actual Attempt has its own unique Match ID and immutable attempt number; on successful durable Match commit, the **scheduled fixture** is counted at most once. Preserve all failed Attempts separately, make completion/reconciliation idempotent and prevent duplicate score increments after restart or duplicated completion signals. SQLite constraints and transaction endpoints remain open.

### D91. One-time scoped reset request ID for external Bots — DECIDED (Q90: A)

A v2-capable adapter issues an **opaque single-use reset request ID** scoped to the Participant, planned Match and Attempt (and the immutable declared build/checkpoint). Accept a clean-state acknowledgement only for the matching authenticated Participant and current request; reject duplicated, stale, replayed or wrong-Attempt acknowledgements. This is a lightweight protocol correlation check, **not** independent proof of binary execution. Built-in Bot reset is controlled by server lifecycle separately.

### D92. Periodic and pre-payload Admin SSE authorization enforcement — DECIDED (Q91: A)

Check Admin session/authorization on initial SSE connection, **periodically throughout streaming**, and **before emitting private all-hands state**. Close on known expiry, revocation or access loss; do not let an old open stream bypass read-only/Admin-only projection limits. Polling fallback and authorized Replay fetches enforce their own auth. Exact interval and revocation notification integration remain future implementation-design choices.

### D93. Bounded latest-state queue for slow Admin viewers — DECIDED (Q92: A)

Bound each Admin SSE viewer's pending-state memory. Replace obsolete queued snapshots with the most recent authoritative state/revision instead of accumulating every Match action; ensure terminal Match/Run state is delivered when possible or is obtainable via a fresh authorized snapshot on reconnect. Clients too slow to remain within bounds should be disconnected/reconnected safely, with stale-state indication. Do not block the engine's continuous Match progression for a slow spectator.

### D94. Fail closed when mandatory Replay/results cannot persist — DECIDED (Q93: A)

If the server cannot durably store **required** Replay, result or failed-Attempt evidence, prevent further Match advancement and do not mark the unfinished Match completed. Display an appropriate recoverable Blocked or terminal Failed state based on integrity/recovery eligibility, while preserving previous durable results. **Optional diagnostic log truncation is different** and must not stop healthy Match progression. Never delete previously committed Match results to reclaim space. Storage error/recovery transaction details remain open.

### D95. Pinned deterministic rendered screenshot regressions — DECIDED (Q94: A)

Use a **pinned browser/graphics/rendering environment and viewport** with deterministic game state, distinct per-Seat scores, camera/view orientations, and documented pixel-difference tolerances for rendered screenshot regressions. Pair these with authoritative Participant/Seat/score association tests across 3p/4p, normal live, Replay and Benchmark Admin focus, as well as Reduced Motion and GPU/Worker paths. This does not claim the earlier user-reported normal-live center-score misplacement is fixed. No test code/CI execution is authorized until implementation permission.

### D96. Consistent exported SQLite snapshot/revision — DECIDED (Q95: A)

Generate versioned JSON and normalized CSV from **one coherent persisted SQLite snapshot/revision**, including common snapshot timestamp, Experiment configuration/version, scheduled fixtures, committed Match data and incomplete/active status. During a running Experiment, label any partial/incomplete Paired Blocks and provisional descriptive results; do not present in-flight work as finalized. Replay references remain access-controlled, and no credential/secret data enters export. The snapshot transaction duration/streaming strategy remains open.

### D97. Benchmark core purpose is continuous Bot-vs-Bot Matches + dashboard/live preview — OWNER CLARIFICATION (not a multiple-choice question)

Benchmark Mode's principal user experience is **MCP/MJAI/Built-in Bots fighting one Match after another in an Admin-created 3p or 4p Benchmark Run**, while the dashboard reports accumulated/per-Match performance and offers live game preview. An A/B Experiment with fixture scheduling, statistical confidence intervals, and provenance requirements is an **optional additional analysis/orchestration layer**. The user did not approve requiring an Experiment to start ordinary Benchmark Matches, replacing the main dashboard with an A/B-only view, or auto-running both 3p and 4p. **Later Q99 B supersedes the assertion that non-experimental Bots may join Benchmark v2 without v2 capabilities; migration of the existing v1 protocol is unresolved.** All v1 bounded Run semantics remain unless separately decided. User's "continuous" statement does **not** authorize infinite or unattended auto-renewing Runs.

## Decisions — Round 12 (2026-10-14)

### D98. Run-first Benchmark home — DECIDED (Q96: A)

The primary Admin Benchmark workflow starts a continuous, bounded Run for a chosen exact game mode and Bot roster, showing live progress, outcomes and a live 3D table. An **Experiment is an optional secondary area** and not necessary to start or observe a standalone Run.

### D99. Continuous until configured finite target — DECIDED (Q97: A)

Automatically advance Match-to-Match until the configured 1–1,000 completed-Match Run target, stop/failure, or approved pause boundary; no indefinite self-renewing standalone Run. An optional Experiment may queue further capped Runs under its fixed plan.

### D100. Live preview and Replay/history for standalone Runs — DECIDED (Q98: A)

Give every active ordinary Benchmark Run Admin-only read-only 3D live preview and ongoing statistics, then historical results and authorized Replay links after completion. Do not reserve the live view for A/B Experiments.

### D101. All Benchmark v2 Bots must support v2 capability — DECIDED (Q99: B; migration settled by Q110 B)

Every MJAI/MCP/Built-in Participant in every Benchmark v2 Run must support the versioned v2 identity/reset capability, **even without Experiment membership**. The owner also selected an intentionally breaking new-release switch of existing Benchmark bot connections (D113). Legacy-only Bot clients therefore need upgrading before that cutover. Existing Replay/results remain accessible; exact deployed route inventory and migration/error notices are open. This is not permission to perform the cutover now or require independent runtime attestation.

### D102. Log and disclose A/B temporal drift — DECIDED (Q100: A)

Capture actual run/match wall-clock windows and relevant engine/Bot environment version metadata for A-then-B chunks; warn that conditions may drift over time despite matching seed and Seat plans. Do not silently change the chosen order.

### D103. Safely garbage-collect only reconciled orphan Replay stages — DECIDED (Q101: A)

On restart, first reconcile staged and final Replay artifacts with authoritative DB records and active/pending fixtures. Remove only provably unreferenced staging artifacts after a safety grace period, never a Replay still needed for recovery.

### D104. Explicit durable Experiment state machine — DECIDED (Q102: A)

Use SQLite to persist explicit Experiment states and transitions (e.g., Draft, Queued, Running, PauseRequested, Paused, Blocked, Interrupted, Completed, CancelRequested, Cancelled, Failed). Differentiate Run/Attempt states; exact transition edges remain open.

### D105. Per-Participant negotiated v2 capability/version — DECIDED (Q103: A)

Negotiate declared capability **per Participant**, not per Bot Token, since one token may authenticate several Participants. Round 12 Q99 B requires all Benchmark v2 participants to be capable; Round 13 Q110 B authorises the *design choice* to migrate legacy Benchmark bot admission upon future release. No running endpoint changes during the design interview.

### D106. Non-sensitive SSE heartbeat and stale detection — DECIDED (Q104: A)

Send heartbeat messages even during long Bot thinking periods; mark a missed-heartbeat timeout as a stale/disconnected Admin viewer and reconnect via authenticated current snapshot or polling fallback. Absence of game changes does not imply disconnection.

### D107. Automatic optional-diagnostics retention cleanup — DECIDED (Q105: A)

Delete expired **optional** diagnostic records at bounded startup and periodic maintenance intervals. Preserve mandatory Match results, authorized Replay artifacts and failed-Attempt evidence outside diagnostic expiry and quotas.

### D108. Pinned linear-percentile Bootstrap convention — DECIDED (Q106: A; final type Q120 A)

Use **Hyndman–Fan Type 7** (explicitly chosen Round 13) linear interpolation for the 2.5% and 97.5% endpoints of the 95% confidence interval from 10,000 complete-block Bootstrap replicates, with separate pinned analysis PRNG/seed and canonical input order. Floating precision and portable test vectors remain open.

### D109. Versioned JSON/CSV research schema — DECIDED (Q107: A)

Use `schema_version`, stable IDs, paired/descriptive labels and explicit absent-value semantics. Additive changes preserve backward compatibility; breaking semantic changes require a versioned migration.

### D110. Render both actual main-thread and Worker/bitmap 3D output — DECIDED (Q108: A)

Rendered screenshot regressions must inspect both supported actual 3D paths (main-thread and Worker/bitmap) and verify Seat/score mapping across 3p/4p normal live, ordinary Benchmark live, Replay and Admin focus. Existing user-reported center-score wrong-Seat defect remains **unverified as fixed**; this is test planning only.

### D111. Keep bounded samples during Match, persist aggregates — DECIDED (Q109: A)

Maintain enough observed per-Decision timings in a bounded Match-time buffer to derive first-response and valid-action count/mean/median/p95; persist per-Match aggregates normally. Raw Decision samples may be retained only in optional quota-bound diagnostics; explicitly disclose incomplete sample distributions rather than manufacture exact p95.

### D112. Modern UI component system and removal of obscure UI copy — OWNER REQUEST REFINED BY Q112–Q117

The user requests a modernized app UI with meaningless decorative/poetic copy removed. Round 13 fixes **shadcn/ui, incremental Tailwind and coherent design tokens**, minimalist information-dense appearance, short useful labels, **every app page included in one public UI release** (Q114 B), and an internal sequence of shared foundation → existing Run-first Benchmark dashboard → remaining pages → optional v2 feature screens. Avoid a partially redesigned public release, but support incremental review/testing internally. Do not rewrite specialized 3D rendering just to use shadcn. Q117 adds simultaneous table/metrics on the dashboard; the literal addendum "3卓を全画面表示できるように" is **ambiguous** (3 separate tables vs one 3D table fullscreen) and must be clarified in Q125. No UI code is authorized during this interview.

## Verified v1 baseline## Decisions — Round 13 (2026-10-14)

### D113. Breaking Benchmark Bot admission migration — DECIDED (Q110: B; Q131 A, Q132 A)

At the explicitly authorized future new Benchmark release, **replace the existing legacy Benchmark Bot admission with mandatory v2 capabilities**, not a permanent v1 compatibility endpoint. **Q131 A:** old unsupported v1 Benchmark clients receive an actionable, explicit protocol/version mismatch with upgrade guidance; never claim missing build/checkpoint/reset data as valid. **Q132 A:** drain/stop existing v1 Benchmark Runs deliberately *before* cutover; preserve recorded history, accurately label incomplete/interrupted work, and never silently upgrade an in-flight v1 Run to strict v2. This breaking choice does not automatically affect unrelated Room or normal-play APIs. Exact endpoint inventory, wire status/error fields and maintenance/rollback workflow remain open. No production migration or code changes are authorized now.

### D114. Native Built-in v2 Bot contract — DECIDED (Q111: A, Q133: A)

Server-native Built-in Bots implement the same mandatory **Participant-scoped protocol/capability version, build/checkpoint identity and clean reset contract** as external MJAI/MCP Bots without external Bot Tokens. **Q133 A** requires a versioned Participant identity/model declaration for **every** v2 Benchmark Run, standalone or Experiment, and an explicit reset acknowledgement/record when an eligible retry occurs. Declarations by trusted external Bot Token holders are accepted without independent binary attestation; no gratuitous per-Match heavyweight process proof is required.

### D115. Shared UI foundation before v2 features — DECIDED (Q112: A)

Develop modern shared components/tokens and redesign the **existing standalone Benchmark Run dashboard before implementing new v2 feature screens**. Complete the UI release before the later Benchmark v2 release, with separate future implementation authorization.

### D116. shadcn/ui plus Tailwind and tokens — DECIDED (Q113: A)

Adopt shadcn/ui in the existing React/Vite frontend with an explicit design token system and incremental Tailwind integration during internal development. Preserve the specialist 3D/Pixi table renderer; primitive selection and concrete theme tokens remain open.

### D117. Every app page in one public UI cutover — DECIDED (Q114: B, Q126: A)

All app pages (Admin, Benchmark, Lobby/Room, live game, Replay and shared navigation) receive a coordinated new UI in **one public release**. Internal development **may and should** proceed incrementally by page/component with independent review and verification (Q126 A); partial public cutovers are not approved. The one-release signoff requires full route coverage and a rollback/compatibility gate; exact rollout method remains to decide.

### D118. Delete cryptic decorative prose, keep functional guidance — DECIDED (Q115: A)

Remove decorative pseudo-poetic, promotional or cryptic copy, replacing with concise functional labels where useful. Preserve errors, privacy/security notices, accessibility descriptions and necessary help.

### D119. Functional minimal dense dashboard visual direction — DECIDED (Q116: A; Q127 A)

Modern minimal, information-dense dashboard visuals, coherent **light and dark themes**. Initial palette follows the operating-system/browser theme preference, with **manual override** available to users (Q127 A). Maintain accessible colors/contrast, readable statistics and reduced motion. Exact color tokens, theme-preference persistence and flash-of-wrong-theme safeguards are open.

### D120. Run-first table alongside live statistics — DECIDED (Q117: A; Q125: B clarifies fullscreen)

The ordinary Benchmark dashboard presents one live **3D Mahjong table together with current Run progress, controls and key performance statistics**; history and detailed diagnostics remain accessible below or via tabs. **Q125 B resolves the original Japanese addendum "3卓を全画面表示できるように": the owner meant the existing 3D table should be expandable to fullscreen, *one live Match at a time***. This does **not** ask for three Mahjong tables or simultaneous Runs/Matches. Fullscreen entry/exit, access to live overlays and mobile behavior are new Round 15 questions.

### D121. Interrupted and Blocked are distinct — DECIDED (Q118: A; Q134: A)

**Interrupted** means restart/crash stopped active work pending reconciliation; **Blocked** means a recoverable unmet prerequisite. Neither auto-resumes or counts as Completed. **Q134 A** requires SQLite transactions, conditional current-state/revision checks and unique fixture/Attempt keys so simultaneous Admin Resume or callbacks cannot schedule duplicate work or overwrite durable results. Persisted transition-table specifics and conflict response remain open.

### D122. SSE 15s heartbeat / 45s stale — DECIDED (Q119: A; Q135: A)

Non-sensitive SSE keepalives about every **15 seconds**; visibly stale/disconnected after roughly **45 seconds** without stream/heartbeat connectivity, not when a Bot simply spends time deciding. **Q135 A** requires bounded exponential authenticated reconnect backoff **with jitter**, a polling fallback during repeated failures, fresh authorized snapshot and clear stale labeling. Specific attempt ceilings, backoff intervals and fallback/recovery behavior remain open.

### D123. Hyndman–Fan Type 7 bootstrap quantile — DECIDED (Q120: A)

Use explicitly pinned **Type 7** linear interpolation at 2.5%/97.5% quantiles for the seeded 10,000-replicate complete-block Bootstrap CI. Keep analysis PRNG separate from gameplay RNG, with portable numeric tests needed.

### D124. Persisted UTC diagnostic expiry timestamps — DECIDED (Q121: A)

Base optional diagnostic expiry on persisted UTC timestamps and bounded maintenance, not browser timezone or sudden clock-reset deletion. Do not expire durable Match results/Replay/Attempts.

### D125. Manifest + canonical JSON/normalized CSV in one archive — DECIDED (Q122: A; Q136: A)

Only an **authorized Admin** may create/download a consistent-revision Experiment research archive containing a schema/snapshot manifest, canonical JSON and normalized CSV. **Do not** embed Bot Tokens, private credentials, future concealed wall state or Replay body content; stable Replay references remain separately access-controlled. Exporting an active Experiment may describe incomplete Blocks/Run progress without suggesting a finalized paired claim. Archive authorization, expiring download mechanics and audit granularity remain open.

### D126. Investigate center-score Seat bug separately from styling — DECIDED (Q123: A; Q137: A; Q138: A)

Do not claim shadcn/ui or CSS redesign magically fixed the previously user-observed **normal-live central score shown near the wrong Seat**. Identify/reproduce it with rendered-pixel 3p/4p screenshot tests, including main-thread and Worker/bitmap variants, then fix a demonstrated cause in a distinct attributable work item. **Q137 A:** the *whole-app one-public-release UI cutover* is gated on route-wide E2E smoke, accessibility and targeted rendered visual checks for desktop/mobile. **Q138 A:** record baseline route startup, JS bundle, memory, interaction and live 3D rendering performance before the redesign, with reviewed regression budgets. Specific budget numbers/platform fixtures are still open. **These are design requirements only; no test execution was requested or performed.**

### D127. Bounded timing samples, no false exact p95 — DECIDED (Q124: A; Q139: A)

Match-time Decision sample buffers have a fixed bound. After overflow, report accurate **count and streaming mean** for genuinely observed first-response / accepted-valid-action timings, mark the distribution **incomplete** and show **p95 unavailable**, not an artificially exact p95 computed from silently discarded samples. Do not allocate unbounded RAM. Quantitative cap, denominator fidelity and UI representation remain a future decision.

## Verified v1 baseline## Decisions — Round 14 (2026-10-14)

### D128. Exactly one live 3D table may expand fullscreen — DECIDED (Q125: B)

The owner's phrase "3卓を全画面表示できるように" **means the current single 3D Mahjong table**, as embedded in the Run-first Benchmark dashboard, must offer fullscreen expansion **of one active Match at a time**. Do **not** design three simultaneous Mahjong tables or extra Match concurrency. Existing live preview, chosen Participant focus and read-only state persist across fullscreen transitions. Entry/exit, mobile API fallback and accessibility details remain open (Q140).

### D129. Internally phased development, one whole-app public UI release — DECIDED (Q126: A)

Develop/review/verify shared UI components and individual pages incrementally, but expose **the completely migrated app** in a single release. No mixed-theme public rollout. Require all-route regression testing, a coherent release gate and rollback strategy (Q144), consistent with Q112 A and Q114 B.

### D130. Light/dark theme with system default and user override — DECIDED (Q127: A)

Both palettes are supported. Initially follow system/OS preference, but allow users to select a manual override. Persisting that override, server-rendering/initial-flash behavior and accessibility contrast tests remain open (Q141).

### D131. Shared role-aware navigation shell — DECIDED (Q128: A)

Use a consistent app-wide navigation shell for Admin, Room/Lobby, normal game, Run-first Benchmark dashboard, optional Experiments and Replay, showing appropriate destinations/actions according to permissions. The shell is a UI affordance **not a substitute for server authorization**. Exact routes, deep-link retention and permission failure behavior remain open (Q143).

### D132. Responsive compact Benchmark dashboard — DECIDED (Q129: A)

On compact screens, stack one live 3D table, Run status/controls and key accumulated statistics vertically, with Match history/detail in accessible tabs. Do not hide the table just because the viewport is small; support the single-table fullscreen functionality of D128. Tab preservation and device-specific fullscreen fallback remain open (Q142).

### D133. Keep specialist 3D/Pixi renderers in UI redesign — DECIDED (Q130: A)

Retain the current Three.js/Pixi specialized table/game rendering paths; rework shared shell/chrome, overlays and controls to match shadcn/ui/tokens. A separately proven rendering correctness bug may be fixed independently. Do not assume a generic component library can replace the game renderer or remove game views from the all-pages release.

### D134. Explicit version mismatch and migration guidance — DECIDED (Q131: A)

After the future deliberate breaking Benchmark Bot cutover, reject v1-only Benchmark Bot clients with an actionable **protocol/version mismatch** indication and guidance for updating. Never silently insert fake checkpoint declarations or treat a legacy client as v2-capable. Exact transport-specific error and closing behavior remain open (Q145).

### D135. Drain legacy v1 Runs before strict-v2 cutover — DECIDED (Q132: A)

Before enabling new v2-only Benchmark admissions, perform a deliberate drain/stop of existing v1 Benchmark Runs. Preserve durable historical Run/Match/Replay results and report incompletely finished work honestly; do not silently reinterpret v1 in-flight Attempts as v2. Cutover sequencing/maintenance and rollback behavior remain open (Q146).

### D136. Every v2 Bot declares identity and supports clean reset — DECIDED (Q133: A)

For *both standalone and Experiment-managed* v2 Runs, each Participant must provide a versioned v2 capability/identity declaration with build/checkpoint identity. External Bots use trusted, authenticated self-declarations, not independent execution attestation; Built-in Bots implement native server identity/reset. Clean-reset evidence is required when an eligible retry occurs, not a forced externally attested process restart before every Match. Exact negotiation schema and rejection errors remain open (Q147).

### D137. SQLite transactional state/revision CAS and uniqueness — DECIDED (Q134: A)

Condition state changes on SQLite's authoritative **current revision/state**, enforce transitions within a transaction and unique fixture/Attempt identities, and reject/reconcile races rather than letting stale browsers or duplicate completion callbacks mutate already committed progress. Exact API conflict response and transaction schema remain open (Q148).

### D138. Bounded SSE reconnect backoff and polling fallback — DECIDED (Q135: A)

On Admin SSE loss, show stale/disconnected, retry with **bounded exponential backoff plus jitter and fresh authentication**, and transition to authorized polling when necessary. New authenticated snapshots supersede cached frames; the viewer remains read-only and does not stall ongoing games. Specific backoff caps/fallback thresholds remain open (Q149).

### D139. Admin-only export archives with no secrets/Replay bodies — DECIDED (Q136: A)

Only authenticated Admins may export consistent Experiment JSON/CSV archives. Keep Bot Tokens, secret protocol payloads and Replay content out of the archive; Replay IDs remain permission-protected references. Decide download expiry, audit and packaging mechanics later (Q150).

### D140. Whole-app E2E/accessibility/render acceptance for single UI cutover — DECIDED (Q137: A)

Gate the entire one-release UI redesign on **every real app route**, responsive desktop/mobile smoke tests, accessibility checks and targeted **actual-rendered 3D screenshot** tests for live Room, Benchmark live, Replay and Admin focus. Preserve permission boundaries, UI semantics and normal gameplay, and cover both main-thread and Worker/bitmap paths where available. Explicit coverage matrix and required evidence remain open (Q151).

### D141. Measure performance baseline and reject unreviewed regressions — DECIDED (Q138: A)

Before any frontend redesign coding, capture known-good **route startup, JS bundle footprint, interactive responsiveness, memory and live 3D render performance** in a repeatable environment. Require reviewed regression budgets before the one-release UI cutover, rather than unlimited degradation for appearance. Concrete supported devices, thresholds and exception process remain open (Q152).

### D142. On bounded sample overflow, hide exact p95 — DECIDED (Q139: A)

Maintain accurate online **count/mean** for valid observed Decision timings even if bounded raw sample memory is exceeded. Once a per-Match distribution is incomplete, label its p95 **Unavailable / Incomplete** (unless a future separately approved *approximate* estimator is explicitly distinguished). Do not invent exact values from dropped samples, mix failures with valid timings or silently allocate unbounded memory. The buffer cap and whether full aggregate median also becomes unavailable remain open (Q153).

## Verified v1 baseline (not v2 decisions)

- A Benchmark Run automatically and sequentially plays bot-vs-bot Matches under a fixed selected roster, rotating seats, until its configured 1–1,000 completed and durably recorded Match target is reached (or it stops/fails). It does not require an Experiment.
- Its Admin dashboard provides live progress, a read-only all-current-hands live table preview, per-Match results, and statistics such as mean placement, first-place rate and cumulative score relative to the table mean.
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

## Decision tree — Round 15 frontier

```text
Benchmark v2 & full-app UI redesign
├── D128,D132 single fullscreen 3D table / responsive   [DECIDED]
│   ├── Q140 Fullscreen enter/exit and overlay behavior  [FRONTIER]
│   └── Q142 Mobile fullscreen fallback                 [FRONTIER]
├── D130,D131 modern theme and shared navigation       [DECIDED]
│   ├── Q141 Theme override persistence                 [FRONTIER]
│   ├── Q143 Deep links / permission-restricted nav     [FRONTIER]
│   └── Q154 Copy audit and language compatibility      [FRONTIER]
├── D129,D140,D141 one public UI cutover, tested        [DECIDED]
│   ├── Q144 Atomic UI launch / rollback strategy       [FRONTIER]
│   ├── Q151 Route-wide UI acceptance matrix           [FRONTIER]
│   ├── Q152 Performance regression budget             [FRONTIER]
│   └── Q155 Pixel-visual baselines and 3D correctness   [FRONTIER]
├── D134–D136 strict-v2 Bot breaking migration          [DECIDED]
│   ├── Q145 Protocol mismatch wire contract           [FRONTIER]
│   ├── Q146 Legacy Run drain deadline/recovery         [FRONTIER]
│   └── Q147 Capability handshake validation           [FRONTIER]
├── D137 durable CAS and unique Attempt identities      [DECIDED]
│   └── Q148 Stale Admin transition response           [FRONTIER]
├── D138 authorized SSE retry/polling                    [DECIDED]
│   └── Q149 Backoff / polling takeover policy         [FRONTIER]
├── D139 Admin-only export archive                      [DECIDED]
│   └── Q150 Archived download authorization lifetime  [FRONTIER]
└── D142 bounded Decision timing                        [DECIDED]
    └── Q153 Latency sample cap + median completeness   [FRONTIER]
```

### Round 15 questions — awaiting owner

- **Q140, single 3D table fullscreen controls:** A — an explicit fullscreen toggle for the *existing single table*, Escape/close to return to the same dashboard state, accessible controls and compact read-only live overlays (current Match/seat/status) while fullscreen; B — replace dashboard permanently when entering fullscreen; C — create three simultaneous Matches. **Recommendation A**: respects Q125 B without changing game scheduling.
- **Q141, theme preference storage:** A — system default until explicit choice, persist user override locally and apply early to prevent a visible wrong-theme flash; B — reset override after each page; C — store a theme on each Room/Match independently. **Recommendation A**; server-synced preferences can be discussed later if accounts require them.
- **Q142, fullscreen on small/mobile devices:** A — use Fullscreen API where supported; otherwise an accessible viewport-filling in-page presentation, preserving orientation/return control; B — remove mobile fullscreen entirely; C — require landscape and permanently lock navigation. **Recommendation A**.
- **Q143, shared nav and protected deep links:** A — use role-aware navigation but independently guard every server resource; preserve valid deep links through auth and display an explicit access-denied view where appropriate; B — treat hidden links as sufficient authorization; C — redirect all pages to Home and lose deep links. **Recommendation A**.
- **Q144, whole-app UI cutover and rollback:** A — a single tested UI artifact/release behind a controlled switch with a prepared prior-UI rollback artifact; freeze the route inventory and signoff checklist before launch; B — overwrite each page live as soon as ready; C — release without any rollback path. **Recommendation A**; no backend protocol cutover is implied merely by a UI release.
- **Q145, strict v2 Bot mismatch wire error:** A — define explicit versioned `unsupported_benchmark_protocol` structured error (supported/required version, migration-doc location), with HTTP/WS/MCP transport-appropriate failure framing and no leaked secrets; B — generic 500 with no version; C — silently claim v2 capability. **Recommendation A**.
- **Q146, active legacy Benchmark Run during protocol cutover:** A — announce a maintenance/drain window, allow eligible current Matches to persist, then mark unfinished Runs interrupted and preserve history; require explicit operator review before strict-v2 start; B — erase all Runs at deployment; C — silently let a v1 Match become a v2 Match. **Recommendation A**.
- **Q147, admission handshake validation:** A — per-Participant negotiated version plus immutable trusted build/checkpoint declaration before Run starts, and a scoped reset capability recorded for retry (Built-in supplied natively); B — one shared Token version with no per-Participant identity; C — defer verification until an Experiment is created. **Recommendation A**.
- **Q148, stale SQLite CAS/Admin actions:** A — return an explicit state/revision conflict with latest authoritative status and no duplicate work; client offers Refresh/Retry, not silent overwriting; B — execute both Admin Resumes; C — overwrite DB using stale UI state. **Recommendation A**.
- **Q149, SSE retry and polling handoff:** A — bounded exponential retry with jitter and one active stream per viewer; after repeated failures use authenticated polling until stream resubscription succeeds with a new snapshot; B — flood reconnects while polling and streaming simultaneously; C — continue to label a stale snapshot live. **Recommendation A**.
- **Q150, exported archive delivery:** A — only Admin can start/get an export; use short-lived authenticated download or streaming response, no permanent bearer links, with access rechecked and audit of export request; B — generate a forever-public download URL; C — package Bot Tokens so consumers can fetch anything. **Recommendation A**.
- **Q151, complete UI-route test matrix:** A — inventory every current route/role/state, test critical happy/error/permission flows at desktop/mobile, plus automated accessibility smoke and a manual final review before one release; B — only test the Benchmark page; C — treat build compilation as equivalent to acceptance. **Recommendation A**.
- **Q152, UI performance budgets:** A — baseline pinned desktop and modest mobile profiles (bundle size, route startup, interaction latency, 3D FPS/memory), with explicit budgets and owner-reviewed exceptions; B — only optimize CSS size; C — unlimited slowdowns for aesthetics. **Recommendation A**; concrete percentage/ms/FPS targets remain an implementation acceptance choice.
- **Q153, bounded timing sample precision:** A — keep accurate streaming count/mean for all genuinely observed samples, allocate fixed raw-sample cap and mark **median and p95 unavailable** after overflow, with clear completeness flag; B — calculate exact median/p95 from the surviving subset; C — allocate all raw samples without cap. **Recommendation A**.
- **Q154, multilingual UI copy audit:** A — remove opaque marketing/poetic copy in all supported locales via one inventory, retain short functional translations and accessible text; B — clean English only while keeping obsolete decorative translations; C — delete all helpful/translated UI instructions. **Recommendation A**, not silently changing which languages are already supported.
- **Q155, actual-rendered 3D visual regression baselines:** A — use pinned renderer/browser/viewport screenshot fixtures with distinct player scores and verified Participant-to-Seat IDs, thresholds and reviewable baselines across light/dark/fullscreen, normal Room, Benchmark, Replay and Admin focus; B — use CSS-unit tests only; C — approve without checking the known wrong-Seat issue. **Recommendation A**.

### Deferred decisions

Once Round 15 settles: actual route inventory and ownership matrix, concrete shadcn primitive package choices, UI theme tokens and component acceptance, physical deployment/rollback switches, strict-v2 error transport schemas and release drain operational steps, reset request wire payloads, SQLite transition table implementation, seed-stream encoding and serialized fixture golden vectors, SSE retry defaults and throughput test cases, authorized archive packaging tests, latency allocation cap defaults and exact streaming-metric definition. Neither the UI release nor the v2 backend/protocol cutover is authorised to start in this docs-only design interview.

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
