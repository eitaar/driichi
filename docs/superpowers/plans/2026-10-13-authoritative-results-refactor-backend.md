# Authoritative Results Backend Refactor Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. 実行方法はこの計画の確認後に選ぶ。現時点では実装・委任を開始しない。

**Goal:** 公開・保存契約を変えず、不要な結果保持、コピー、同一境界内の二重検証とRustテスト重複を削除する。

**Architecture:** 新しい層や型階層は作らず、既存MatchMachine、EngineAdapter、Room、Storageの中で削る。DB roster照合はStorage、結果domainとcanonicalイベント照合は既存Replay joinが担う。

**Tech Stack:** Rust 1.95.0、riichienv-core =0.4.10、既存Tokio/SQLx/SQLite、Cargo workspace。

**Spec:** `docs/superpowers/specs/2026-10-13-authoritative-results-refactor-design.md`（承認済み）、項目8、10–15、6。

## Global Constraints

- 新依存・version変更・汎用test framework・fixture class階層・新flagなし。挙動/麻雀ルール/API/保存schema/MJAI互換を変えない。
- 5000msの公開期限、確認の冪等性、未公開結果/次局Decisionの秘匿、Admin Replay権限、入力/保存の上限・transaction・失敗処理を維持する。
- 画質/DPR[1,2]/AA/2048shadowと中央表示/DOM自手牌/Reduced Motion/画像≤10MB・≤16MP・centered cover・遷移reset/fallbackも維持する。
- cold reconnect、compound全体実行、旧表示との画素一致、Chrome/Edge、実GPUと人間の目視は未完了を完了扱いにしない。
- 証拠・`.superpowers`・target junction・prototypeを削除しない。dirty worktreeをreset/cleanしない。
- event batching、validation-only公開API、serde schema変更、依存更新、engine capture方式の変更は対象外。

## Review Focus

1. Winが0件のDrawでも公開前release不可、精算公開後は一度だけreleaseできる（Task B1）。
2. 次局へ進んだ実engineのround情報を終局snapshotへ混ぜない（Task B3）。
3. 内部的に整合する偽participant IDや短い精算を実rosterとして受け入れない（Task B5）。
4. recordあり/marker破損を旧Replayへ降格せず、allocation前の上限とrollbackを維持する（Task B4）。
5. Append→EndKyoku→Flush→futureの順序とfull queue時のRoom/benchmark別の失敗処理を維持する（Task B6）。

## 実行場所・共通手順

- 製品コードと下記コマンドのcwdは **`.worktrees/authoritative-results`**。基準はHEAD `b07b1a9` と現在の未コミット変更。計画文書はrootに置くがroot mainは実装場所ではない。
- 初回に `git status --short`、`git diff --stat`、`git ls-files --others --exclude-standard` を記録し、sole writerと既存移行差分の所有を確認する。未commitのWorker移行をresetせず、commitへ含める扱いは着手前に確認する。
- 各taskは対象symbolと全callerを再読→既存試験のbaseline→必要な小さい試験→最小変更→同じ試験→差分点検。純粋な削除では既存挙動試験が変更前にもPASSしてよい。新規APIの試験は未実装でRED、既存不具合の試験は期待assertでREDを確認する。無理に製品コードを壊してREDを作らない。
- 各taskの最後に検証結果を記録。commitの許可・基準差分の扱いが確定している場合のみ、列挙したファイルの**今回のhunk**をstageして `git diff --cached` を点検し、task単位でcommitする。`git add .` は使わない。失敗/未実行を完了にしない。
- 前半のB1→[UI計画F1](2026-10-13-authoritative-results-refactor-frontend.md)→B2–B7で状態/DTOを先に削る。その後B8–B10、UI計画F2、[ブラウザ計画](2026-10-13-authoritative-results-refactor-browser.md)へ進む。3編の製品インターフェース依存はなく、この順序は承認設計の優先順位である。

---

### Task B1: MatchMachineの保持状態を減らす（設計8）

**Files:** Modify `crates/double_riichi_core/src/match_machine.rs`、`crates/double_riichi_core/src/room.rs`。Test: 同ファイル内tests、`crates/double_riichi_core/tests/task8_room.rs`。

**Interfaces:** `pub(crate) fn next_kyoku_step_index(&self) -> Option<usize>` をMatchMachineへ追加。private field `last_settlement_scores: Option<Vec<i32>>` だけが直近精算後得点を保持。公開 `held_kyoku()` とpersistence drainの署名は変更しない。

