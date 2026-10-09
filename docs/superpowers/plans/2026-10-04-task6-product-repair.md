# Task6 Product Repair Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Finish the existing authoritative-result persistence/Replay implementation with bounded reads and writes, coherent replay views, live round-trip evidence and honest native-fault disposition.

**Architecture:** Keep the existing Room actor, effect worker, canonical ReplayWriter, SQLite storage and Replay attachment boundary. Extend the existing private read transaction to the whole replay view; reuse LimitedJsonWriter and existing failure/cleanup paths. Do not redesign Mahjong rules or add services, dependencies or a second scheduler.

**Tech Stack:** Existing Rust/sqlx SQLite WAL/Tokio/serde stack; pinned `riichienv-core = "=0.4.10"`; installed CDB only if separately approved below.

**Spec:** `docs/superpowers/plans/2026-10-02-authoritative-kyoku-results.md`, especially Task6 and Global Constraints; `docs/superpowers/specs/2026-10-02-pre-action-result-findings.md`; approved Issues #10, #11 and #12 referenced there. This supplements Task6, not Tasks1–5 or the UI/table design.

## Global Constraints

- Work only in `.worktrees/authoritative-results`, branch `feat/authoritative-kyoku-results`; baseline HEAD `1258dbadba8f09283f555874fc9f5261df0a65e1`. Preserve existing Task6 changes and historical receipts.
- `.worktrees/prototype-dora` remains clean at `30329420f9ea8451bada652c02c8bc827114ae48`.
- New admissions use authoritative version `1`; only genuinely historical NULL marker + NULL final JSON + zero result rows is legacy. No downgrade on corruption or failure.
- Canonical `.mjson` format and private event indices stay unchanged. Preserve exact physical tiles, actual ordered winners, draw adjustment evidence, held-score/pot movement, frozen initial-seat roster and final MatchResult; no inferred authority or synthetic production fixtures.
- `MAX_DECOMPRESSED_REPLAY_BYTES = MAX_REPLAY_FRAME_BYTES = 64 * 1024 * 1024`; `MAX_REPLAY_EVENTS = 100_000`. Reject greater-than-limit input before unbounded materialization/serialization; retain exact-limit behavior.
- Admin-only complete Replay access; no public/private wire changes, future-result disclosure, dependency/vendor/cache/credential changes or gameplay-rule changes.
- One serial writer; separate read-only reviewer. Preserve stored writer `openai-codex/gpt-6-sol:medium` and reviewer `openai-codex/gpt-6.1-sol:high` contracts. Check exact run eligibility and capabilities before resume; no silent agent/model/tool/runner fallback.
- Keep feature-local `.pi-lens.json` mutation controls disabled and diagnostics available; never commit/publish this operational file or private logs/DBs/dumps. Cached Git `M` alone is not drift: inspect actual diff and bytes.
- Source-audit each focused test, then `--exact --list` must select exactly one before its serial execution. No broad name filters. Approved full-package gates are explicitly separate, serial, unfiltered suites.
- Any native fault, link/resource failure, unexpected source mutation, missing runtime-bound writer report or runner/quota failure stops that lane; preserve evidence and report the exact failure. No assertion suppression, stack/timeout inflation, blind retry, settings/cache deletion or alternate execution protocol.
- Parent owns acceptance and commits. Children cannot commit, push, update Issue acceptance, start Task7+, merge, deploy or release. Historical failed/tainted runs remain failed/tainted.

## Authority and renewed budget

The user's `修正プランを作成＆実行` authorizes a consolidated repair direction, not unlimited corrections or exceptional system changes. **Human review approved `修正＋必要時CDB1回 (Recommended)` on 2026-10-04.** Execute Tasks1–4 continuously, with one initial implementation per source task and at most **two scoped correction/re-review passes total** for this supplemental plan. Old passes1–7 stay exhausted historically; this is a new, explicit budget. A real blocking defect at the new cap remains blocked, not waived.

