# authoritative-results: 不要な状態・変換・テストを実質削減するリファクタ設計

状態: 設計承認済み（ユーザーの「LGTM」）。前案の「別ファイルへの移動中心」は採用しない。次は実装手順の確認。計画作成のみ、実装・試験実行・commit・merge・push・deployなし。

## 対象と目的

対象は `.worktrees/authoritative-results`、branch `feat/authoritative-kyoku-results`。調査時HEAD `b07b1a9` と未コミットのWorker移行・結果decoder・Replay修正を合わせて基準とする。root main (`54d5be6`) とprototype-doraは変更しない。

ユーザーの意図: 過剰設計を削る。特にテストをもっと削り、その後「もっと探して。テスト以外も」と対象を拡張した。ファイルを分けるだけの整理ではなく、不要な保持状態・表示用再変換・同一境界内の重複処理・重複した証明・試験専用観測器・組合せの実行を減らす。

以下の設計範囲はユーザーが承認したもの。承認は製品/受入条件の緩和、未完了ゲートの免除、commit/push/deployや直ちに実装する許可を意味しない。行番号は調査時点の目印で、実装時はsymbolと最新差分で再確認する。

## 削除・縮小するもの

1–6は既調査のテスト削減、7–15は製品コード。最新調査では、**8–10の状態・DTOの削除を先行**させる。作業順と検証は末尾にまとめる。

### 1. 実サーバーE2Eの総当たりをやめる

`frontend/tests/kyoku-results-real-server.spec.ts:528–575` は通常局3ケース＋最終局2人数×5 viewport/DPR＝**13ケース**。各viewportで結果取得、cold reconnect、確認、保存、Replay、Rematchまで繰り返す。viewportの差で保存やRematchの規則は変わらない。

- 実サーバーの完全なシナリオは **5ケース** に限定: 通常3p、通常4p、最終3p、最終4p、4p renderer-stall。cold reconnect／確認／privacy／保存／Rematchに対応する元のassertを残す。
- 画面の16組合せ（4 viewport × Live/Replay × 3p/4p）は、既存 `frontend/tests/result-surfaces.spec.ts` の表示テストへ集める。静的UI・配置の検査であり実サーバー証拠とは称さない。実サーバーの代表的な3p/4pシナリオから表示に同じ権威データが到達することは別に残す。
- DPR clamp、動作中/静止中の画質、実native Worker、resizeは既存 `task12.spec.ts`／`renderer-worker.spec.ts` が担当。同じ完全Matchをその確認のために繰り返さない。
- 5000msを延長しない。期限内first actual snapshotを後続snapshotや最終DOMで代用しない。既存の不合格は不合格のまま扱う。

### 2. 移行前の試験用Workerを製品テストへ吸収して削除

`frontend/tests/renderer-worker.spec.ts:190–221` のshared Scene検査は、別の試験用Workerを構築する。現在は同ファイル:34–76で製品Workerの画質・resize・Live/Replay・lifetimeを検査済み。

- 必要な非zero描画・固定品質・DOMなしrealm・cleanupの保証を製品Workerの既存ケースで対応付ける。
- 最後のprobe専用ケース、および `tests/support/renderer-worker-{main.ts,probe.tsx,html}` を削除する。build入力とpreview readiness URLも製品側へ変更する。
- 制約のないWindow constructorの属性観測を、製品画質の合格根拠として維持しない。旧診断証拠は `.superpowers` に保持する。
- **未完了の旧表示との画素一致検査は別**。このprobeは画素一致を検査していないため、その削除を画素一致の免除・完了とは扱わない。

### 3. テスト実行基盤のCargoログ分類器を削除

`frontend/tests/support/real-server.ts:93–207` の `runCommand` はprocess起動・timeout・receiptだけでなく、Cargo stderrを1文字ずつ解析し、notice種別・ANSI・4096文字上限・65535 counter・出力間隔を管理する。`frontend/src/test/real-server.test.ts` の多くはこの観測器自体をテストしている。