- [ ] `next_kyoku_step_index_tracks_publication_and_release` を既存 `first_held(mode)` で追加。assertは未公開 `Some(0)`、各publish後 `Some(index + 1)`、公開前releaseは `ResultsPending`、release後 `None`。既存Draw/二重ロンの試験でも最後の公開までrelease拒否を確認する。
- [ ] `cargo test --locked -p double_riichi_core --lib next_kyoku_step_index_tracks_publication_and_release` を実行。getter未実装のREDを確認する。
- [ ] `results` Vec/getter/pushを削り、record確定時にafter得点だけ保存する。`settlement_published` を削り、公開とreleaseを `next_step > record.wins.len()` に揃える。Roomのstep index取得は新getterを使い、Win prefixのcloneを避ける。
- [ ] 同moduleで `final_score_mismatch_still_aborts` を追加。既存 `complete(mode)` のmachineの `last_settlement_scores` を全0へ変え、`record_events(Vec::new())` が `MatchError::Aborted`、`result()` がNoneになることをassert。全局recordを削ってもこの照合は残す。
- [ ] `cargo test --locked -p double_riichi_core --lib match_machine::tests` と `cargo test --locked -p double_riichi_core --features task8-test-support --lib match_machine::tests` を実行。全選択試験PASSかつ件数>0。公開prefix、Draw、単ロン、二重ロン、最終得点を維持する。
- [ ] `cargo test --locked -p double_riichi_core --test task8_room` を実行し履歴/再接続/保存との接続を確認。未使用の古いTask注記のみ削除し、使用中のpersistence drainは残す。差分点検・条件付きcommit: `refactor: trim held kyoku state`。

### Task B2: 捕捉済みWinResultの二重Mapを削る（設計10）

**Files:** Modify/Test `crates/double_riichi_core/src/engine.rs`。

**Interfaces:** private `fn state_win_results(state: &GameStateVariant) -> &HashMap<u8, riichienv_core::types::WinResult>`。`EngineKyokuEnd` から `win_results` fieldを削除し、`pre_action`、`ended_copy`、実after得点/potは残す。

- [ ] `cargo test --locked -p double_riichi_core --lib capture` でbaselineを保存する。`pre_action_capture_survives_next_round`、`simultaneous_capture_uses_one_resolution` は削除しない。
- [ ] helperを追加し、finish_applyのwinner数照合とresult_recordの取得を借用へ変える。capture constructor、破損Draw constructor、テストの `.win_results` 参照をすべて追従させ、Mapのcloneとfieldを削除する。
- [ ] 同じcapture試験と `cargo test --locked -p double_riichi_core --lib double_ron` を実行。通常局では実engineのMapが消え、ended_copyには1/2人の評価が残るassert、最終局の供託精算を維持する。
- [ ] field/Map cloneが消え、captureの実/copy比較・unknown/missing winnerエラーが残ることを差分確認。条件付きcommit: `refactor: borrow captured win results`。

### Task B3: 終局table生成のstate cloneを削る（設計11）

**Files:** Modify/Test `crates/double_riichi_core/src/engine.rs`、検証 `crates/double_riichi_core/src/match_machine.rs`。

**Interfaces:** private associated functions `fn table_state_from(state: &GameStateVariant, mode: GameMode, participants: &[Participant]) -> Result<TableState, EngineError>` と `fn round_data_from(state: &GameStateVariant) -> (Wind, u8, Seat, u8, u32, u8)`。既存 `table_state`/`ended_table_state` のcrate向け署名は維持する。

- [ ] 既存 `pre_action_capture_survives_next_round` に、進んだ実stateとended_copyでround/kyoku/dealer/honba/potの少なくとも1項目が異なる局面を使い、生成tableの全round情報がended_copyの値であるassertを加える。連荘ならdealerまで変わる必要はない。公開前はrecordのbefore得点/pot、公開後はafter、手牌は終局の物理牌であることを既存hold試験で確認する。
- [ ] `table_state` の変換本体を借用関数へ移し、通常は `&self.state`、終局は `&ended.ended_copy` を渡す。round_dataの読取も渡したstateに揃える。`self.clone()` とended_copyのcloneだけを除き、roster/tileエラーを残す。
- [ ] `cargo test --locked -p double_riichi_core --lib pre_action_capture_survives_next_round`、`cargo test --locked -p double_riichi_core --features task8-test-support --lib match_machine::tests` を実行。選択件数>0で全PASS。
- [ ] getter以外の3p/4p分岐を汎用化していないことを確認。条件付きcommit: `refactor: read ended table without engine copies`。

