# Authoritative Results UI and Renderer Refactor Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. 実行方法の選択前に実装・委任しない。

**Goal:** 和了の表示用再変換と、Worker移行前のtexture生成/試験用Worker経路を削除する。

**Architecture:** 検証済みWinResultを既存DOM Surfaceが直接表示する。Windowは既存の画像入力・pixel準備、製品WorkerだけがGPU textureの生成/cover/disposeを所有し、Sceneは受取専用にする。

**Tech Stack:** 既存React 19.3.0、TypeScript 7.0.2、Three.js 0.186.0、R3F 9.4.2、Vite/Vitest/Playwright。version変更なし。

**Spec:** `docs/superpowers/specs/2026-10-13-authoritative-results-refactor-design.md`（承認済み）、項目9、7、2。

## Global Constraints

- 新依存・version変更・汎用test framework・fixture class階層・新flagなし。挙動/麻雀ルール/API/保存schema/MJAI互換を変えない。
- 5000msの公開期限、確認の冪等性、未公開結果/次局Decisionの秘匿、Admin Replay権限、入力/保存の上限・transaction・失敗処理を維持する。
- 画質/DPR[1,2]/AA/2048shadowと中央表示/DOM自手牌/Reduced Motion/画像≤10MB・≤16MP・centered cover・遷移reset/fallbackも維持する。
- Worker分離、1 in-flight＋latest pending、revision/lifetime/motion ID、bitmap解放を維持する。
- cold reconnect、compound全体実行、旧表示との画素一致、Chrome/Edge、実GPUと人間の目視は未完了を完了扱いにしない。
- 証拠・`.superpowers`・target junction・prototypeを削除しない。dirty worktreeをreset/cleanしない。

## Review Focus

1. participant IDは一致してもseatが違う、またはrosterが欠損/重複なら和了portraitを推測表示しない（F1）。
2. null limitや旧Replayの結果欠損を、han/イベントから復元しない（F1）。
3. 削除したEffect fixtureの代わりでも和了牌位置・閉槓の裏牌・ariaを維持する（F1）。
4. Scene unmountはWorker所有textureをdisposeせず、置換/clear/Worker teardownでbitmapを一度だけ解放する（F2）。
5. probe削除でnative描画・非zero geometry・固定品質・cleanupの保証と製品build入口が消えない（F2）。

## 実行場所・順序

製品cwdは `.worktrees/authoritative-results`、npm/Playwrightコマンドはその `frontend` 内。rootの文書相対pathと混同しない。

[バックエンド計画](2026-10-13-authoritative-results-refactor-backend.md)のbaseline/sole writer/dirty差分/条件付きcommit規則を共通手順として使う。F1はB1の次に実施、F2はB8–B10の後に実施する。F2はScene契約変更とprobe削除を一緒に行い、使えない中間buildを作らない。

---

### Task F1: 和了SurfaceへWinResultを直接渡す（設計9、7のprop部分）

**Files:** Modify `frontend/src/game/gameplay.tsx`、`frontend/src/replay.tsx`、`frontend/src/game/three-table.tsx`。Test/fixtures `frontend/src/game/gameplay.test.tsx`、`frontend/src/game/task12.test.ts`、`frontend/src/game/three-table.test.tsx`、`frontend/src/replay.test.tsx`、`frontend/tests/result-surfaces.spec.ts`。呼び出しgrepで見つかったmockも追従する。

**Interfaces:** `RoundWinSurface({ result, room, assets, reducedMotion = false }: { result: WinResult; room: RoomSnapshot | null; assets: AssetState; reducedMotion?: boolean })`。local `RoundWinTiles({ result }: { result: WinResult })`。`WinResult`/decoder/Room wire型はそのまま。

- [ ] `npm test -- src/game/gameplay.test.tsx src/game/task12.test.ts src/replay.test.tsx src/game/three-table.test.tsx` でbaselineを保存する。
- [ ] 既存portrait fallback試験を新propsと正常WinResult fixtureへ変更し、identity不一致でSurfaceなしの試験 `rejects a recorded winner with a foreign seat or invalid roster` を加える。rosterの同ID異seat、重複ID、欠損人数をそれぞれ拒否。新props未実装でREDを確認する。
- [ ] `RoundWinSurface` 内で既存 `resultRoster(room)` とwinnerのID/seatを照合し、違えばnull。name/kind/characterの既存記録を使い、concealed/melds/winning index/yaku/win_points/evaluationをそのまま読む。Ordinary/Yakumanのtagだけを分岐しnull limitは表示しない。
- [ ] LiveはstepのWin、Replayはdecode成功かつevent_index一致のWinを渡す。旧Replayのresultなし、Generic/Silent、倍速/Reduced Motion、portrait画像失敗、キーボード確認を維持する。
- [ ] `RoundWinEffect`、`RoundWinYaku`、`winEffectFromResult`、`PortraitEffect` を削除。ThreeTableの未使用 `portraitEffect` propと各caller/mock、和了牌だけの不完全手牌分岐を削除する。`task12.test.ts` のhanからlimitを推測しないassertはSurface表示試験へ移し、消さない。
- [ ] unitでWin Points 8000をそのまま表示、5han/30fu/null limitからManganを作らない、physical winning tileを1箇所、closed Ankanは裏牌2枚、open kanは裏牌0枚、portrait unavailableのaria labelをassert。更新した正常fixtureの実画像枚数に期待値を揃える。
- [ ] `npm run typecheck`、baseline unit、`npm run test:browser -- result-surfaces.spec.ts`。全PASS。古い型/prop/変換関数への参照0件をgrepする。
- [ ] 今回のhunkだけ差分点検・条件付きcommit: `refactor: render authoritative win results directly`。

