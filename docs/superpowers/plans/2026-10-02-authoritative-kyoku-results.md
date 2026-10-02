# Authoritative Kyoku Results Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. Operator authorization controls delegation; the currently authorized method is direct investigation, not implementation. Do not retry the broken subagent host or switch execution protocols without approval.

**Goal:** Capture authoritative ended-Kyoku results before state is overwritten, publish them in server-controlled steps, and preserve the same results in saved Replay.

**Architecture:** Extend the existing engine adapter seam with a pre-action capture; use the same fixed engine on a private copy to obtain adjudicated win details, subject to the first compatibility gate. MatchMachine withholds its internally prepared next Kyoku and terminal event suffix; the existing RoomActor publishes steps and accepts confirmations. Reuse existing projection, database and Replay modules; the frontend consumes server results rather than reconstructing them.

**Tech Stack:** Rust 1.95.0, `riichienv-core = "=0.4.10"`, Tokio, Serde, existing SQLite/sqlx and MJAI writer; React 19, TypeScript, Zustand, Vitest and Playwright already installed.

**Spec:** [#10 resolution](https://github.com/eitaar/driichi/issues/10#issuecomment-5961109168), [#12 resolution](https://github.com/eitaar/driichi/issues/12#issuecomment-5960734813), [#11 release policy](https://github.com/eitaar/driichi/issues/11#issuecomment-5961363330). Read the [exact-version investigation](../specs/2026-10-02-pre-action-result-findings.md) before Task 1. The comments are governing decisions; the investigation is evidence, not a replacement specification.

## Global Constraints

- Planning only until explicit approval. No production release, dependency patch, Cargo registry edit, prototype merge or deployment is authorized by this document.
- Preserve `.worktrees/prototype-dora`, branch `prototype/dora-language`, HEAD `3032942`; create an execution branch/worktree separately after approval. Paths below are repository-relative, not prototype edit instructions.
- No new dependency, client scoring engine, service, scheduler, event-store replacement or gameplay rule change. Preserve MJAI wire compatibility.
- Win Result → combined Kyoku Settlement → next Kyoku/Post-Match. Draw reason/settlement is one step. Each step lasts until all sampled interactive seated Humans confirm or **maximum five seconds from server publication**; zero targets advance immediately.
- No next-Kyoku state, legal actions, Decision/deadline, automatic action or Post-Match exposure during the gate. Unpublished/unoperated internal next-round preparation is permitted.
- Reveal only the current winner's new details; retain previously released details. Apply the boundary to history, snapshot, every transport and reads as well as writes. No newly revealed nonwinner concealed hands.
- Capture settlement **decision-resolution-before → delta → real engine after**; include final pot awards; already-paid riichi stays deducted. Every seat satisfies `before + delta = after`; delta sums need not be zero.
- Win Points exclude honba/deposits: Ron basic payment, Tsumo actual basic-payment total under the existing three/four-player rules. Do not substitute a held-point delta.
- Actual roster/IDs/initial seats/recorded names; unique ranks, tie by initial seat index ascending. Character metadata remains cosmetic.
- Unknown is not zero. Missing old data is omitted, never fabricated; yakuman has classification/multiplier, no invented han or `0 fu`. No new missing-result copy or ura-marker area.
- Keep the approved result composition, no result tile sheen, `Portrait unavailable`, focus/labels, Reduced Motion and failure fallbacks. Live timing never resets on assets, reconnect, resends or local Pause.
- Replay remains Admin-authorized and does not wait for Live confirmations. Saved data does not imply restart/resume of active Rooms.
- All automated, human visual, privacy and verified target-GPU gates from #11 are required before production release. This plan covers result/publication correctness; it is not the separate approved-table integration/performance plan or evidence that those gates passed.

## Review Focus

1. Final draw with leftover pot: real scores differ from a single-Kyoku copy; Task 2 must preserve the real final award exactly once.
2. Compound riichi discard / robbed kan / kita: capture the precise transaction and winning physical tile; Tasks 1–2 must not infer by tile kind or hand length.
3. A read opens a Decision or publishes final state: Tasks 3–5 must block `project`, `next_wake`, event-history and action APIs, not just the UI.
4. Confirmation arrives at the deadline or after reconnect/rematch: Task 4 must bind identities, preserve deadlines and advance once.
5. Corrupt new result metadata versus legitimately missing old metadata: Tasks 6–7 must distinguish invalid records from partial legacy display without inventing ranks or points.

---

## File map and interfaces

Keep the existing large modules; do not reorganize unrelated code. Add one focused core module, `crates/double_riichi_core/src/results.rs`, for result values/validation, and one focused frontend module, `frontend/src/game/results.ts`, for decoding them. Existing modules retain their responsibilities:

| Module | Change / responsibility |
| --- | --- |
| core `engine.rs` | Pre-action copy, same-engine adjudication and authority checks; no protocol/UI types |
| core `results.rs`, `lib.rs` | Protocol-neutral result values, validation and exports |
| core `match_machine.rs`, `projection.rs` | Ended-table projection, event-prefix publication, withheld next state and result |
| core `room.rs` | Publication clock/cohort/confirmation, history updates, persistence effects |
| server `http.rs` | Human confirmation input and safe public event/snapshot serialization |
| server `storage.rs`, migration `0007_authoritative_results.sql` | Versioned result metadata beside unchanged canonical `.mjson` |
| replay `frames.rs`, `reader.rs`, `lib.rs` | Join saved result metadata to canonical event indices without changing MJAI |
| frontend `game/{types,results,store,gameplay}.ts/tsx`, `api.ts`, `replay.tsx` | Validated server results, confirmation and Replay navigation |

### Shared values (introduced in Task 2)

Use existing `Seat`, `Tile`, `ParticipantId`, `GameMode` and `MatchResult`; no second roster or ranking implementation. Existing table `MeldState` has neither meld kind nor Serde support; do not silently discard those result fields or make a full TableState serializable.

- `YakuValue { id: u32, name: String, han: Option<u32>, yakuman_multiplier: Option<u32> }`: exactly one applicable value; names from the installed library, bonus counts from captured engine context.
- `WinEvaluation`: tagged `Ordinary { han: u32, fu: u32, limit: Option<ScoreLimit> }` or `Yakuman { multiplier: u32 }`. `ScoreLimit`: Mangan/Haneman/Baiman/Sanbaiman/CountedYakuman. Ordinary counted yakuman preserves real han/fu; explicit yakuman does not serialize inapplicable fields.
- `ResultMeld { kind: ResultMeldKind, tiles: Vec<Tile>, opened: bool, from_who: Option<Seat>, called_tile: Option<Tile> }`; `ResultMeldKind` is Chi/Pon/Daiminkan/Ankan/Kakan, translated from the captured library meld, not guessed from tiles.
- `WinResult { event_index: usize, winner: ParticipantId, seat: Seat, target: Seat, tsumo: bool, concealed: Vec<Tile>, melds: Vec<ResultMeld>, winning_tile: Tile, winning_tile_index: usize, yaku: Vec<YakuValue>, evaluation: WinEvaluation, win_points: u32 }`.
- `ScoreChange { participant_id: ParticipantId, seat: Seat, before: i32, delta: i32, after: i32 }`.
- `KyokuSettlement { changes: Vec<ScoreChange>, pot_before: u32, pot_after: u32, draw_reason: Option<String> }`.
- `KyokuResultRecord { version: u32, kyoku_sequence: u64, mode: GameMode, end_event_index: usize, wins: Vec<WinResult>, settlement: KyokuSettlement }`; `version = 1`. Sequence increases at every actual StartKyoku, including renchan. Event indices refer to the private complete canonical journal/writer, not the temporarily released Live prefix.
- `PublishedKyokuResults { kyoku_sequence: u64, wins: Vec<WinResult>, settlement: Option<KyokuSettlement> }`; no future winners or settlement before their step.

### Shared publication values (introduced in Task 4)

- `ResultStepId { match_id: String, kyoku_sequence: u64, step_index: u16 }`, equality on all fields.
- `KyokuEndContent`: tagged `Win(WinResult)`, `Settlement(KyokuSettlement)`, or `Draw(KyokuSettlement)`.
- `KyokuEndStep { id: ResultStepId, content: KyokuEndContent, published_at_ms: u64, deadline_at_ms: u64, remaining_ms: u64, required: Vec<ParticipantId>, confirmed: Vec<ParticipantId> }` in `RoomSnapshot.kyoku_end: Option<KyokuEndStep>`.
- Internal expiry uses monotonic `Instant`; epoch milliseconds are a wire representation. Refresh remaining time at serialization; reads do not start or reset a step.

The full capture/result record is private. Only `PublishedKyokuResults` and the current `KyokuEndStep` are public DTOs; persistence effects are never serialized as Room updates.

## Task 1: Prove and add pre-action engine capture

**Files:** Modify `crates/double_riichi_core/src/engine.rs`; tests in its existing `#[cfg(test)] mod tests`. Adapt the local research probe assertions into these checked-in tests; do not rely on an ignored `.git` file at execution time.

**Interfaces:** Keep `EngineAdapter::apply` / `apply_simultaneous` arguments; change their success return to `EngineApplyOutcome { events: Vec<GameEvent>, ended: Option<EngineKyokuEnd> }`. `EngineKyokuEnd` is crate-private and holds the pre-action and ended-copy state, adjudicated library win-results map, and **real** post-action score/pot values. Update both callers in `MatchMachine::resolve_actions` as part of this slice, initially consuming `outcome.events` only.

- [ ] **1. Write failing tests** `pre_action_capture_survives_next_round` and `simultaneous_capture_uses_one_resolution`: four modes, ordinary Tsumo/two-Ron and capped Kokushi. Controlled ordinary pre-Ron kinds are `[9,10,11,12,13,14,15,16,17,18,18,18,19]`, winning physical ID `77`; Kokushi kinds are `[0,8,9,17,18,26,27,28,29,30,31,32,33]`, winning ID `1`. Assign distinct physical copies, disable first-turn/riichi flags, use honba/pot `2`, and use sufficient balances to avoid accidentally testing bankruptcy instead of advancement. Add full seeded legal-action lifecycle coverage, not only prepared states.

```rust
assert_eq!(capture.real_after_scores, adapter.scores());
assert_eq!(actual_hora_values, copied_hora_values); // ordered arrays, all fields
assert_eq!(capture.win_results.len(), accepted_winner_count);
assert!(actual_next_round_win_results.is_empty());
assert_eq!(capped_kokushi_result.han, 13);
```

- [ ] **2. Run RED:** `cargo test -p double_riichi_core --lib pre_action_capture`; expected missing capture/return contract, not an unrelated setup failure.
- [ ] **3. Implement** one capture after validation/raw-command translation and before mutating `step` in both apply paths. For a compound RiichiDiscard, replay both raw steps from the same capture. Adopt an ended capture only after the real operation succeeds and emits EndKyoku. On that disposable copy only set the existing stop boundary to `0`/`3`; execute the same raw command batch, retrieve `win_results`, ignore artificial EndGame. Compare actual/copy Hora and Ryukyoku semantics; compare scores except the specifically understood real final-pot award. Retain no next-round data in the ended table. A divergence is an adapter error, not a guessed result.
- [ ] **4. Run GREEN and compatibility gate:** `cargo test -p double_riichi_core --lib`. Add named cases for `capture_rejection_is_not_adopted`, `capture_preserves_riichi_kan_kita_context`, `capture_preserves_responsibility_and_yakuman_rules`, `capture_preserves_draw_classification`: riichi declaration discard, ippatsu, last tile, rinshan, robbed kakan/ankan, kita/robbed kita, open/closed kans, ura/red/multiple Dora, counted/stacked yakuman, responsibility variants, sanchaho/abortive/exhaustive/nagashi, bankruptcy/agari-yame/dealer continuation. Assertions compare actual ordered terminal events and identities, not just the existence of a result. **Do not proceed if the candidate fails; report the exact case and request a new design decision before any dependency patch.**
- [ ] **5. Commit:** `git add crates/double_riichi_core/src/{engine,match_machine}.rs && git commit -m "feat(core): capture engine state before terminal resolutions"`.

**Known ceiling:** one whole-state/log clone per resolved batch, extra engine evaluation only on accepted terminal batches. Add a `ponytail:` comment naming that ceiling; measure it in Task 8. Do not create a snapshot abstraction or alter logging fields speculatively.

## Task 2: Build validated win, settlement and final-result values

**Files:** Create `crates/double_riichi_core/src/results.rs`; modify `engine.rs`, `match_machine.rs`, `lib.rs`; tests beside the touched code.

**Interfaces:** Add `KyokuResultRecord::validate(&self, roster: &[Participant]) -> Result<(), String>` and `MatchResult::validate(&self) -> Result<(), String>`. Adapter method `EngineAdapter::result_record(&self, ended: &EngineKyokuEnd, roster: &[Participant], kyoku_sequence: u64, first_event_index: usize, events: &[GameEvent]) -> Result<KyokuResultRecord, EngineError>`. Values are the shared definitions above; errors are mapped to existing adapter divergence/abort handling, not silently accepted partial new results.

- [ ] **1. Write RED tests** `result_preserves_physical_winning_position`, `settlement_uses_real_scores_and_final_pot`, `result_rejects_inconsistent_roster`, `final_result_preserves_initial_seat_ties`.

```rust
assert_eq!(win.concealed[win.winning_tile_index], win.winning_tile);
assert_eq!(winning_highlight_count, 1);
assert_eq!(multi_ron_record.wins.iter().map(|w| w.seat).collect::<Vec<_>>(), hora_order);
for change in &record.settlement.changes {
    assert_eq!(change.before.checked_add(change.delta), Some(change.after));
}
assert_eq!(final_draw.settlement.changes[0].delta, 2_000);
assert_eq!(final_draw.settlement.pot_after, 0);
assert_eq!(final_result.players.iter().map(|p| p.rank).collect::<Vec<_>>(), vec![1, 2, 3]); // tied 3p fixture
```

- [ ] **2. Run RED:** `cargo test -p double_riichi_core --lib result_`; expected missing typed values/validation.
- [ ] **3. Implement** the result adapter. Tsumo: locate the exact captured `drawn_tile` physical ID in the already-complete hand; Ron: append the accepted winning physical tile to that winner's captured concealed vector and explicitly record the appended index. Never use kind membership, dedup or length inference. Preserve meld kind/openness. Read winner order from actual Hora events and bind IDs to the frozen roster.
  - Derive adjudicated han/fu/yakuman from the copied engine's post-rule-cap WinResult, not bare evaluator output. Reuse `riichienv_core::yaku::get_yaku_by_id` for labels. Supply per-yaku values in this backend adapter: open-hand reductions for IDs `15,16,17,26,27,29`; bonuses `31–34` counted from completed physical hand/melds, actual indicators and kita; double-yakuman values use the captured rule flags. Compare summed values with adjudicated totals; unknown IDs/inconsistent values fail the new-record contract.
  - Obtain **base** payment using the installed `score::calculate_score` with `honba = 0` and actual player count, using the already-adjudicated result. For ordinary counted yakuman, cap scoring han at `13` just as the evaluator does; do not turn ordinary 26 han into two yakuman. `Score.total` is Win Points. Cross-check against adjudicated basic payment fields/actual responsibility-payment totals in tests; no frontend scoring and no delta substitution. Classification is backend-only: explicit yakuman uses final han/13; ordinary limits follow that library's thresholds/payment cap, including low-han mangan.
  - Settlement uses captured pre-resolution scores and real post-resolution scores/pot, checked subtraction, all actual seats. Preserve engine draw reason (including nagashi); no fabricated completed hand for a draw. Reuse `MatchMachine::build_result` for ranks and initial-seat tie break; validate participant/seat bijection, counts, unique `1..=n` ranks and final-score agreement. Keep legal zero/negative balances.
- [ ] **4. Run GREEN:** `cargo test -p double_riichi_core`. Include duplicate kinds/red winning tile, two winners sharing one discard, closed kan, 3p kita/ura, normal capped/countable/stacked yakuman, responsibility, final remaining-pot award, already-paid riichi, duplicate names, mismatched IDs, overflow and nonapplicable fu serialization. Assert `win_points != credited_delta` in a honba+pot fixture and exact ordinary totals: nondealer 4-han 40-fu Tsumo `8_000` (4p) / `6_000` (3p); no honba/deposit in either value.
- [ ] **5. Commit:** `git add crates/double_riichi_core/src && git commit -m "feat(core): record authoritative kyoku results"`.

## Task 3: Hold the ended table and block every machine entry point

**Files:** Modify `crates/double_riichi_core/src/{match_machine,projection}.rs`; tests in `match_machine.rs` and `tests/task4_projection.rs`.

**Interfaces:** `MatchMachine::held_kyoku(&self) -> Option<&KyokuResultRecord>`, `publish_kyoku_step(&mut self, step_index: usize) -> Result<Vec<GameEvent>, MatchError>`, `release_kyoku(&mut self) -> Result<Vec<GameEvent>, MatchError>`, and crate-private `take_persistence_events(&mut self) -> Vec<GameEvent>` (drains accepted resolution events exactly once). Add `MatchError::ResultsPending`; retain existing public signatures for project/action APIs. Projection gains optional `kyoku_results: PublishedKyokuResults`; Room history projections gain optional `authoritative_results: PublishedKyokuResults`.

- [ ] **1. Write RED** `held_results_do_not_open_decision`, `held_final_result_is_not_complete`, `result_projection_releases_one_winner`, `terminal_event_prefix_hides_combined_settlement`.

```rust
assert!(machine.current_decision().unwrap().is_none());
assert!(machine.legal_actions(next_dealer).unwrap().is_empty());
assert!(machine.result().is_none());
assert!(!machine.is_complete());
assert_eq!(public_step.wins.len(), 1);
assert!(public_step.settlement.is_none());
assert!(!serialized_public.contains("next-kyoku-sentinel"));
assert!(!serialized_public.contains("future-winner-sentinel"));
```

- [ ] **2. Run RED:** `cargo test -p double_riichi_core --lib held_`; expected current read/complete/automatic behavior to violate the gate.
- [ ] **3. Implement** a single held-Kyoku field in MatchMachine containing the ended table, full record, publication cursor, withheld canonical suffix and optional final MatchResult. Track complete-journal event count separately from `events()`'s public released prefix. Preserve ordinary nonterminal resolution behavior.
  - Freeze the ended table through the existing adapter; win steps use pre-settlement scores. Publish only typed current/prior Win Results. Nonwinner table hands remain masked by existing audience projection; a complete concealed hand in the result payload is published only at that winner's step.
  - Buffer terminal canonical Hora/Ryukyoku events until **settlement publication**: their deltas/scores can otherwise disclose combined settlement during the first win. Preserve exact MJAI fields/order when eventually released. Keep EndKyoku plus next StartKyoku/draw or EndGame withheld until final step completion. Earlier genuinely public discard/reach/call events remain available.
  - `project`, `current_decision`, `decision`, `open_decision`, `legal_actions`, `apply`, `submit_action`, timeout resolution and `events()` must not bypass the hold. Reads return frozen state/no Decision; Game Actions reject ResultsPending without aborting. `is_complete`, `status` and `result` do not expose the pending final result.
  - `publish_kyoku_step` validates contiguous progress and returns only newly released canonical events; published results retain earlier wins. `release_kyoku` clears the hold, releases the remaining suffix and only then exposes completion or allows a fresh next Decision/deadline. Abort/delete/shutdown drop the pending presentation without publishing speculative future state.
- [ ] **4. Run GREEN:** `cargo test -p double_riichi_core`. Test all audiences, serialization, direct action/timeout helpers, repeated reads, multi-Ron order, draws and final-match gates. Verify no captured full-state object is serializable and no stale queued action becomes a next-Kyoku action.
- [ ] **5. Commit:** `git add crates/double_riichi_core && git commit -m "feat(core): gate next kyoku behind result publication"`.

## Task 4: Server-controlled steps and idempotent Human confirmations

**Files:** Modify `crates/double_riichi_core/src/{results,room,lib}.rs`; extend `tests/task8_room.rs` and Room unit tests.

**Interfaces:** Add `RoomCommand::ConfirmResult { participant_id: ParticipantId, step_id: ResultStepId }` and constructor `RoomCommand::confirm_result(participant_id: impl Into<ParticipantId>, step_id: ResultStepId) -> Self`. Return existing `RoomResponse::Accepted(current_snapshot)` for successful, duplicate or stale confirmations. Add `RoomSnapshot.kyoku_end`, `RoomEvent::KyokuEndStepChanged { match_id: MatchId }`. Internal Room methods `start_result_step(&mut self, now: Instant)` and `advance_result_step(&mut self, now: Instant)` use the existing actor clock/serialization.

- [ ] **1. Write RED** `result_step_expires_at_five_seconds`, `confirmation_is_bound_to_step`, `reconnect_joins_next_cohort`, `zero_target_steps_preserve_order`. Use existing Tokio paused-time test support; no wall-clock sleeps.

```rust
assert_eq!(step.deadline_at_ms - step.published_at_ms, 5_000);
assert_eq!(at_4999ms.kyoku_end.as_ref().unwrap().id, original_id);
assert_ne!(at_5000ms.kyoku_end.as_ref().unwrap().id, original_id);
assert_eq!(after_stale_confirm.kyoku_end.as_ref().unwrap().confirmed, vec![]);
assert!(!current_step.required.contains(&reconnected_id));
assert!(next_step.required.contains(&reconnected_id));
```

- [ ] **2. Run RED:** `cargo test -p double_riichi_core --test task8_room result_`.
- [ ] **3. Implement** Room step sequence from the validated record: each winner, then one settlement; draw has one Draw step. Sample connected interactive seated Humans at each start; exclude built-in bots, MJAI/MCP, auto controllers and Spectators. Disconnect/auto/leave removes a target; reconnect does not add it mid-step. Match/sequence/index and authenticated participant bind every confirmation. Stale/duplicate/noncohort requests return current state without advancing another step.
  - Start deadline at the publication transition, not after network delivery or disk flush. Persist monotonic deadline and the corresponding wire start/deadline; do not recompute the duration on reads. Deadline and confirmation race in the same RoomActor queue and advance at most once.
  - Check the hold **before completion and automatic actions** in `advance_match`. Integrate its expiry into `next_wake`; `next_wake` must not open a next Decision. At zero targets iterate synchronously through steps in order, retaining the existing bounded automatic-loop protection and yielding for benchmark work as already implemented. At final step completion call `release_kyoku`, then normal next-decision/completion code. No extra confirmation gate on Final Standings.
  - Update public Room history only from released events/results, not the private complete record. `handle_decision_result` drains `take_persistence_events` into existing append/flush effects, while `DecisionResult.events` contains only released Live events. Never append the same canonical event again when a presentation step releases it. Keep initial OpenMatch events on their existing path, and count them in global event indices. Keep frozen match roster/characters independent of current Room edits/rematch.
- [ ] **4. Run GREEN:** `cargo test -p double_riichi_core`. Add deadline-minus-1ms/exact-deadline, two simultaneous confirms, resend, other participant, same step index in another Match/renchan, disconnect of last target, all-bot benchmark/no targets, missed ticks catching up, repeated snapshot/history reads, final→PostMatch→Ready/Rematch. Assert each released event/step occurs once and reconnect never creates a new five seconds.
- [ ] **5. Commit:** `git add crates/double_riichi_core && git commit -m "feat(room): synchronize kyoku result confirmations"`.

## Task 5: Apply the publication boundary to all transports

**Files:** Modify `crates/double_riichi_server/src/http.rs`; extend `tests/{task9_http,task13_compat,task14_mcp,task16_contracts}.rs`; inspect incumbent `double_riichi_mjai` and `double_riichi_mcp` projection adapters and modify them only if a bypass test proves necessary.

**Interfaces:** Human input adds `{"type":"confirm_result","step_id":{"match_id":"...","kyoku_sequence":1,"step_index":0}}`; participant identity comes from the authenticated socket, never this payload. Human snapshots/updates expose the Task 4 `kyoku_end`; projections carry only Task 3 released results. MJAI events retain canonical shape and order; no new gameplay action or MCP tool.

- [ ] **1. Write RED** `unpublished_results_are_absent_on_every_transport` and `human_confirmation_is_not_a_game_action`. Seed unique sentinels for future winner, nonwinner and next hand, plus distinct combined settlement values; assert absence from complete serialized HTTP public/private/history responses, Human WebSocket snapshots/action_resolved payloads, MJAI and MCP state/history/resources/notifications. Confirm the released winner is present where authorized.
- [ ] **2. Run RED:** `cargo test -p double_riichi_server --test task16_contracts unpublished_results -- --test-threads=1`.
- [ ] **3. Implement** the new HumanInput branch through RoomCommand. Serialize only released Room/Match views in `queue_room_event`, `send_snapshot`, history and summary notifications. `GetMatchEvents` must return the released prefix, not the persistence journal. Handle delayed queued events against current publication/revision without pairing a future raw payload with an older projected step. During the hold, old/next action submission is rejected; MJAI/MCP/bots have empty legal actions and no started deadline. Preserve bot wire semantics when buffered canonical events are released; do not send invented partial Hora deltas.
- [ ] **4. Run GREEN:** `cargo test -p double_riichi_server --test task9_http --test task13_compat --test task14_mcp --test task16_contracts -- --test-threads=1`. Add authenticated spectator/auto/bot attempts, unrelated/unknown step fields, delayed broadcasts/reconnect/resends, final-state summaries and rejected direct API actions. Check byte-level absence in nested fields, not only top-level DTO keys.
- [ ] **5. Commit:** `git add crates/double_riichi_server && git commit -m "feat(server): publish only released kyoku result data"` (include any proven adapter changes explicitly).

## Task 6: Persist authoritative records and join them to Replay

**Files:** Create `crates/double_riichi_server/migrations/0007_authoritative_results.sql`; modify server `src/storage.rs`, core `src/room.rs`, replay `src/{frames,reader,lib}.rs`; extend server `tests/task15_replay.rs`, replay `tests/task5_replay.rs`.

**Interfaces:** Add optional record to `RoomEffect::FlushKyoku` and persist it before acknowledging successful flush. Migration adds `match_kyoku_results(match_id TEXT NOT NULL REFERENCES matches(match_id) ON DELETE CASCADE, kyoku_sequence INTEGER NOT NULL, version INTEGER NOT NULL, end_event_index INTEGER NOT NULL, payload_json TEXT NOT NULL, PRIMARY KEY(match_id, kyoku_sequence))` and nullable `matches.authoritative_result_json` for the final validated existing MatchResult. Records store no raw captured nonwinner hands. `ReplayFrame.result: Option<WinResult>` and `.settlement: Option<KyokuSettlement>` are joined by exact event index; `ReplayView.result: Option<MatchResult>`. Add `attach_result_records(frames: &mut [ReplayFrame], records: &[KyokuResultRecord]) -> Result<(), ReplayError>` in `frames.rs` and export it.

- [ ] **1. Write RED** `saved_results_equal_live_authority`, `legacy_replay_omits_unknown_details`, `corrupt_result_metadata_is_not_legacy_missing`.

```rust
assert_eq!(loaded_record, captured_record);
assert_eq!(replay.result, Some(captured_final_result));
assert_eq!(hora_frame.result.as_ref().unwrap().winning_tile_index, captured_index);
assert!(legacy_frame.result.is_none());
assert!(invalid_version_or_duplicate_index.is_err());
assert!(!partial_or_failed_match_is_replay_available);
```

- [ ] **2. Run RED:** `cargo test -p double_riichi_server --test task15_replay saved_results -- --test-threads=1` and `cargo test -p double_riichi_replay --test task5_replay result_`.
- [ ] **3. Implement** version `1` result rows alongside the unchanged canonical writer. Flush canonical lines and insert validated record in the existing effect worker; a successful ack means both succeeded. Idempotent same-key/same-payload replay is allowed; conflicting duplicates fail. Finalization writes validated MatchResult in the existing completion transaction before marking availability; preserve DB match-player roster/character linkage. Ensure failed/incomplete cleanup includes result rows and new-format completed matches require all required records. Do not change the old completed-row constraint/migration history or infer absent old result fields.
  - Store the private complete canonical stream, including internally prepared but unoperated next-round events; Live prefix and writer indices are deliberately different. Join wins at their original Hora indices and combined settlement at EndKyoku; no repeated settlement for multi-Ron. Replay frames keep existing event order and Admin-only complete-hand access; no Live cohort/timer persisted as a playback wait.
  - Bound JSON/input sizes using incumbent Replay limits; validate IDs/seat/mode/index range, terminal boundaries, unique records and final result consistency. Unknown version, contradictory record or inaccessible artifact is unreadable/unavailable, not sample data. Missing metadata in a genuinely old record leaves optional values absent. Saved/unavailable status remains authoritative even on flush/rename/database/queue failures.
- [ ] **4. Run GREEN:** `cargo test -p double_riichi_replay` and `cargo test -p double_riichi_server --test task15_replay -- --test-threads=1`. Test 3p/4p multi-Ron/final draw, changed current Room names/characters, interrupted append/flush/DB/final rename, disabled saving, corrupt/missing files, unsupported version, duplicate index, final-score mismatch, initial-seat tie, cleanup/restart and Admin access control. Assert no new public replay access and no claimed match resumption after restart.
- [ ] **5. Commit:** `git add crates/double_riichi_core/src/room.rs crates/double_riichi_replay crates/double_riichi_server && git commit -m "feat(replay): preserve authoritative result records"`.

## Task 7: Consume results and server step timing without UI inference

**Files:** Create `frontend/src/game/results.ts` and `results.test.ts`; modify `game/{types,store,gameplay}.ts/tsx`, `api.ts`, `replay.tsx`; extend `game/gameplay.test.tsx`, `replay.test.tsx`, `tests/result-surfaces.spec.ts`. No table-quality prototype edits or unrequested visual redesign.

**Interfaces:** `decodeWinResult(value: unknown, mode: string): WinResult | null`, `decodeKyokuEndStep(value: unknown, mode: string): KyokuEndStep | null`, `decodeFinalStandings(value: unknown, mode: string): FinalStandings | null` in `results.ts`. TypeScript uses the exact snake_case serialized Rust fields/tags. `FinalStandings` is the validated existing MatchResult, not a newly ranked client structure. Store adds `confirmResult(stepId: ResultStepId, transport?: Transport): boolean`, transmitting only `confirm_result` and keeping local confirmation separate from PendingAction.

- [ ] **1. Write RED** `renders_only_authoritative_winning_position`, `does_not_substitute_delta_or_rank`, `reconnect_uses_server_remaining_time`, `legacy_replay_keeps_missing_fields_absent`.

```ts
expect(screen.getAllByTestId("winning-tile")).toHaveLength(1);
expect(screen.queryByText("0 fu")).not.toBeInTheDocument();
expect(screen.queryByText("99999")).not.toBeInTheDocument(); // delta-only fixture; not Win Points
expect(sentPayload).toEqual({ type: "confirm_result", step_id: currentStep.id });
expect(sentPayload).not.toHaveProperty("action_id");
expect(decodeFinalStandings(duplicateRanks, "3p-red-east")).toBeNull();
expect(reconnectedCountdownMs).toBeLessThanOrEqual(serverRemainingMs);
```

- [ ] **2. Run RED:** `npm test --prefix frontend -- src/game/results.test.ts src/game/gameplay.test.tsx src/replay.test.tsx`.
- [ ] **3. Implement** bounded decoding: integers/roster bijection/participant IDs/ranks/winning position/applicable fields; legal zero/negative points stay valid. Replace `visibleWinnerHand`, event/delta-derived evaluation and ResultsPanel index/name/rank fallbacks with validated server values. Missing fields are omitted, not replaced by current hand, sample numbers or extra explanatory copy. Select the sole certified rank 1 and recorded roster character for portrait.
  - Live Continue is a Result Confirmation. Live local Pause must not freeze server progress or reopen gameplay. Use server `remaining_ms` mapped to a local monotonic baseline, never a fresh five seconds; cap by subsequent authoritative updates. Confirmation waits on the current display. Settlement animates before→after over the first **3 seconds**, holds to the **5-second** deadline; reconnect catches up to the existing phase, Reduced Motion uses final values immediately, server early advance wins. Audio/images cannot start/reset deadlines.
  - Spectator dismiss is local: ended table plus approved waiting text; no confirmation request or next-result inference. Each new server step reopens current content. Preserve no-glow result tiles, ordinary/explicit-yakuman distinctions, ura as a yaku value only, keyboard/focus/labels and existing composition/fallbacks.
  - Replay uses saved WinResult/settlement/final MatchResult, original event indices and recorded roster; no confirmation transport. Keep manual next/seek and speeds `0.5/1/2/4/8/16/32/64×`, including discontinuity cancellation/end. Unknown old fields stay absent; never manufacture the fourth player in 3p.
- [ ] **4. Run GREEN:** the RED command, then `npm run typecheck --prefix frontend` and `npm run test:browser --prefix frontend -- result-surfaces.spec.ts`. Include corrupted/unknown tags, duplicate names/ranks/IDs, old partial roster, same-kind red winning tile, open/closed kans, two wins/one settlement, keyboard focus, missing portrait/audio refusal, Reduced Motion, spectator dismiss/reopen, reconnect midway through count/hold, expiry while assets load, rematch, Replay seek/high speeds/end and **zero Game Action/confirmation requests in Replay**.
- [ ] **5. Commit:** `git add frontend/src frontend/tests/result-surfaces.spec.ts && git commit -m "feat(ui): consume authoritative live and replay results"`.

## Task 8: Verify the actual lifecycle and record the remaining release gates

**Files:** Create `frontend/tests/kyoku-results-real-server.spec.ts`; extend `crates/double_riichi_server/tests/task16_contracts.rs`; existing real-server browser configuration. Test-only fixture setup follows incumbent real-server tests; no test-only public gameplay route.

**Interfaces:** No new production interface. Consume Tasks 1–7 exactly; reuse existing real-server harness/auth/session/storage setup.

- [ ] **1. Write RED** `real_match_result_capture_publication_save_and_reconnect`: drive offered legal action IDs through actual adapter→Room→transport→UI→save→Admin Replay in 3p and 4p. Exercise multi-Ron, draw, final pot and tied final standings using bounded deterministic existing test fixtures/seeds. Record actual commands/events, not a synthetic frontend result projection alone.
- [ ] **2. Run RED:** `npm run test:browser:real --prefix frontend -- kyoku-results-real-server.spec.ts`; assert no future/nonwinner sentinel appears in **any** captured public response while waiting, no next action/deadline begins, all confirmations/deadlines advance once, and saved records equal Live authority.
- [ ] **3. Implement only missing test harness wiring** and minimal production fixes owned by the earlier tasks; add a snapshot/evaluation timing measurement against existing seeded baseline, report delta/recorded machine and maximum match-log size. Do not mask overhead by lowering rendering quality or remove the correctness gate. Run resize/3p/4p/Live/Replay result overlap and fallback checks at `1024×600`, `1280×720`, `1600×900`, `1920×1080` (16 cases), plus DPR 2 and unsupported-size restoration; real Windows Chrome/Edge, human inspection and target-GPU budgets remain the later #11 gate.
- [ ] **4. Run GREEN:**

```sh
cargo fmt --all -- --check
cargo test --locked --workspace
npm run typecheck --prefix frontend
npm test --prefix frontend
npm run test:browser --prefix frontend
npm run test:browser:real --prefix frontend -- kyoku-results-real-server.spec.ts
```

Expected: all pass, no skipped privacy/lifecycle scenario represented as acceptance. Run incumbent `just check`/contract/real-server setup commands where their prerequisites are installed; record any missing prerequisite as a blocker, not a pass. No dependency/cache-source or prototype diff; no push/merge/deploy.

- [ ] **5. Commit:** `git add frontend/tests/kyoku-results-real-server.spec.ts crates/double_riichi_server/tests/task16_contracts.rs && git commit -m "test: verify authoritative result lifecycle and privacy"` (include explicitly identified harness fixes).

## Approval / handoff

First approve this plan, especially the private-copy stop-boundary candidate and its Task 1 compatibility gate. If that gate fails, stop for a decision; no dependency patch is preapproved. Execute on an isolated branch only after approval, using the user-authorized execution/review method. Native implementation is the practical default while the subagent host is broken, but approval of direct investigation is **not** approval of implementation or a waiver of later review.

This deliverable is one dependent vertical slice, not unrelated subsystem plans: capture → gate → transport → saved metadata/Replay → display. Approved whole-table/sheens/image integration and the remaining visual/browser/GPU acceptance require a subsequent plan; all #11 gates still block production release.