### Task B4: DB局結果readを一本化する（設計12）

**Files:** Modify/Test `crates/double_riichi_server/src/storage.rs`。検証 `crates/double_riichi_server/tests/benchmark_storage.rs`。

**Interfaces:** private Storage method `async fn load_kyoku_records(&self, match_id: &str, connection: &mut sqlx::SqliteConnection) -> Result<Vec<KyokuResultRecord>, StorageError>`。callerが所有する同じconnection/transactionを使う。

- [ ] `cargo test --locked -p double_riichi_server --lib authoritative_`、`cargo test --locked -p double_riichi_server --lib ranked_` をbaseline実行。旧ReplayのNoneとcorruptの区別、recordの事前上限検査、ranked/room完了を残す。
- [ ] COUNT/SUM→上限→順序付きSELECT→decode→SQL/JSON列照合をhelperへ集める。rows.lenと累積payload byte検査も共通化先へ残す。readのlegacy条件はmarkerなし・final payloadなし・records空のAND。rankedの借用recordはDB readへ迂回させない。
- [ ] readとcompleteの重複本体をhelper呼び出しへ置換。新transactionは開始せず、既存BEGIN IMMEDIATE・rollbackを維持する。recordありのunknown marker/missing finalをNoneにしない。
- [ ] baselineコマンドに加え `cargo test --locked -p double_riichi_server --lib replay_view_keeps_summary_roster_auxiliary_and_results_in_one_snapshot` と `cargo test --locked -p double_riichi_server --test benchmark_storage`。全PASS、TooLarge/Corruptの区別とDB不変更を確認。
- [ ] 条件付きcommit: `refactor: share bounded kyoku record reads`。

### Task B5: 保存境界内の二重domain検証を削る（設計13）

**Files:** Modify/Test `crates/double_riichi_server/src/storage.rs`。検証 `crates/double_riichi_replay/tests/task5_replay.rs`。Replay公開APIは変更しない。

**Interfaces:** 既存 `validate_result_bundle(game_mode: &str, players: &[ReplayPlayer], frames: &mut [ReplayFrame], records: &[KyokuResultRecord], result: &MatchResult) -> Result<(), StorageError>` を維持。

- [ ] 既存正常multi-Ron読込fixtureから `authoritative_replay_rejects_foreign_identity_and_short_settlement` を追加。①seat1のsettlement IDとwin.winnerを同じforeign IDへ変更、②changesを1件削除、③seat入替。それぞれReplayCorrupt、stored payload不変更をassert。DB側name/kind/ID/final_points矛盾は既存metadata corruption試験に残す。
- [ ] MatchResult validate、DB rosterの人数/seat/ID/name/kind/final_points、record.mode、changes.len、seatとIDの実roster一致をStorageで確認する。**len確認前にzipしない**。validation用Participant構築とStorage側 `record.validate` だけを削除する。
- [ ] 全domain検査は既存 `attach_result_records` に任せ、last after/最終得点照合、canonical dealer/terminal/settlement、サイズ検査、全検査後のframe変更を残す。skip-validation flagや「検証済み」型は作らない。
- [ ] `cargo test --locked -p double_riichi_server --lib authoritative_` と `cargo test --locked -p double_riichi_replay --test task5_replay result_`。全PASSでcanonical矛盾、破損非修復、join失敗非変更を確認。
- [ ] 検査責務を差分で照合。別trust boundaryのvalidationが消えていないことを確認。条件付きcommit: `refactor: validate saved result domains once per join`。

### Task B6: Room保存の再cloneと不要な戻り値を削る（設計14）

**Files:** Modify `crates/double_riichi_core/src/room.rs`。Test `crates/double_riichi_core/tests/task8_room.rs`、`crates/double_riichi_core/tests/benchmark_actor.rs`。

**Interfaces:** `fn send_append(&mut self, match_id: MatchId, events: Vec<GameEvent>)`、`async fn flush_kyoku(&mut self, match_id: MatchId, record: KyokuResultRecord)`。`persist_events` はunitを返すasyncのまま、所有したOption recordを `take()` する。