Ordinary local source edits, isolated fresh test databases, Cargo build/test/fmt checks, and independent source review are the proposed implementation scope. No privileged WER operation, registry/ACL/policy/environment change, installation, ProcDump invocation/EULA acceptance, original forensic DB mutation or uploading evidence is included.

An optional approval covers **one additional CDB observation only**, specified in Task4; source repairs do not depend on selecting it. No repeated micro-approval requests for ordinary in-scope tests. Exceptional new operations or a broken causal hypothesis are escalated once with the consolidated remaining blockers.

## Current evidence and causal separation

- The private `load_authoritative_result` metadata/stats/rows snapshot has scoped acceptance; that is not a whole-view snapshot.
- `load_replay` selects the summary independently; `reconstruct_replay` selects auxiliary records; `load_players` reads stats then rows; only afterward does the private result transaction start. Thus a concurrent commit can mix different DB versions within one returned view.
- `load_authoritative_result` selects the full final JSON before checking its byte length. `complete_match` materializes Room rows before count/aggregate-byte checks and clones ranked records before bounding them. `persist_kyoku_record` and completion serialize into an unbounded String before rejecting its size.
- `finalize_room_replay` already finalizes the writer before the completion transaction and sends its outcome afterward. The worker's FlushKyoku already flushes canonical data and persists the record before acknowledgment. These are **validation targets, not established defects**; fix only a demonstrated failing behavior.
- Original saved-Room AV `0xc0000005`/shell139 has no causal stack/seed. CDB/instrumented passes did not repair or localize it. Storage coherence/size fixes must not be described as its repair without evidence. Actor and compat historical AVs (#47/#49) remain separate, not silently closed by this plan.

## Review Focus

1. Concurrent metadata/roster/auxiliary/result mutation: one coherent old or new view, never mixed details (Task2).
2. Oversized final JSON, result-row count/aggregate bytes and borrowed ranked records: reject before materialization; preserve rollback and caller values (Task1).
3. DB or canonical/final-rename failure: never acknowledge durable success or advertise saved availability; cleanup remains retryable (Task3).
4. Modern missing/corrupt metadata versus genuine historical absence: no fabricated legacy success or ranks (Tasks1–3).
5. Real Room/standalone saved authority versus current Room configuration: capture actual values and compare every persisted result; no fixture-based equality substitute (Task3).

## File map and interfaces

- Modify `crates/double_riichi_server/src/storage.rs`: bounds, single-connection read path, persistence tests; no module split.
- Extend `crates/double_riichi_server/tests/task15_replay.rs`: live saved-authority and lifecycle/API checks.
- Extend `crates/double_riichi_server/src/compat.rs` tests only for standalone real-authority round trip; production edit only with a failing test and identified cause.
- Extend `crates/double_riichi_replay/tests/task5_replay.rs` only if an uncovered attachment/legacy assertion needs a check.
- Existing core/Replay product files, genuine fixtures and `migrations/0007_authoritative_results.sql` are retained for whole-Task6 review; no speculative further changes.
- Operational evidence goes into a new ignored workspace `.superpowers/sdd/2026-10-04-task6-product-repair/`; historical workspace is read-only evidence, not rewritten or deleted.

## Focused-test command contract

For each named server-lib test below, in the feature worktree:

```sh
cargo test --locked -p double_riichi_server --lib storage::tests::<name> -j1 -- --exact --list
cargo test --locked -p double_riichi_server --lib storage::tests::<name> -j1 -- --exact --test-threads=1
```

Expected: list selects one exact qualified test; behavioral RED fails the specified assertion (not compilation/link failure); GREEN selects and passes one with Cargo exit0. Integration tests substitute `--test task15_replay <name>`; compat-lib tests substitute `compat::tests::<name>`. Record full logs, exits, source/diff hashes, test counts and protected-file hashes before/after. Existing historical RED is not fresh RED for a new behavior.

### Task 1: Bound authoritative storage before allocation

**Files:** `crates/double_riichi_server/src/storage.rs`, functions `load_authoritative_result`, `persist_kyoku_record`, `complete_match`, `LimitedJsonWriter`, and `storage::tests`.

**Interfaces:** Keep public storage completion APIs unchanged. Add private `fn bounded_result_json<T: serde::Serialize>(value: &T) -> Result<String, StorageError>` using existing LimitedJsonWriter at `MAX_DECOMPRESSED_REPLAY_BYTES`; overflow maps to `ReplayTooLarge`, serialization failure to the incumbent metadata error. SQLite length/count/aggregate queries run on the same connection/transaction as their subsequent payload queries. Ranked records remain borrowed for validation; no eager `records.to_vec()`.

- [ ] Write behavioral RED tests `authoritative_final_json_is_bounded_before_decode`, `authoritative_completion_bounds_room_record_rows_before_decode`, `ranked_completion_bounds_borrowed_records_and_rolls_back`, and `bounded_result_json_rejects_overflow`.

  Assertions: oversized final JSON (`64 * 1024 * 1024 + 1` bytes), `100_001` record rows, or aggregate record bytes over `64 * 1024 * 1024` return `ReplayTooLarge`, not a JSON-corruption fallback. For final JSON include an oversized SQLite TEXT payload containing an invalid UTF-8 byte: length rejection must precede String decoding (the baseline selects/decodes it first). Invalid, valid-UTF-8 JSON below the ceiling still returns `ReplayCorrupt`. A ranked count/aggregate rejection leaves status `writing`, NULL final JSON/points and zero newly inserted records; caller records/result remain equal to their before values. Serializer output exactly at the ceiling succeeds, one byte over fails. Use SQLite-generated payloads/rows and the existing serializer for small boundary checks, not hundreds of huge duplicated fixture values. Before the helper-specific RED, extract the incumbent unbounded serialization into the named helper without changing behavior, so an undefined function is never counted as behavioral RED.
- [ ] Source-audit/list/run each RED serially; preserve already-GREEN count/aggregate cases as observations, not allocation-order proof. For ranked early rejection, use valid admission/mode/result and the oversized borrowed-record input with only its owned registered canonical artifact changed to invalid JSON: verify whether the baseline reports ReplayCorrupt before the required early ReplayTooLarge. For Room aggregate early rejection, use an oversized SQLite TEXT payload containing invalid UTF-8 and verify the actual baseline decode error. Require genuine observed rejection-order RED before the corresponding guard/refactor; never force a contradicted hypothesis. Independent source review additionally verifies guards precede fetch_all/cloning: error precedence alone is not an allocation measurement. See the recorded initial-task clarification `.superpowers/sdd/2026-10-04-task6-product-repair/task-1-parent-rejection-order-ruling.md`.
- [ ] Implement bounded serialization; inspect SQLite BLOB byte length/count/aggregate metadata **before** selecting payloads or Room `fetch_all`, retaining checked accumulation on materialized bounded rows. Check ranked slice count and bounded aggregate serialization before cloning/insertion. Reuse current transaction rollback and validation; do not add a second schema/version. If the proposed adversarial RED does not expose the expected boundary, stop that proposed fix and report what actually happened; do not manufacture a failing assertion.
- [ ] Run all four new exact tests and the existing exact idempotency, completion consistency, corruption, rollback, ranked saved-round-trip and persisted-mode tests. Run `git diff --check` and file-local rustfmt check. Independent task review must inspect the actual allocation/query order, not infer it from error codes alone.
- [ ] Save the reviewed delta/report without committing; parent later commits the complete coherent Task6 checkpoint.

### Task 2: Read the whole Replay view from one SQLite snapshot

**Files:** `crates/double_riichi_server/src/storage.rs`, replay-read helpers/callers and storage snapshot tests.

**Interfaces:** Keep `load_replay(&self, match_id: &str) -> Result<ReplayView, StorageError>` unchanged. Change private `load_replay_view(&self, match_id: &str) -> Result<ReplayView, StorageError>` to own one read transaction, select the completed summary/path within it and pass its connection through all DB-dependent helpers. Private signatures are `async fn load_players(&self, match_id: &str, connection: &mut sqlx::SqliteConnection) -> Result<Vec<ReplayPlayer>, StorageError>`, `async fn load_auxiliary(&self, match_id: &str, connection: &mut sqlx::SqliteConnection) -> Result<Vec<AuxiliaryRecord>, StorageError>`, and `async fn load_authoritative_result(&self, match_id: &str, game_mode: &str, players: &[ReplayPlayer], frames: &mut [ReplayFrame], connection: &mut sqlx::SqliteConnection) -> Result<Option<MatchResult>, StorageError>`. `async fn reconstruct_replay(&self, game_mode: &str, replay_path: &str, file_size: i64, auxiliary: &[AuxiliaryRecord]) -> Result<Vec<ReplayFrame>, StorageError>` consumes the selected auxiliary slice rather than opening another pool query. Update startup validation, `load_replay`, list availability and direct tests together; no nested pool transaction.

- [ ] Add RED `replay_view_keeps_summary_roster_auxiliary_and_results_in_one_snapshot`. Use an owned WAL database and a deterministic test-only connection-release barrier, not sleep as race evidence. Commit a second internally valid view while the baseline read pauses after the first summary query.

  Assertions: the returned tuple of summary/players/auxiliary-derived frames/wins/settlement/final result equals the complete prior or complete next tuple, never a mixture or corrupt hybrid. Verify the concurrent commit occurred. If the fixed read finishes before that commit, record that fact rather than claiming forced mid-transaction interleaving. Preserve the private-snapshot regression and its pristine-frame assertion.
- [ ] Source-audit/list/run the new RED; document the precise mixed-version assertion and first connection-release boundary.
- [ ] Implement one read transaction across header, auxiliary/roster stats and rows, private result metadata/stats/rows and validation. All returns release/rollback correctly. Reconstruct from the same path/mode/snapshot values; retain Task1 bounds. After a deleted row, return the incumbent not-found/unavailable outcome, never fabricated legacy success.
- [ ] Add/run exact `replay_file_disappearance_never_returns_a_fabricated_view`: delete only the owned finalized artifact while metadata remains; assert `ReplayUnavailable` and unavailable listing. A SQL snapshot does **not** atomically snapshot the filesystem; do not add a global lock to pretend otherwise.
- [ ] Run both new tests, the updated private snapshot test and existing modern/legacy/corruption/read-boundary tests serially; independently review the entire call chain and all updated callers. Save reviewed delta/report; no commit yet.

### Task 3: Prove actual saved authority and durable failure behavior

**Files:** server `tests/task15_replay.rs`; `src/compat.rs` tests; incumbent worker/Replay tests as needed. Production `storage.rs`/`compat.rs`/ReplayWriter edit requires its own behavioral RED.

**Interfaces:** Existing `RoomEffect::FlushKyoku { match_id, record, completion }`, `FinalizeMatch { match_id, result, ..., completion }`, `spawn_room_effect_worker(...) -> JoinHandle<()>`, Room snapshots and `complete_ranked_match` are authority boundaries. Use a test-only forwarding receiver to capture actual emitted records/final result without replacing the real worker or inventing data.

- [ ] Implement `saved_results_equal_live_authority` for actual 3p and 4p Room runs. Capture the genuine FlushKyoku records and FinalizeMatch result, forward unchanged effects/completions to the real worker, then compare DB records and authenticated API frames/final result against those captured values. Compare every winner/index/settlement/roster field, not just final points. Change current Room names/characters after completion and assert saved identity is unchanged. Join/shut down owned forwarding/worker tasks and close DB before cleanup. Record seed where actually available; do not claim random workloads identical.
- [ ] Add `compat::tests::ranked_saved_results_equal_live_authority` using an actual seeded MatchMachine/standalone result path, collected genuine records and actual CompatReplay finalization; verify persisted values, not a hand-written MatchResult. Existing legal actions only; no gameplay or production seed API changes.
- [ ] Add/strengthen exact `legacy_replay_omits_unknown_details`, `corrupt_result_metadata_is_not_legacy_missing`, `kyoku_flush_ack_waits_for_record_commit`, and `finalization_failure_never_advertises_saved_replay`.

  Assertions: historical NULLs keep result/settlement absent; modern absent row, unknown version, conflicting sequence/index and final-score mismatch fail unavailable without downgrade. A failed owned SQLite insert trigger makes FlushKyoku completion Err and saved availability false. Owned canonical-write/final-rename/DB-completion failure cases never yield a successful durable completion; retryable cleanup retains registered metadata when file removal fails. Success is observable only after record persistence and final artifact/completion commit. Saving disabled creates no result rows or advertised artifact. Reuse existing exact tests for already-covered cases rather than duplicate them.
- [ ] Audit/list each exact test before execution. For any uncovered failed assertion, identify the actual worker/writer/cleanup cause, make its minimum fix, rerun its exact test; no speculative filesystem-transaction redesign.
- [ ] Source-review existing 3p/4p multi-Ron/final-draw genuine fixture tests and rerun their exact names plus affected attachment/size-rollback tests. These checks supplement, not replace, live saved equality. Independent review covers authority provenance, disabled-saving, Admin access, worker shutdown and all requested failure boundaries.
- [ ] Save reviewed evidence/delta, preserving original native-fault evidence. Do not declare an AV repaired from successful fixture/IO tests.

### Task 4: Exact current-source gates, native disposition and parent checkpoint

**Files:** new ignored evidence workspace and final parent report; no production changes except a separate demonstrated native-root-cause fix within the renewed budget.

**Interfaces:** Consume the reviewed Tasks1–3 source package plus pre-existing Task6 dirty product files/migration/genuine fixtures. Parent checks the final complete package against original Task6, not just the latest delta.

- [ ] Preserve current HEAD/diff/untracked fixture+migration hashes, protected transport/config hashes and original crash EXE/PDB/source evidence before any build. Record current builds separately; never overwrite preserved original evidence or reuse the instrumented binary as a normal gate.
- [ ] Source-audit and exact-list `completed_room_match_is_visible_through_admin_replay_api`, rebuild restored current source serially, record current EXE/PDB/source hashes, then perform **one** normal exact test run on the changed source. This is current implementation validation, not an unchanged retry of the historical fault. Stop on native/link/resource/filter failure; no automatic retry.
- [ ] **Optional CDB observation, only if selected in the human review:** one launch of the coherent current saved-Room EXE/PDB pair **only if the new normal exact gate actually faults**. A passing gate does not trigger another launch of the preserved original binary; retain that historical evidence without a blind repeat. Before launch, confirm exact test listing selects one, no concurrent heavy job, installed CDB identity, local ignored output directory created fresh, original hashes and source pairing, sufficient disk for a single full dump (at least 4 GiB free; stop if unavailable), and unchanged application/test deadlines. The observer wall-clock cap is 1100 seconds, the previously recorded observation cap, never enlarged; it is not a gameplay/test timeout. Use only per-process CDB commands; no AeDebug/IFEO/WER, symbol downloads, ProcDump/EULA, environment/policy edits or automatic observer retry. Capture first-chance exception record and stack then continue appropriately; on an actual unhandled AV/illegal instruction preserve exception/context/registers/all-thread stacks/modules and at most one private full dump. Preflight/timeout/capture failure stops and preserves the exact receipt. A debugger pass, no fault or no dump does not close the original AV. No enlarged target stack/timeout, assertion suppression or diagnostic pass used as acceptance.
- [ ] If actual fault evidence supports a production cause, write one reproducible regression, make the minimal causal repair and run a fresh **normal**, uninstrumented covering gate. If no causal evidence exists, leave native disposition UNKNOWN/BLOCKED; do not relabel it environmental or fixed. Actor/compat AVs remain separately tracked.
- [ ] On a coherent, non-faulting tree, run serial unfiltered package gates: `cargo fmt --all -- --check`; `cargo test --locked -p double_riichi_replay -j1 -- --test-threads=1`; `cargo test --locked -p double_riichi_core -j1 -- --test-threads=1`; `cargo test --locked -p double_riichi_server -j1 -- --test-threads=1`. Stop a gate on failure; classify assertion, compiler/link/resource, native and infrastructure failures separately. These renewed ordinary gates are not permission for settings repairs or unlimited re-execution.
- [ ] Obtain separate independent complete-Task6 review of the exact final source package, all new/existing evidence and original Task6 coverage. At most two scoped correction/re-review passes total; parent checks physical/runtime-bound reports and final hashes. File-only writer must physically write/read-verify the new mapped artifact; reviewers use complete inline/runtime-persisted verdicts, with no fictitious direct-write claim.
- [ ] Parent records either **accepted complete Task6** or **reviewed source improvements with Task6 still blocked**. Full acceptance requires complete durable/live/corruption/legacy/privacy gates and defensible native disposition; no automatic fault waiver. Only then stage explicit product paths and this plan for local `feat(replay): preserve authoritative result records` commit, recording exact reviewed diff/HEAD. Exclude `.pi-lens.json` and all operational/forensic artifacts. Do not mark Issue21 complete, start Tasks7–10 or push/PR while Task6 acceptance remains blocked.

## Self-review and execution handoff

- Spec coverage: original Task6 persistence, genuine authority, canonical/private indices, roster/final consistency, size bounds, legacy/corruption, durable ack/failure/cleanup and Admin-only Replay are assigned above. Tasks7–10, actor/compat unknown fault diagnoses and hardware/human acceptance remain explicitly outside this supplemental checkpoint, not claimed complete.
- Step scan: named tests, assertion values, entry points, command contracts and reviewed deliverables are defined. Success cases already passing need fresh coverage, not manufactured RED; production edits require a genuine behavioral RED.
- Type consistency: public APIs remain unchanged; Task2 owns private connection-signature changes and updates every caller/test atomically. Task1 bounds remain applicable on that connection. Task3 captures existing effects without changing authority.
- Review focus: all five input classes have tests in their owning tasks; SQL/file consistency is explicitly distinguished.
- Shared surface: Tasks1/2 share storage-read helpers (bounds retained); Tasks1/3 share completion (rollback/authority retained); Tasks2/3 share view assembly (same DTO/identity); Tasks1–3/4 share final package (exact source pairing). Each task ends in its own source review before its dependent task begins.
- Ruling: prioritize demonstrated bounds/snapshot defects and missing live proof over more ProcDump preparation — these changes directly advance Task6 without exceptional system permissions — cost if wrong: reversible source/test rework; no claim of native repair.
- Ruling: keep the existing writer/reviewer method and ask for one consolidated plan approval, not renewed approval per Cargo command — writing-plans requires human review while the user requested execution — cost if wrong: one approval delay, rather than silent exceptional authority or repeated stalls.
- Ruling: SQLite snapshot coherence does not justify a filesystem/global-lock redesign — unavailable artifact errors remain truthful — cost if wrong: a demonstrated lifecycle defect needs a scoped causal fix, not fabricated consistency.

**Human review approved:** source repairs + ordinary validation/review + the one specified CDB observation, only if a new normal exact saved-Room gate actually faults. No discretionary between-task confirmation prompts. Safety/infrastructure stops retain actual partial work and report one consolidated blocker.