- ログ分類・notice counter・そのためのテストを削除。開始／終了／失敗／timeoutの短い状態報告だけを残す。
- stdout/stderr原文の保存、実close codeと未観測の区別、spawn失敗、timeoutとprocess tree cleanup、timer解放、build/binary/source/feature照合は残す。
- test helperの新しいprogress frameworkを作らない。実行timeoutとアーティファクト形式の重要な証拠項目は変更しない。

### 4. 麻雀を計算しないクライアントの得点境界マトリクスを削る

`frontend/src/game/results.test.ts:143–165` は16 scoring境界×複数labelを受け入れることを反復する。クライアント得点計算は現在のdirty `results.ts` で既に削除されており、この組合せに分岐はない。

- 当該16境界テストを削除し、既存のWin Points保持、5種類のrecognized label、null/省略、Unknown label拒否、高fuの形、通常/役満tagと入力検証へ集約する。
- backend `results.rs::basic_payment_checks_limits_at_pinned_scoring_boundaries` は実際の得点検証なので残す。
- 同じ理由で全layerの入力検証を削らない。backend、保存済み記録、frontendの各trust boundaryは別の責務。

### 5. E2E内の自己テストと重複assertを縮小

`kyoku-results-real-server.spec.ts:51–99` のoracle自己テストは独立ケースから呼ばれるのに、各実Matchでも619行目付近で再実行。その他のpure helper自己テストも590,620,626–627,659付近で反復する。

- pure helperの自己テストは1回の独立ケースでのみ実行し、各実サーバーケースから呼び出しを削除する。
- 幾何helperの自己テストは実際のzero-width/height拒否1ケースにする。helperが参照しない `elementMissing` / `elementConnectedAfterImport` の3通りは削る。
- oracle自己テスト内で2回列挙する同じforeign Match/Kyoku/clock/cohortの入力は1つの表で検査する。
- 時計の出所・生frame・現在stepとの対応は維持。native observerは診断で `exact_frame_verdict: UNSUPPORTED` としており、削減で新たにexact-frameの合格を捏造しない。

### 6. Rustもテスト本体・無効な組合せ・実行を削る

`crates/double_riichi_core/src/engine.rs` は約7309行だが1699行目以後はtest module。深掘りで次の具体的な削減を決めた。

**6a. 二重ロン3テストの本体を1つにする。** `engine.rs:5653–6177` の `task8_final_double_ron_has_physical_deal_literal_payments_and_canonical_bankruptcy_end`、`task8_half_final_double_ron_has_same_physical_wall_offers_literal_payments_and_genuine_end`、`task8_full_wall_offers_double_ron_and_captures_literal_authority` は、配牌→77打牌→二人Ron→実/copy評価→record検証をほぼ同じ本体で行う。6つのmode/通常・破産ケースをliteral期待値の表にし、共通本体1つへ統合する。Halfのproducer mode=2、死牌/手牌、第四席Pass、破産EndGame、before/after/pot、和了牌位置は落とさない。シナリオ数は6のまま、テスト本体の重複を削る。

**6b. 検証したふりになる不正入力の総当たりを削る。** replay `task5_replay.rs:510–607` のtile/meld/sourceテストとstorage `storage.rs:3641–3754` のsource変種は、完成した14枚手牌へmeldを追加したままsourceやtileを変える。core `results.rs:201–202` の `invalid completed hand size` で先に拒否され、meld側tile/source検査を証明していない。

- core `results.rs` へ、**正常時はvalidateが成功するmeld付きbase**から1項目だけ壊す小さい検証を置く。winning/concealed/meld/called各tileの範囲・三麻除外、sourceの人数外/255を担当する。これは合成validation入力であり新しいproducer captureとは称さない。
- replayは「不正recordがjoin失敗→既存frame/借用recordを変更しない」を代表入力で1回検査。詳細な麻雀domainの組合せは削除する。
- storageは「不正recordがReplayCorrupt→旧記録扱い・自動修復しない」を代表入力で検査。4種source×3/4pの無効な組合せを削除する。3/4人の正常読み込みは既存round-tripに残る。