- [ ] 上記2つのintegration test targetをbaseline実行。full queue、ack失敗、Roomが遊べること、benchmarkの保存失敗停止を残す。
- [ ] EndKyokuでrecordを移動し再cloneを削る。send_appendのasync/awaitとflush_kyokuの常にOkのResult/捨てる代入を削る。`fail_replay`・ack待ち・失敗通知・各eventの順序を変えない。
- [ ] `cargo test --locked -p double_riichi_core --test task8_room` と `cargo test --locked -p double_riichi_core --test benchmark_actor` を実行。effectの件数だけではなく各eventとFlushの順序を確認。batch化しない。
- [ ] 条件付きcommit: `refactor: move owned kyoku records into flush effects`。

### Task B7: ドラ後継の自前wrap表を削る（設計15）

**Files:** Modify/Test `crates/double_riichi_core/src/engine.rs`。

**Interfaces:** 新helperは不要。3pは `riichienv_core::state_3p::game_mode::get_next_dora_tile(marker / 4)`、4pは `riichienv_core::types::standard_next_dora_tile(marker / 4)`。

- [ ] `cargo test --locked -p double_riichi_core --lib result_counts_extracted_north_for_dora_and_ura` と `cargo test --locked -p double_riichi_core --lib task8_actual_open_kan` をbaseline実行。
- [ ] `next_kind` のwrap表を上記public関数に置換。markerの物理IDと牌種を混ぜない。bonus集計、北抜き/裏ドラ、red physical ID、rule依存役満倍率は残す。
- [ ] 同じ試験と `cargo test --locked -p double_riichi_core --lib result_preserves_red_winning_physical_id` を実行し全PASS。既存依存のpublic wrap単体テストを大量に再実装しない。
- [ ] 条件付きcommit: `refactor: reuse pinned engine dora successor helpers`。

### Task B8: 二重ロンの重複本体と診断通常実行を削る（設計6a、6e、配牌準備）

**Files:** Modify/Test `crates/double_riichi_core/src/engine.rs`。

**Interfaces:** test-onlyのliteralケース表と本体1つ。test名 `double_ron_capture_preserves_literal_payments_in_six_cases`。新たな汎用fixture frameworkは作らない。

- [ ] 既存 `task8_final_double_ron_has_physical_deal_literal_payments_and_canonical_bankruptcy_end`、`task8_half_final_double_ron_has_same_physical_wall_offers_literal_payments_and_genuine_end`、`task8_full_wall_offers_double_ron_and_captures_literal_authority` を `cargo test --locked -p double_riichi_core --lib double_ron` でbaseline検証し、6行のmode/通常・破産/literal期待値を控える。
- [ ] 6ケースを1つのtest本体へ統合。実際のoffered Ronと物理wall、77打牌、Half producer mode=2、第四席Pass、実/copy評価、和了牌位置、before/after/pot、破産EndGameをすべて残す。期待得点を製品計算で生成しない。
- [ ] 共通の13枚配牌wall準備のみtest-only関数へ集める。責任払い・役満・kan/kita・最後の供託・異なるwire経路の固有試験は削らない。
- [ ] 診断 `task8_seeded_snapshot_evaluation_delta_and_observed_max_match_log` に `#[ignore = "diagnostic timing; run explicitly"]`。正しさの `seeded_legal_action_lifecycle_reaches_capture` は通常suiteに残す。
- [ ] `cargo test --locked -p double_riichi_core --lib double_ron_capture_preserves_literal_payments_in_six_cases` と `cargo test --locked -p double_riichi_core --lib seeded_legal_action_lifecycle_reaches_capture`。6ケース全PASS。診断は必要時のみ `cargo test --locked -p double_riichi_core --lib engine::tests::task8_seeded_snapshot_evaluation_delta_and_observed_max_match_log -- --ignored --exact --nocapture`。
- [ ] テスト本体/準備の純減と通常suiteからの診断除外を確認。条件付きcommit: `test: consolidate literal double ron scenarios`。

### Task B9: 無効なmutation総当たりとHTTPの保存再準備を削る（設計6b、6d）

**Files:** Modify/Test `crates/double_riichi_core/src/results.rs`、`crates/double_riichi_replay/tests/task5_replay.rs`、`crates/double_riichi_server/src/storage.rs`、`crates/double_riichi_server/tests/task15_replay.rs`。