### Task F2: GPU texture所有を製品Workerへ一本化しprobeを削除（設計7、2）

**Files:** Modify `frontend/src/game/three-table-scene.tsx`、`frontend/src/game/table-center.tsx`、`frontend/src/game/table-appearance.tsx`、`frontend/src/game/three-table-worker.tsx`（契約追従が必要な場合のみ）。Test `frontend/src/game/three-table.test.tsx`、`frontend/src/game/three-table-worker.test.ts`、`frontend/src/game/table-appearance.test.tsx`、`frontend/tests/renderer-worker.spec.ts`。Config `frontend/tests/support/renderer-worker-build.config.ts`、`frontend/playwright.worker.config.ts`。Delete `frontend/tests/support/renderer-worker-main.ts`、`frontend/tests/support/renderer-worker-probe.tsx`、`frontend/tests/support/renderer-worker.html`。

**Interfaces:** Sceneの `centerTexture: Texture | null` と `appearanceTextures: { surface: Texture | null; surround: Texture | null }` を必須にする。`TableCenter({ texture }: { texture: Texture | null })`。Sceneのlocal image `appearance` propを削除する。ThreeTable/rendererのWindow側appearance入力は残す。

- [ ] unit baseline: `npm test -- src/game/three-table.test.tsx src/game/three-table-worker.test.ts src/game/table-appearance.test.tsx`。prototype/証拠ディレクトリをcaller削除の対象にしない。
- [ ] `three-table-worker.test.ts` に `owns centered cover and replacement cleanup` を追加。square bitmapのsurroundを1280×720へ送るとrepeat `[1, 0.5625]`/offset `[0, 0.21875]`、720×1280へresizeするとrepeat `[0.5625, 1]`/offset `[0.21875, 0]`。rootAdapterに渡ったtextureを観測し、clear/置換/teardownでbitmap.closeが一度だけをassert。既存初期化2999/3000ms・古いrevision・configure途中dispose試験を残す。
- [ ] Scene/ProceduralTable/TableCenterの旧texture生成・cover・dispose分岐を削除し、受け取ったtextureだけ使用する。projection→2048 center pixelsは既存 `paintTableCenter`/renderer、GPU資源所有は既存Workerのまま。Scene unmountで受取textureをdisposeしない試験は既存Scene test adapterで確認する。
- [ ] `createTableImageTexture` と旧helperだけを検査するtestを削除。loadLocalTableImageのMIME/header/10MiB/16MP/abort/再選択/unmount保証は維持する。未使用Three importを除去する。
- [ ] native製品Workerの既存4ケースへprobeが担当した非zero primitive/triangle、固定品質、resize、fresh lifetime、cleanupを対応付ける。**native Workerで実Sceneが描けること**をDOM非依存の証拠とし、試験用Workerや新しい診断flagを追加しない。旧Window constructorとのalpha比較は製品画質の根拠から外す。
- [ ] 最後のprobe専用ケースと3ファイルを削除。build inputはproduct HTMLと既存index.htmlだけにし、preview readiness URLを `/tests/support/renderer-worker-product.html` へ変更する。product HTML/TSX/CSSと製品Workerは残す。
- [ ] unit baselineコマンドと `npm run typecheck` を実行し全PASS。残った旧probe/texture helper参照が0件でbuildできることを確認する。
- [ ] fresh native build/browserを実行する（Bash、frontend cwd、既存CIの上限と同じ）。

```bash
set -euo pipefail
evidence=$(mktemp -d "${TMPDIR:-/tmp}/driichi-refactor-worker.XXXXXXXX")
export DRIICHI_WORKER_PROBE_DIST="$evidence/dist"
timeout --signal=TERM --kill-after=10s 180s npx --no-install vite build \
  --config tests/support/renderer-worker-build.config.ts --outDir "$DRIICHI_WORKER_PROBE_DIST" \
  2>&1 | tee "$evidence/build.log"
timeout --signal=TERM --kill-after=10s 720s npx --no-install playwright test \
  --config=playwright.worker.config.ts --output "$evidence/test-results" \
  2>&1 | tee "$evidence/browser.log"
```

- [ ] build exit0、renderer-workerの製品4ケースとtask12の選択試験すべてPASSを確認。Windowsで同じtimeoutがない場合は既存bounded runnerを使い、上限を消さない。fresh出力は削除せず場所と結果を記録する。
- [ ] 旧probeの削除を画素一致/Chrome/Edge/実GPUゲートの合格と扱わない。条件付きcommit: `refactor: keep GPU textures in the product worker`。

## UI/描画の最終ゲート

- [ ] frontend cwdで `npm run typecheck`、`npm test`、`npm run build`、`npm run test:browser -- result-surfaces.spec.ts task12.spec.ts`。F2のbuilt/native Workerゲートも別に保持する。
- [ ] 実サーバー5ケースは[ブラウザ計画](2026-10-13-authoritative-results-refactor-browser.md)のC1で実行。mock UIの成功を実サーバー証拠にしない。
- [ ] 旧表示との画素一致、Windows Chrome/Edge、実GPU、人間の目視は既存受入手順で確認し、未実施/不合格を明記。固定品質・deadlineを下げて合格を作らない。