**6c. private SQL helperの重複したsnapshot試験を削除。** `storage.rs:3210–3355` の `authoritative_read_keeps_metadata_and_rows_in_one_snapshot` は、テスト自身がtransactionを開始してprivate helperへ渡す。public readのtransaction所有を検査する `replay_view_keeps_summary_roster_auxiliary_and_results_in_one_snapshot`（3020–3153）へconcurrent deleteケースとcascade/read-after-deleteのassertを吸収し、private helper専用テストを削除する。update/deleteの両caseを1つの共通本体で実行し、テストでtransactionを与えて製品のtransaction欠落を覆い隠さない。

**6d. HTTPで麻雀の矛盾ごとに保存workerを作り直さない。** `task15_replay.rs:482–639` の `contradictory_saved_limit_is_unavailable_not_legacy_or_repaired` は3/4p各々でOpen→Flush→Append→Finalizeを再構成する。矛盾limitをstorageの既存正常multi-Ron読み込みへのmutationケースに移し、保存payloadがそのまま残ることもそこで検査。HTTPは既存 `corrupt_result_metadata_is_not_legacy_missing` の422/code/list availabilityでerror変換を検査する。coreの得点境界テストは残す。これで159行級の別の保存経路再構成を削除する。

**6e. 診断benchmarkを通常の正しさsuiteから外す。** `engine.rs:1778–1957` の `task8_seeded_snapshot_evaluation_delta_and_observed_max_match_log` は100サンプル×2モード＋12本の自然Matchを実行するが速度の合否条件はない。`#[ignore = "diagnostic timing; run explicitly"]` にし、既存の正しさ検査 `seeded_legal_action_lifecycle_reaches_capture` を残す。必要時は同じtestを `--ignored --exact --nocapture` で実行する。過去の測定証拠は保持し、renderer/実GPU性能ゲートは外さない。

残りの `task8_actual_*` はまず同一の13枚配牌用壁構成だけをtest-only関数へまとめる。責任払い・役満・kan/kita・最後の供託・旧Replayの固有シナリオを削らない。HTTP/Human/MCP/MJAIの各wire境界は別の故障を検出するため、名前が似ているだけで統合しない。特にMJAIのprobe付き/なしはprivate Markerによるqueue経路が異なり、単純な上位互換として一方を削除しない。

### 7. 本体の不要な互換経路を削除

- `three-table.tsx` の受け取って捨てる `portraitEffect` propとLive/Replay呼び出し・mock契約を削除。さらに9の変更後は、表示用DTOのためだけに残っていた `PortraitEffect` 型も削除する。DOM和了surface自体は残す。
- `three-table-scene.tsx`／`table-center.tsx` はtexture受取専用へ。Scene内の旧生成/dispose分岐と `table-appearance.tsx::createTableImageTexture` を削除。coverとGPU資源は既存製品Workerが唯一の所有者。
- Worker分離、1 in-flight＋latest pending、revision/lifetime/motion ID、bitmap解放は削除対象外。

### 8. MatchMachineの全局結果の蓄積と二重の公開状態を削る

`match_machine.rs:103–110,217–220,685,715–724` の `results: Vec<KyokuResultRecord>` は全局の非公開手牌・役・精算を保持し、同時に `HeldKyoku.record` に同じ局のrecordを保持する。`kyoku_results()` はcrate内の呼び出しがなく、将来のTask用という `allow(dead_code)` が残る。実際の保存は `room.rs:3797–3810` が `held_kyoku()` と `take_persistence_events()` を消費している。Vecのその他の用途は最後のafter scoreとの比較だけ。