**Interfaces:** results.rs tests内 `fn valid_meld_result(mode: GameMode) -> (KyokuResultRecord, Vec<Participant>)`。正常baselineをvalidateしてからserdeで1項目ずつ壊す。

- [ ] `restored_meld_result_rejects_tile_and_source_at_the_intended_guard` をcoreに追加。baseはversion1/sequence1/end index4、Win event index3、winner seat1、target0、tsumo false、concealed `[36,40,44,48,52,56,60,64,68,112,113]`、Pon `[108,109,110]` opened/from0/called108、winning113/index10、場風牌(id11/名前「場風牌」)1han・30fu・limit None・Win Points1000。3p/4pのroster IDをseatごとに揃え、beforeは全25000、deltaはseat0=-1000/seat1=+1000/他0、afterはbefore+delta、potは前後0、draw reason/adjustmentはNone。最初にvalidate成功をassertする。これは合成validation fixtureで、実producer捕捉とは称さない。
- [ ] winning/concealed/meld/calledの各fieldを136/255、三麻では4へ変え、エラーが `result contains an invalid tile for its mode` であることをassert。from_whoを人数外/255へ変え、`result contains an invalid meld source seat for its mode` をassert。先行 `invalid completed hand size` の拒否を合格にしない。
- [ ] Replayのtile/source matrixは1つの代表破損recordのjoin拒否、既存frame/record不変更へ縮小。Storageも代表破損→ReplayCorrupt→stored JSON不変更を残す。型復元でchecked constructorを通さない既存serde mutation手段を使う。
- [ ] `contradictory_saved_limit_is_unavailable_not_legacy_or_repaired` の保存worker再準備を削り、同じlimit矛盾とpayload非修復をStorageの正常multi-Ron読込mutationに集約する。HTTPは既存 `corrupt_result_metadata_is_not_legacy_missing` の422/code/list availabilityを残す。
- [ ] `cargo test --locked -p double_riichi_core --lib restored_meld_result`、`cargo test --locked -p double_riichi_replay --test task5_replay result_`、`cargo test --locked -p double_riichi_server --lib authoritative_replay_`、`cargo test --locked -p double_riichi_server --test task15_replay corrupt_result_metadata_is_not_legacy_missing`。全PASS、core得点境界テストは削除しない。
- [ ] 条件付きcommit: `test: reduce overlapping replay corruption matrices`。

### Task B10: public readのsnapshot試験だけを残す（設計6c）

**Files:** Modify/Test `crates/double_riichi_server/src/storage.rs`。

**Interfaces:** public `Storage::replay_view` の既存署名とtransaction所有を維持。private helperをテストから直接呼んでtransactionを提供しない。

- [ ] `replay_view_keeps_summary_roster_auxiliary_and_results_in_one_snapshot` の共通本体でupdate/deleteの2ケースを検査。readerはpublic readのみ。concurrent update/delete中のsummary/players/auxiliary/result/framesは同一snapshot、read後deleteのcascade/次readで不在、をassertする。
- [ ] `cargo test --locked -p double_riichi_server --lib replay_view_keeps_summary_roster_auxiliary_and_results_in_one_snapshot` を実行。追加したdeleteケースもPASSしてから `authoritative_read_keeps_metadata_and_rows_in_one_snapshot` のprivate helper専用本体を削除する。
- [ ] 同じpublic試験を再実行してPASS。テスト自身のtransactionが製品の不足を隠さないことを点検。条件付きcommit: `test: keep replay snapshot guarantees at the public read`。

## バックエンド最終ゲート

- [ ] `cargo --locked fmt --all -- --check`、`cargo test --locked --workspace`、`cargo test --locked --workspace --features double_riichi_core/task8-test-support,double_riichi_server/task8-test-support`。Default-OFFとfixture有効の両方で全PASSを確認。件数0や既知native crashを成功と扱わない。
- [ ] frontend変更も揃った段階で現行 `just test-all` と既存受入ゲートを実行する。必要な既存toolchain/契約fixtureがない場合はBLOCKEDと記録し、OS/settings/依存を勝手に修復しない。
- [ ] 基準差分との比較でfield/clone/本体の純減を記録。速度/メモリ削減率は未測定なら未測定と記載する。fresh whole-branch reviewは選択した実行方法で手配し、未承認のpush/merge/deployはしない。
