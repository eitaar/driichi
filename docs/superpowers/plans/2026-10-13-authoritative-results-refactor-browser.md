# Authoritative Results Browser Test Reduction Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. 実行方法の選択前に実装・委任しない。

**Goal:** 完全Matchの実サーバーE2Eを13→5ケースへ減らし、試験基盤のCargo分類器と反復するhelper自己テストを削除する。

**Architecture:** 実サーバーは生成→公開→保存→Replay→Rematchの契約を代表ケースで検証する。画面配置の16組合せは既存mock表示テスト、native描画は製品Workerテスト、domain得点境界はRustが担当する。

**Tech Stack:** 既存Node 24.11.0/npm 11.4.0、TypeScript/Vitest/Playwright、既存Rust実サーバー試験基盤。version変更なし。

**Spec:** `docs/superpowers/specs/2026-10-13-authoritative-results-refactor-design.md`（承認済み）、項目1、3、4、5。

## Global Constraints

- 新依存・version変更・汎用test framework・fixture class階層・新flagなし。挙動/麻雀ルール/API/保存schema/MJAI互換を変えない。
- 5000msの公開期限、確認の冪等性、未公開結果/次局Decisionの秘匿、Admin Replay権限、入力/保存の上限・transaction・失敗処理を維持する。
- 画質/DPR[1,2]/AA/2048shadowと中央表示/DOM自手牌/Reduced Motion/画像≤10MB・≤16MP・centered cover・遷移reset/fallbackも維持する。
- cold reconnect、compound全体実行、旧表示との画素一致、Chrome/Edge、実GPUと人間の目視は未完了を完了扱いにしない。
- 証拠・`.superpowers`・target junction・prototypeを削除しない。dirty worktreeをreset/cleanしない。
- viewportのmock PASSは実サーバー/実GPUのPASSではない。timeout/retries/deadline/既存性能ゲートを緩和しない。

## Review Focus

1. 別viewportのケースを削っても通常3p/4p、最終3p/4p、4p renderer-stallの別経路を全部通す（C1）。
2. 高速な後続snapshotで期限切れ/不正なfirst reconnect frameを隠さない（C1）。
3. mockの三麻fixtureへ四人のroster/scoreや除外牌を混ぜず、Live/Replay双方を表示する（C1）。
4. spawn error/close null/timeout後のlate closeからexit成功を捏造しない（C2）。
5. helperが読まないfieldのmutationやpure自己テストの反復を消してもforeign Match/Kyoku/clock/cohort拒否を残す（C3）。

## 実行場所・共通手順

製品cwdは `.worktrees/authoritative-results`、npmコマンドはその `frontend` 内。HEAD `b07b1a9` とdirty差分を基準とし、[バックエンド計画](2026-10-13-authoritative-results-refactor-backend.md)のbaseline/sole writer/条件付きcommit規則を使う。実サーバーfixtureは毎ケースで独立、準備済みbuildを使う場合も現行source/binary/feature照合を通す。

この編は[UI/描画計画](2026-10-13-authoritative-results-refactor-frontend.md)後に行う。C1→C2→C3の各差分は単独で検証可能。source custodyに影響する製品変更の後にはfresh buildを使い、古いverified-build receiptを使い回さない。

---

### Task C1: 実サーバー13→5と表示16組合せを分離する（設計1）

**Files:** Modify/Test `frontend/tests/kyoku-results-real-server.spec.ts`、`frontend/tests/result-surfaces.spec.ts`。既存 `frontend/tests/fixtures/task12-projection.json` とcharacter fixtureを再利用し、汎用fixture frameworkを追加しない。

**Interfaces:** 既存 `prepareTask8Server`/harnessと完全Match本体は維持。表示testのlocal helperを `installSocket(page: Page, mode = "4p-red-east")`、`showStandings(page: Page, result = standings)` とし、人数対応の既存fixtureを使う。外部APIの変更なし。