- 全record Vec・初期化・毎局の `record.clone()`・未使用getterを削除する。
- `record_events` の最終得点照合は削らず、直近精算後の3/4人分の得点だけを保持する。結果一覧を別の新規cacheへ移さない。公開済み履歴は既存RoomHistory、永続記録は既存保存workerに任せる。
- `HeldKyoku.settlement_published` は **`next_step > record.wins.len()` と同値**。初期false、最後のstepでtrueという二重状態を削り、公開projectionとreleaseの判定をこの式へ揃える。Win 0件のDrawでも同じ規則になる。
- `room.rs::start_result_step` は次indexを知るだけのために `published_kyoku_results()` を呼び、公開済みWinを全cloneしている（3572–3579）。既存の `next_step` を読む小さなcrate内getterに置き換える。公開用DTOはprojection/history向けに残す。
- `take_persistence_events()` は現にRoomとテストが使用するため残す。機能接続前という古い `allow(dead_code)` 注記だけを整理する。

変更範囲: MatchMachineのfield・constructor・record/publish/release、Roomのstep開始、対応するcore/Roomテスト。公開 `held_kyoku()` の署名、wire/schema、保存workerは変えない。Draw・単ロン・二重ロン・release前後・最終score不一致abortを検証する。

### 9. 和了表示用の再変換DTOを丸ごと削る

`gameplay.tsx:158–203` は検証済み `WinResult` を `RoundWinEffect` へ作り直し、snake_caseをcamelCaseへ変換し、yaku配列も再生成する。そのDTOは `RoundWinSurface` だけが使う。`RoundWinYaku` と、3Dで既に使わない `PortraitEffect` まで別定義され、結果の必須fieldがoptionalになっている。

- `RoundWinSurface` は **検証済みWinResultと既存RoomSnapshot** を直接受け取る。`RoundWinEffect`／`RoundWinYaku`／`winEffectFromResult` と `PortraitEffect` を削除する。
- 既存 `resultRoster(room)` の人数・ID一意性・名前/ID上限・seat完全性の検証と、winnerのparticipant ID **とseatの一致**を残す。Surface内で照合し、見つからなければ従来どおり表示しない。Live/Replayで別々の推測fallbackを作らない。
- `RoundWinTiles` の入力をWinResultの必須concealed/melds/physical winning positionに揃え、実呼び出しが作らない「手牌がないが和了牌だけある」分岐を削る。通常/役満は `evaluation` のtagをそのまま分岐する。null limitをhanから復元しない。
- 旧Replayでresultがないときは既存の静かな非表示を維持。Generic/Silent、倍速、Reduced Motion、portrait fallback、aria属性を維持する。
- `gameplay.test.tsx` の部分的なEffect fixtureを実際のWinResult fixtureへ変更。`task12.test.ts` の変換関数テストはSurfaceでの表示保証へ移す。名前・Win Points・limit非推測・和了牌位置・open/closed meldの保証は残す。

変更範囲: gameplay、Replayの2つの表示経路、ThreeTable props/import、対応unit/browser fixturesとmock。ネットワークdecoderと結果wire型は削除しない。

### 10. 捕捉済みengine内のWinResultの二重保持を削る

`engine.rs:49–55,496–507,609` の `EngineKyokuEnd.win_results` は `ended_copy` が既に持つMapをそのままcloneしたもの。独立に更新する経路はなく、producerの検証・record変換・数件のtestが読むだけ。

- field・capture時のMap clone・constructorの項目を削除する。3p/4pの `ended_copy` からMapを借用する最小の内部helperを、winner数照合とrecord生成で共有する。
- 捕捉テストはそのMapを直接検査する。**実engineのWinResultが次局開始で消えるが、ended copyでは残る**という保証は維持する。
- `pre_action` と `ended_copy` 自体は両方残す。実精算・最後の供託・終局tableの復元に使用され、単なるテスト専用コピーではない。

変更範囲: capture struct、finish_apply、result_record、capture/破損入力テスト。3p/4p、二重ロン、通常局で実engine側のmapが消えるケース、最終局の供託を確認する。

### 11. 終局tableを読むためのengine全体cloneを削る

`engine.rs::ended_table_state`（956–958）は `self.clone()` した直後に、そのstateを `ended.ended_copy.clone()` で上書きする。現在stateのコピーは即座に破棄され、終局copyのコピーも読取だけに使われる。

