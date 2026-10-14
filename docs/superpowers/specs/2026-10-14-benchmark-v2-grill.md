# Benchmark Mode v2 — Grill with Docs (working design)

Date: 2026-10-14
Status: **Design interview in progress — Rounds 1–10 decided; Round 11 unanswered**
Owner: eitaar
Scope: research-oriented improvements to driichi's Benchmark Mode, not implementation approval.

## Sources and invariants

- Existing vocabulary: `CONTEXT.md`.
- Shipped v1 specification: `docs/superpowers/specs/2026-09-29-benchmark-mode-design.md`.
- Existing Run state: `crates/double_riichi_core/src/room.rs`.
- Persistent Run records and statistics: `crates/double_riichi_server/src/storage.rs`, `migrations/0006_benchmark.sql`.
- Existing UI: `frontend/src/benchmark.tsx`.

Keep v1's currently shipped behavior as the compatibility baseline. A design idea below is **not** a requirement or implemented feature until the owner decides it.

## Benchmark game-mode invariant — CONFIRMED BY OWNER (2026-10-14)

A Benchmark Room/Run/Experiment evaluation is configured for **one specific game mode**. A `4p` Benchmark plays **only four-player Mahjong** with four seats; a `3p` Benchmark plays **only three-player Mahjong** with three seats. **Creating or running one NEVER automatically starts or also executes the other**. A/B candidates, fixed reference Bots, seed fixtures, Paired Blocks, metrics and the target count all stay within the chosen mode. Round 9's 6-Match (3p) and 8-Match (4p) comparison-block sizes describe **alternative modes**, not combined workloads. The existing three-player MJAI prohibition remains in force.

These constraints hold for the existing v1 behavior and must not be weakened by v2. **Round 10 confirms exact ruleset/game-mode equality**: red-east and red-half are different fixtures even within the same player count. An Experiment may not automatically run or mix 3p and 4p, nor east and half. (D75)

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

Experiment supports **both** attaching previously completed Benchmark Runs for retrospective comparison and creating/running new Benchmark Runs from its Admin Web UI. A Run remains independently addressable and its committed Match results must not be silently rewritten when linked. Round 4 restricts admission to Runs whose relevant experiment conditions can be verified; historical Runs without sufficient evidence are not retroactively treated as reproducible or admitted. Round 6 adopts token-authenticated Bot self-declarations as sufficient artifact identity under the owner's trust policy. Round 7 binds an immutable model declaration to each Participant for the Run and requires matching declarations upon resume; linking reversibility remains open.

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

### D77. Optional versioned v2 Bot metadata/reset extension — DECIDED (Q76: A)

Add a **versioned, optional protocol extension** for MJAI/MCP adapters participating in v2 verifiable Experiments to convey Participant-scoped trusted build/checkpoint declarations and explicit authenticated clean-Match reset acknowledgements. The extension is **required to qualify for applicable v2 Runs**, but ordinary existing v1 MJAI/MCP Bot connections retain their current wire behavior. No new artifact attestation process or mandatory runner is introduced; Token holders remain trusted as declarants. The message format, negotiation, reset correlation and compatibility failure behavior are later decisions.

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

## Decision tree — Round 11 frontier

```text
Benchmark v2
├── D58,D70,D71 single-mode paired fixtures split across Runs  [DECIDED]
│   ├── Q82 Block-aligned Run chunking                       [FRONTIER]
│   ├── Q83 A/B leg ordering within the run queue             [FRONTIER]
│   └── Q84 Deterministic Seat/fixture assignment             [FRONTIER]
├── D59,D72–D74 reproducible 95% paired Bootstrap            [DECIDED]
│   ├── Q85 Unit of A/B effect aggregation                    [FRONTIER]
│   └── Q86 Analysis PRNG/quantile convention                 [FRONTIER]
├── D76 SQLite authoritative, D65 Replay+result completion   [DECIDED]
│   ├── Q87 Replay + DB persistence transaction boundary      [FRONTIER]
│   ├── Q88 Restart behavior for interrupted Experiment       [FRONTIER]
│   └── Q89 Idempotency key for scheduled Attempt             [FRONTIER]
├── D77 trusted, optional v2 Bot metadata/reset extension    [DECIDED]
│   └── Q90 Reset acknowledgement correlation                 [FRONTIER]
├── D78,D79 private 4 Hz SSE snapshots                        [DECIDED]
│   ├── Q91 SSE revocation/expiry checking mechanism          [FRONTIER]
│   └── Q92 Slow Admin viewer/backpressure                    [FRONTIER]
├── D80 1 GiB/7-day diagnostics plus durable results          [DECIDED]
│   └── Q93 Mandatory storage failures                        [FRONTIER]
├── D81 visual screenshot regression testing                 [DECIDED]
│   └── Q94 Rendered screenshot baseline strategy            [FRONTIER]
└── D69,D82 versioned JSON/CSV and authorized Replay links   [DECIDED]
    └── Q95 Export snapshot consistency                      [FRONTIER]
```

### Round 11 questions — awaiting owner