- [ ] `npm run test:browser:real -- kyoku-results-real-server.spec.ts --list` で現行登録を記録。完全シナリオ13と独立control試験を区別し、fixture2種×3p/4p/viewportの対応と保持assertを控える。このコマンドは試験の列挙であってPASS証拠ではない。
- [ ] 完全Matchケースのviewportをすべて1280×720/DPR1の代表値へ揃え、通常3p、通常4p、最終3p、最終4p、通常4p renderer-stallの**5件**だけを登録する。通常と最終fixtureを混ぜず、stallケースは現行native stall操作を使う。
- [ ] 保存/Replay/Rematchと最終standingsは最終3p/4pケース、通常局は次局の公開/Decision、再接続/確認/秘匿は元の該当ケースで維持する。cold reconnectとfinalのtransport reconnectを単一mockで代用しない。first actual snapshotの生frame、generation/index/clock、5秒期限、非参加者/将来winnerの秘匿、stale confirmation拒否を削除しない。
- [ ] `result-surfaces.spec.ts` の既存2幅のscreenshotケースを4viewport（1024×600、1280×720、1600×900、1920×1080）×3p/4p×Live/Replayの**16表示ケース**へ置き換える。DPR検査は既存task12/製品Workerが所有。既存keyboard、shrinking cohort、renderer held/refused、64x seek/stopの固有ケースは残す。
- [ ] 各表示caseはfixtureのmode/人数/roster/scoreを揃え、Win Surfaceと最終standingsを同じcaseで検査する。三麻で合法な既存win牌を使い、standingsのmode/人数も変更する。Replay resultのevent_indexとframe indexを一致させる。`resultGeometry` を実サーバーfileから移植せず、staticなDOMの有限・正のwidth/heightとviewport内収まりをその場のboundingBoxでassertする。
- [ ] LiveはContinueのkeyboard操作可能、ReplayはContinueなし・送信POSTなし、両方でWin Points/physical winning tile/closed Ankan/最終人数をassert。配置caseにはserver期限達成の合否や実GPU画素一致を名乗らせない。
- [ ] `npm run test:browser -- result-surfaces.spec.ts --list`、実サーバー `--list` を再実行。表示16/完全Match5を名前から確認し、独立controlの追加件数を完全Match数に混ぜない。
- [ ] `npm run typecheck`、`npm run test:browser -- result-surfaces.spec.ts` を実行。続いて `npm run test:browser:real -- kyoku-results-real-server.spec.ts` を既存bounded runnerで実行し、残した全caseのPASS/FAIL/未実行とfresh evidenceを保存。既存不合格を削除で合格扱いしない。local runnerがない場合は準備を勝手に拡大せずBLOCKEDを報告する。
- [ ] 13→5の実行削減と元assertの対応を差分点検。条件付きcommit: `test: separate result layout from real match contracts`。

### Task C2: Cargo stderr分類器を削除する（設計3）

**Files:** Modify `frontend/tests/support/real-server.ts`。Test `frontend/src/test/real-server.test.ts`。

**Interfaces:** 既存 `runCommand(command: string, args: string[], cwd: string, input?: string, timeoutMilliseconds?: number, task8?: Task8Build & { stage: "frontend_build" | "cargo_feature_release" }): Promise<{ stdout: string; stderr: string }>` を維持する。ログ原文/command receiptの重要fieldとsource/binary照合を変えない。

- [ ] `npm test -- src/test/real-server.test.ts` でbaseline。維持する試験はsuccess/observed failed close、async error、同期spawn throw、null close、600000ms watchdog、late child events、非Task8/別build stageでの返却/報告。
- [ ] `real-server.test.ts` の共通summary key期待値からnotice分類fieldを外す。`stdout`/`stderr`原文保持、秘密の引数/パス/原文をconsole summaryへ出さない、receiptのobserved_close/exit/incomplete/timed_out/timeout_msをassertする。分類を要求するtestは削除予定として分離する。
- [ ] classify/observeStderr、ANSI strip、line/overflow/incomplete counter、65535 saturation、notices/last_notice、lastOutput/last_output_ageの管理とその専用testを削除する。開始/終了/失敗/timeoutの状態、stage、elapsed、timeout、実close/exit、原文byte数だけの短いconsole summaryを残す。新progress frameworkは作らない。
- [ ] 原文ログの保存/読戻し、spawn失敗時の同一error、未観測closeとnull closeの区別、timer解放、既存process cleanupは維持する。Windows cleanupの既存挙動をこの削除に混ぜて改修しない。
- [ ] retained testでclose nullは `observed_close: true, exit: null` かつ失敗、error/timeoutは `observed_close: false`。timeout後にclose0をemitしても記録は成功へ変わらず、late outputをreceiptへ追加しない。600000msの直前では未timeout、到達時はtimeout/cleanupとなることを確認する。
- [ ] `npm test -- src/test/real-server.test.ts` と `npm run typecheck`。全PASSで専用parser/testが消えていることを確認。条件付きcommit: `test: remove compiler notice classification`。