- `table_state` の既存snapshot変換本体を、借用した `GameStateVariant` を読む内部関数にする。通常tableは現在state、終局tableは `ended_copy` を渡す。同じ変換処理を使い、ここでの2つのstate cloneをなくす。
- round/dealer/honba/potも必ず**渡したstate**から読み、現在の次局stateを混ぜない。roster人数・identity、tile変換のエラー、精算公開前のbefore score/potへの置換は維持する。
- 新しいsnapshot object、汎用trait、3p/4p共通化frameworkは作らない。PlayerStateの型が異なる既存match分岐は無理に統合しない。

変更範囲: engine table変換とround dataの内部読取、MatchMachineからの呼び出し検証。公開前のtable・再接続・精算後のscore/pot・次局手牌の秘匿を確認する。

### 12. DB局結果の読込・decodeの二重実装を1つにする

`storage.rs::load_authoritative_result`（1218–1255）と `complete_match` のroom用分岐（1826–1876）は、同じCOUNT/SUM上限検査・順序付きSELECT・JSON decode・SQL列とrecordのversion/sequence/end-index照合を別々に持つ。完了側だけがrow読込後の累積byteも数える。

- **DBに保存された局recordを読み出す具体的なprivate helperを1つ**だけ置き、既存のSqliteConnectionを受け取る。readとcompleteの同じtransaction内で利用する。別transactionを開かない。
- allocation前のCOUNT/SUM検査、行数・累積byte上限、version=1、SQLとJSONのsequence/end-index一致を維持する。完了側の強い後段検査を共通化先へ残す。
- 旧Replayの判定はversionなし・final payloadなし・局recordゼロの3条件を維持。markerあり/recordありの破損をlegacyへ降格しない。rankedから渡されたrecordはDB読込へ迂回させない。
- `room_records.as_deref().unwrap()` の分岐依存もこの整理の中で解消できるが、Optionを消す目的だけのenumや新configは作らない。

変更範囲: 上記2つの保存/read関数、既存snapshot・legacy/corrupt/limit・ranked完了テスト。SQL/schema、BEGIN IMMEDIATE、commit/rollbackは変更しない。

### 13. 保存境界内で同じrecordを2回validateしない

`storage.rs::validate_result_bundle`（168–192）は実際のReplay rosterを確認して各 `record.validate(&roster)` を呼び、その直後の `attach_result_records` が精算IDから作ったrosterで**同じrecordの全domain validationをもう一度**行う（`frames.rs:677–696`）。物理牌集合・和了判定・役・得点も反復する。

- StorageではMatchResultのvalidate、DB rosterとの名前/kind/席/最終得点照合、各recordのmodeと**精算の人数・seat・participant IDが実rosterと一致すること**を確認する。
- 全domain validationは既存 `attach_result_records` で1回だけ行う。Storageでvalidation用Participantを再生成する経路と重複した全validate呼び出しを削除する。
- 長さを確認してからzipする。短い精算、seat入替、ID入替をzipの短縮で見逃さない。最終recordのafterとMatchResult、canonical dealer・terminal・settlementの照合、join前に全体検査して非変更にする保証は残す。
- core producer・Replay・frontendといった**別のtrust boundary間**のvalidationは削らない。検査責務を表にして同一境界内の重複だけを消す。

変更範囲: Storageのbundle検証と既存corruptionテスト。Replay joinの公開APIを「検証済みだから安全」型やskip-validation flagへ変えない。名前/kind/ID/seat/長さのmutationとcanonical矛盾を確認する。

### 14. Roomの保存record再cloneと形だけのasync/Resultを削る

`room.rs:3797–3837` はheld recordを1回cloneして所有してから、EndKyokuでさらにcloneしてFlushKyokuへ渡す。`send_append`（3838–3853）はawaitのないasync、`flush_kyoku`（3855–3873）はすべての分岐がOkで、唯一の呼び出しも戻り値を捨てる。