- **Q82, Run chunking for large fixed experiments:** A — allocate whole **Paired Blocks per candidate Run** (3p: at most 333 blocks/Run, 4p: 250 blocks/Run) and never split a candidate leg across Run boundaries; B — split each Run strictly after 1,000 Matches even mid-Block; C — allow a candidate Run above 1,000. **Recommendation A:** preserves v1 Run limit and auditable block boundaries.
- **Q83, A/B candidate execution order:** A — execute all of A's Matches for a chunk, then all of B's Matches for that same chunk; B — create short A and B Runs per Block, alternating legs; C — independently randomize all candidate legs without recording order. **Recommendation A** for fewer Run transitions under sequential scheduling, acknowledging potential time/environment drift; later record actual wall-clock and environment metadata.
- **Q84, Reference Bot seat rotation fixture mapping:** A — deterministically rotate Candidate through all Seats and assign each fixed Reference Participant to remaining Seats according to a canonical cyclic order, then replay **the identical mapping** in the B leg; B — independently randomize the reference seating in each leg; C — leave references in fixed Seats and force Candidate to remain one Seat. **Recommendation A** for reproducible seat-balanced fixtures.
- **Q85, block-level primary effect estimator:** A — for each complete A/B Block compute the mean-placement difference (B minus A), then **average the complete-Block differences equally**, excluding descriptive-only/incomplete blocks; B — pool all Matches without Block identity; C — use only the best Match from each Block. **Recommendation A**, matching the existing whole-Block bootstrap unit.
- **Q86, deterministic statistical algorithm contract:** A — record a pinned **analysis PRNG/version, canonical input ordering, seed, bootstrap replicate count and percentile interpolation method**; B — rely on the host language's unspecified default RNG and percentile behavior; C — regenerate an unrecorded random seed every dashboard view. **Recommendation A**, making the already-approved 10,000-replicate 95% CI reproducible across versions.
- **Q87, Replay/SQLite cross-store completion:** A — stage the Replay durably, then commit final result+Replay reference and Run progress **atomically in SQLite** with idempotent recovery/garbage cleanup; B — update Run progress before persisting the Replay; C — rely on current browser state to repair partial saves. **Recommendation A**, aligning with D65/D76 even though filesystem+SQLite are not a single ACID transaction.
- **Q88, Experiment status after server restart:** A — restore the persisted plan/results but set in-flight work to **Blocked/Interrupted**, requiring Admin's explicit Resume after reconciliation and clean Bot reset; B — immediately auto-resume without Admin; C — discard all queued Runs. **Recommendation A**, retaining the earlier explicit manual-resume decision.
- **Q89, idempotent scheduled attempt identity:** A — assign an immutable **Experiment/Block/Fixture/Attempt key** and enforce uniqueness of counted completion at the fixture level while preserving each failed Attempt's own Match ID; B — deduplicate only by Display Name and timestamp; C — assume the completion callback executes once. **Recommendation A**, preventing double counting across crashes and multiple Runs.
- **Q90, Bot reset acknowledgement correlation:** A — server issues an **opaque one-time reset request ID** scoped to Participant, planned Match and Attempt, and accepts only a matching authenticated acknowledgement with unchanged declared checkpoint; B — accept any generic `reset: true` message; C — treat a reconnect as reset. **Recommendation A**, small protocol change and robust replay protection without remote attestation.
- **Q91, Admin SSE revocation strategy:** A — validate authorization periodically *and before releasing snapshots*, immediately close on known expiry/revocation; B — validate just at connection start; C — stream to anyone holding any Bot Token. **Recommendation A**, ensuring no revoked viewer continues receiving all-hands updates.
- **Q92, slow Admin SSE clients:** A — use a bounded per-client queue and **replace superseded pending snapshots with the newest authoritative state**, terminating/restarting clients that cannot keep up; B — buffer every revision without limit; C — drop terminal state when buffers fill. **Recommendation A**, consistent with snapshot coalescing and final-state preservation.
- **Q93, required storage full/unwritable:** A — **fail closed**: stop advancing Matches if Replay/results/failed-Attempt evidence cannot be durably saved; show recoverable Blocked or terminal Failed according to integrity; B — continue matches and keep results only in RAM; C — delete prior completed Match results to free space. **Recommendation A**, because optional diagnostics may be truncated but mandatory results cannot.
- **Q94, screenshot regression acceptance baselines:** A — run screenshot comparisons in a **pinned deterministic browser/rendering environment** with controlled viewports, fixed distinct scores, camera orientations and documented pixel-diff tolerance; B — compare screenshots from arbitrary browsers without baseline pinning; C — use only mocked Canvas paint tests. **Recommendation A**, so the visual wrong-Seat bug can be detected meaningfully.
- **Q95, exports during an active Experiment:** A — generate JSON/CSV from one **consistent persisted snapshot with an export timestamp/revision**, explicitly identifying incomplete Blocks and work-in-progress without implying a final result; B — stream independent tables at different progress states with no revision marker; C — prohibit all exports until complete. **Recommendation A**, supporting research during long-running experiments without inconsistent joins.

### Deferred decisions

After Round 11: exact engine RNG/hash and stream-name specification; transaction commit/replay staging details; agent metadata/reset wire schemas and return errors; risk of A/B temporal drift depending on Q83; numerical CI method/version details and secondary-metric intervals; export schema and migration versions; SSE heartbeat/expiry cadence/default resource limits; optional detailed diagnostic cleanup/redaction implementation; high-fidelity rendered score-location acceptance thresholds and fixture test infrastructure. User signoff needed before **any** product-code changes, CI run, or PR merge.

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