### Task C3: helper自己テストとクライアント得点matrixを削減する（設計5、4）

**Files:** Modify/Test `frontend/tests/kyoku-results-real-server.spec.ts`、`frontend/src/game/results.test.ts`。製品 `results.ts` は今回変更しない。

**Interfaces:** 現行pure helperの署名は維持。localの独立case `base("result helper self-checks", async () => { ... })` を1件追加し、実Matchでは自己検証helperを呼ばない。

- [ ] `checkFindFreshMatchStarted`、`checkObserveReconnectEvaluation`、`checkReconnectSchedulingProjection`、`checkBrowserTaskSample`、`checkRetainResultGeometryBounds` を独立caseで一度ずつ実行。`checkReconnectUiMatchesCurrentPublication` は既存oracle caseで一度だけ。実Match内の呼び出しとsynthetic自己テストを実Match証拠と紛らわしく報告するmarkを削除する。
- [ ] geometry自己テストはhelperが本当に読むrectのzero-width/height拒否1件にする。無関係な `elementMissing`/`elementConnectedAfterImport` の3変種を消す。測定のobservationsへ失敗入力を残すassertは維持する。
- [ ] oracle内の共通foreign Match/Kyoku/clock/cohort入力を1表にする。同表の各入力で `valid` による「後続正常sampleでも不正過去は消えない」と `matches` 拒否を検査する。label/status/surface/expiryなど異なるbranchの入力は残す。
- [ ] results.test.tsの `preserves supplied scoring boundary data and keeps omitted or null labels absent` の16境界×5label matrixを削除する。既存recognized5label/Win Points保持/high-fu/tag/不正入力試験を残し、null/省略limitだけが失われない小さな `it.each([null, undefined])` を追加。han2/fu40/Win Points2600でOrdinary出力にlimitがないことをassertする。
- [ ] Unknown label拒否の既存assertがなければ同じdecoder試験へ `limit: "Unknown"` →nullを1件追加する。backendの実得点境界matrixは変更しない。
- [ ] `npm test -- src/game/results.test.ts`、`npm run test:browser:real -- kyoku-results-real-server.spec.ts --grep 'oracle|self-checks|expired result confirmation|verified build|native message diagnostic'`。選択件数>0で全PASS。これらは純粋/合成controlsであり実Match受入PASSではない。
- [ ] C1の5完全Matchも最終的に再実行し、自己テストの反復除去でprivacy/clock/confirmation oracleが消えていないことを確認。条件付きcommit: `test: run helper controls once and trim decoder matrices`。

## 全編の仕上げと検証

- [ ] frontend cwdで `npm run typecheck`、`npm test`、`npm run build`、`npm run test:browser`、`npm run test:browser:real` を実行。完全Match5とは別のtask16/controlケースも検査する。実行は既存上限で打ち切り、skipやfailedを成功に数えない。
- [ ] [UI計画F2](2026-10-13-authoritative-results-refactor-frontend.md)のfresh built/native Workerゲート、[バックエンド最終ゲート](2026-10-13-authoritative-results-refactor-backend.md)のdefault/feature両suite、既存 `just test-all` を維持する。
- [ ] 各段階のbaselineとafterで対象fileの純減、削除field/型/関数/3probe file、実シナリオ13→5、検査組合せの消滅を記録。dirty基準を含む比較なのでHEADだけからのdiffを「今回の削減」と称さない。
- [ ] 完了レポートは変更・残した保証・コマンド/結果・未実施/既存不合格を短く記す。cold reconnect/compound/画素一致/Chrome/Edge/実GPU/目視の未受入はリファクタの合格にすり替えない。
- [ ] fresh whole-branch reviewと実行方法はユーザー確認後に手配。commit/push/merge/deployは別の権限として扱う。

## 計画段階の状態

本書は未実行。計画作成で製品コード/試験を変更せず、テスト・browser/GPU測定・commitは実施していない。設計の3編の自己点検後、実行方法を選んでから着手する。