- 所有した `Option<KyokuResultRecord>` をEndKyokuで `take()` して渡す。最初のcloneはheldを公開用に保持する必要があるため残す。二つ目だけ削る。
- `send_append` は同期関数へ。`flush_kyoku` はunitを返すasyncへ。queue full・ack失敗・`fail_replay`・benchmark停止の現行処理をすべて残す。
- **event batchingは今回は採用しない**。1eventずつの送信は削減余地があるが、queue上限の意味とEndKyoku→durable Flush→futureイベントの順序を変える。小さい削除に混ぜず、実測と別の検証が必要な候補とする。

変更範囲: Roomの唯一のpersist呼び出しと上記private helper。Append/Flushの順序、full queue、保存worker失敗、通常Roomとbenchmarkの異なる失敗応答を既存テストで確認する。

### 15. 既存依存が公開するドラ後継計算の自前再実装を削る

`engine.rs:698–715` の `next_kind` は風/三元/数牌のwrapと三麻1m↔9mを手書きする。固定依存 `riichienv-core = 0.4.10` には `types::standard_next_dora_tile` と `state_3p::game_mode::get_next_dora_tile` がpublicで存在する。

- markerの物理IDを `marker / 4` の牌種へ変換し、3p/4pに対応する既存関数を使う。手書きwrap表を削除する。依存更新はしない。
- bonusの数え上げ自体は残す。抜いた北によるドラ/裏ドラ、赤牌、accepted winning physical tileは独自の結果adapterの責務。
- 役ID→翻の手書き対応まで「依存にありそう」と削除しない。0.4.10の `Yaku` に翻/役満倍率fieldはなく、rule依存の二倍役満も必要。公開された総翻から個々の役の翻を推測する代替は採用しない。

確認根拠: pinned Cargo sourceのpublic宣言と関数本体。Context7のRiichiEnv資料も参照したが、指定helperの0.4.10契約は資料だけでは得られなかったため、最新mainではなくインストール済み固定版を根拠にする。三麻1m/9m、数牌/風/三元のwrap、北抜き・裏ドラの既存producerテストで検証する。

## 成果物と作業順

実装手順は次の3編に保存した（計画確認・実行方法の選択待ち）。独立して差し戻せる単位とし、実装ファイルの分割だけを目的にしない。

- [バックエンド／Rustテスト](../plans/2026-10-13-authoritative-results-refactor-backend.md): B1–B10、設計8・10–15・6。
- [UI／描画](../plans/2026-10-13-authoritative-results-refactor-frontend.md): F1–F2、設計9・7・2。
- [ブラウザテスト基盤](../plans/2026-10-13-authoritative-results-refactor-browser.md): C1–C3、設計1・3–5。

推奨作業順は以下。

1. **状態とDTOの削除（8–10）**。全局record蓄積、公開bool、WinResult重複Map、表示用3型・変換関数を消す。coreとLive/Replayの結果検査を実行する。
2. **読取と保存の重複削除（11–15）**。snapshot clone、SQL decode、同一境界内validate、record再clone、不要なasync/Result、ドラwrap表を消す。項目ごとに独立した差分とする。
3. **Rustテストの実質削減（6）**。製品コードの変更に追従したテストを基準に、重複本体・無効なmutation・重複HTTP保存準備・通常suiteの診断測定を削る。
4. **描画旧経路と試験用Workerの削除（2,7）**。texture所有を製品Workerへ一本化し、probeとbuild/readiness設定を消す。
5. **ブラウザテスト・補助基盤の削減（1,3,4,5）**。13→5の完全E2Eと表示マトリクスの分離、分類器・自己テスト反復を消す。

成功は移動行数ではなく、上記field/型/関数/旧経路の消滅、13→5の完全E2E実行、不要なWorker3ファイルと分類器の消滅、不要なassert組合せの消滅、同じ保証を残した純減で確認する。削減行数・速度は実装前後で測り、未計測の削減率は約束しない。

### 検証の位置付け

これは実装前の設計案。今回テストは実行しておらず、下記は後続計画での必須確認である。

- core: 3p/4p、通常/最終、Draw/単ロン/二重ロン、供託・破産、公開prefix、未公開結果/次局の秘匿、確認/release冪等性、score不一致abort。
- Storage/Replay: 正常readとranked/room完了、legacy/corrupt区別、人数/席/ID/名前/kind/得点矛盾、tile/sourceの実際の検査分岐、上限、公開readの同一transaction、join失敗時の非変更、保存失敗。
- frontend: typecheckと既存unit、Live/Replay同一のauthoritative facts、physical winning position、meldの表裏、portrait fallback、欠損/矛盾時の非推測と非表示、縮まるdeadline、再接続、接続中のみの確認。
- renderer/browser: native製品Worker、revision/lifetime/motion ID、資源解放、固定品質、DPR/resize/画像fallback、残す5件の実サーバー経路とmockの配置matrix。
- focusedの合格だけで既存full-suite/受入条件を免除しない。既存不合格や未実行を、リファクタで消したので合格とは扱わない。

承認後の詳細計画には、実装開始時に現行package scriptとCargo test名を確認して具体的コマンドを載せる。今回、巨大な試験基盤や新しい網羅テストを追加して削減を相殺しない。

## 全計画共通の不変条件

新依存・version変更・汎用test framework・fixture class階層・新flagなし。挙動/麻雀ルール/API/保存schema/MJAI互換を変えない。

5000msの公開期限、確認の冪等性、未公開結果/次局Decisionの秘匿、Admin Replay権限、入力/保存の上限・transaction・失敗処理を維持する。画質/DPR[1,2]/AA/2048shadowと中央表示/DOM自手牌/Reduced Motion/画像≤10MB・≤16MP・centered cover・遷移reset/fallbackも維持する。

cold reconnect、compound全体実行、旧表示との画素一致、Chrome/Edge、実GPUと人間の目視は未完了を完了扱いにしない。証拠・`.superpowers`・target junction・prototypeを削除しない。dirty worktreeをreset/cleanしない。実装開始時に現在差分とsole writerを確認する。

## 採用しない拡張・未調査

- 全約2.7万追加行の網羅監査ではない。結果生成→Room公開/保存→DB read→Replay join→Live/Replay UIと現行Worker移行の読んだ経路を対象にする。
- Room/MatchMachineの公開プロトコル、terminal captureの単局copy、保存方式、DB schema、canonical MJAI、Workerのackとreadyの区別は再設計しない。
- 完了時にもReplay framesへ結果をattachして、その後framesを捨てる処理には追加削減余地がある。ただし公開のvalidation-only APIを新設する変更とサイズ検査の責務整理が要るため、今回の小さい削除から除外する。serialized size検査を単純に外さない。
- frontendのstep再decodeも確認した。まずsnapshot/update内の同じ入力のdecode共有で十分であり、新しい正規化cache・Room型階層・deadline state machineを設計しない。この項目は上記削除が終わる前に独立リファクタとして増やさない。
- 外部frameworkへの置換、ファイル分割だけの整理、追加config/flag、測っていない性能目標は採用しない。

今回テスト・GPU測定・ブラウザ/ネットワーク動作検証は実行していない。独立subagent reviewも行っていない。設計の自己点検にとどまり、承認前の製品コード変更はない。

## 規範となる既存文書

- `.worktrees/authoritative-results/.superpowers/sdd/2026-10-02-authoritative-kyoku-results/approved-resolutions.md`
- `.worktrees/authoritative-results/.superpowers/sdd/2026-10-02-authoritative-kyoku-results/ci-cancel-current1/renderer-isolation-design1.md`
- `.worktrees/authoritative-results/docs/superpowers/plans/2026-10-02-authoritative-kyoku-results.md`

この設計は内部保持状態・変換・検証配置・テスト本数・補助実装の変更案。外部の製品仕様/受入条件の緩和ではない。対応する保証が抜けると判明した削除は、その項目だけ保留して報告する。
