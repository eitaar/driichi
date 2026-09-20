# Cinematic R3F Match Table Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the Live/Replay Pixi faux-3D table with one React 19-compatible R3F v9 scene that matches Sol-approved Live 4p, Live 3p, and Replay concepts without changing authoritative gameplay.

**Architecture:** `ThreeTable` is the React lifecycle/error/fallback adapter. `MatchTableScene` renders one fixed-camera procedural table and instanced tiles from renderer-neutral layout data. Semantic player frames and all controls remain DOM overlays. Existing Pixi entry-page vignette stays untouched; only the Match/Replay renderer is replaced.

**Tech Stack:** React 19.3, Three.js, `@react-three/fiber` v9, TypeScript, Zustand, Vitest, Playwright, existing GSAP only where already owned outside the new R3F loop.

**Spec:** `docs/superpowers/specs/2026-09-19-r3f-cinematic-match-table-design.md`

**Visual authority:** `docs/design/concepts/live-4p-v2.png`, `docs/design/concepts/live-3p-v2.png`, `docs/design/concepts/replay-v2.png`

## Global Constraints

- Install only `three` and React Three Fiber dependencies required by the approved architecture; pin resolved versions in `package-lock.json`.
- Use `@react-three/fiber@9` with React 19.3.
- Preserve `decision_id`, `action_id`, pending/retry, no optimistic mutation, reconnect, terminal close, Replay, Backend, domain, projection, protocol, MJAI, and MCP behavior.
- Use one fixed perspective camera; no OrbitControls, physics, GLTF pipeline, runtime CDN, or ray-cast legal actions.
- Use existing tile faces and Character Pack portraits.
- Use `InstancedMesh`, one tile-face atlas, `frameloop="demand"`, DPR `[1, 1.5]`, one shadow-casting light, and a 1024px shadow map maximum.
- Keep every semantic control/status/dialog in DOM. Canvas is visual-only and hidden from assistive technology.
- Three-player layout contains bottom, left, and right only.
- Local hand projected area is at least 1.28× opponent hand area.
- Motion is event-only; Reduced Motion applies authoritative state immediately.
- UI copy remains English.
- Do not redesign Lobby, Admin, or the entry vignette.

## Review Focus

1. A malformed or missing `remaining_wall` must produce a bounded deterministic count in both center text and instance transforms.
2. A connection loss between pointer-down and action submit must create neither a socket message nor phantom pending state.
3. WebGL unsupported/context failure must leave legal DOM controls, status, and Match facts operable.
4. A 3p projection with stale or malformed fourth-player data must not create a top visual Seat.
5. Rapid projection replacement during event animation must cancel old animation handles and render the newest authoritative state.

---

### Task 1: Add R3F dependencies and renderer-neutral 3D layout

**Files:**
- Modify: `frontend/package.json`
- Modify: `frontend/package-lock.json`
- Create: `frontend/src/game/three-table-layout.ts`
- Create: `frontend/src/game/three-table-layout.test.ts`
- Reuse: `frontend/src/game/orientation.ts`
- Reuse: `frontend/src/game/table-geometry.ts`
- Reuse: `frontend/src/game/types.ts`

**Interfaces:**
- Consumes: `ProjectedState`, `ProjectedPlayer`, `RoomSnapshot`, existing `seatPositionFor()`, and `wallTileCount()`.
- Produces:

```ts
export type Vec3 = readonly [number, number, number];
export type SceneSeat = "bottom" | "right" | "top" | "left";

export interface SceneTile {
  key: string;
  tile: number | null;
  position: Vec3;
  rotation: Vec3;
  scale: number;
  face: "front" | "back";
  group: "hand" | "discard" | "meld" | "wall" | "dora";
}

export interface ScenePlayer {
  participantId: string;
  seat: number;
  position: SceneSeat;
  isLocal: boolean;
}

export interface MatchSceneLayout {
  players: ScenePlayer[];
  tiles: SceneTile[];
  wallCount: number;
}

export function buildMatchSceneLayout(
  projection: ProjectedState,
  room: RoomSnapshot | null,
): MatchSceneLayout;
```

- Constants exported for scene ownership: `TABLE_SIZE`, `CAMERA`, `LOCAL_TILE_SIZE`, `REMOTE_TILE_SIZE`.

- [ ] **Step 1: Install compatible dependencies**

Run:

```bash
cd frontend
npm install three @react-three/fiber@^9
```

Expected: React remains `19.3.0`; lockfile resolves R3F major 9 and no unrelated runtime package.

- [ ] **Step 2: Write failing layout tests**

Cover exact contracts:

```ts
it("maps 3p to bottom, right, and left without top", () => {
  const layout = buildMatchSceneLayout(threePlayerProjection, room);
  expect(layout.players.map((player) => player.position).sort()).toEqual([
    "bottom",
    "left",
    "right",
  ]);
});

it("makes the local hand at least 1.28x the opponent scale", () => {
  const layout = buildMatchSceneLayout(fourPlayerProjection, room);
  const local = layout.tiles.find((tile) => tile.group === "hand" && tile.scale === LOCAL_TILE_SIZE);
  const remote = layout.tiles.find((tile) => tile.group === "hand" && tile.scale === REMOTE_TILE_SIZE);
  expect(local).toBeDefined();
  expect(remote).toBeDefined();
  expect(LOCAL_TILE_SIZE / REMOTE_TILE_SIZE).toBeGreaterThanOrEqual(1.28);
});

it.each([[-1, 0], [Number.NaN, 0], [999, 136]])(
  "bounds malformed wall counts",
  (remainingWall, expected) => {
    const layout = buildMatchSceneLayout(projectionWithWall(remainingWall), room);
    expect(layout.wallCount).toBe(expected);
    expect(layout.tiles.filter((tile) => tile.group === "wall")).toHaveLength(expected);
  },
);
```

Also assert fixed camera values, deterministic keys/transforms, all transforms finite, local hand separated draw tile, six-column rivers, and fourth-player data ignored in 3p.

- [ ] **Step 3: Verify RED**

Run:

```bash
cd frontend
npm test -- --run src/game/three-table-layout.test.ts
```

Expected: FAIL because `three-table-layout.ts` or its exports do not exist.

- [ ] **Step 4: Implement the minimum pure layout module**

Reuse existing orientation and wall helpers. Do not import React, Three.js, R3F, DOM, or Zustand. Convert authoritative projection fields into immutable arrays. Use fixed world constants from the approved spec. Ignore players whose Seat is not in `tableSeatGeometry(mode, viewerSeat)`.

- [ ] **Step 5: Verify GREEN and type safety**

Run:

```bash
cd frontend
npm test -- --run src/game/three-table-layout.test.ts src/game/table-geometry.test.ts
npm run typecheck
```

Expected: both files pass and TypeScript exits zero.

- [ ] **Step 6: Commit**

```bash
git add frontend/package.json frontend/package-lock.json frontend/src/game/three-table-layout.ts frontend/src/game/three-table-layout.test.ts
git commit -m "feat(frontend): define R3F table layout"
```

---

### Task 2: Build the shared instanced R3F scene

**Files:**
- Create: `frontend/src/game/tile-atlas.ts`
- Create: `frontend/src/game/three-table-scene.tsx`
- Create: `frontend/src/game/three-table.tsx`
- Create: `frontend/src/game/three-table.test.tsx`
- Reuse: `frontend/src/game/tiles.ts`
- Reuse: `frontend/src/game/assets.ts`
- Reuse: `frontend/src/game/animation.ts`
- Reuse: `frontend/src/game/three-table-layout.ts`

**Interfaces:**
- Consumes: `MatchSceneLayout`, existing `tileAssetUrl()`, `AnimationItem[]`, and Character Pack asset URLs.
- Produces:

```ts
export interface TileAtlas {
  texture: THREE.CanvasTexture;
  columns: number;
  rows: number;
  cellFor(tile: number): readonly [number, number];
  dispose(): void;
}

export async function createTileAtlas(signal?: AbortSignal): Promise<TileAtlas>;

export interface PortraitEffect {
  characterId: string;
  displayName: string;
  result: "Ron" | "Tsumo";
  han?: number;
  fu?: number;
  limit?: string;
  points?: number;
}

export interface ThreeTableProps {
  projection: ProjectedState | null;
  room: RoomSnapshot | null;
  animations?: AnimationItem[];
  reducedMotion?: boolean;
  portraitEffect?: PortraitEffect | null;
  onAnimationConsumed?: (id: number) => void;
  surface?: "live" | "replay";
}

export function ThreeTable(props: ThreeTableProps): JSX.Element;
```

- Scene host: `data-testid="three-table"`, concise accessible label on host, Canvas `aria-hidden="true"`.
- Instrumentation: `data-render-ready`, `data-rendered-tile-count`, `data-rendered-scene-primitives`, `data-wall-tile-count`, `data-webgl-fallback`, `data-animation-state`, and `data-player-frame-count`.

- [ ] **Step 1: Write failing atlas and adapter tests**

Mock image loading and Canvas only at the browser boundary. Assert:

```ts
it("packs every project tile face into one atlas", async () => {
  const atlas = await createTileAtlas();
  expect(atlas.texture).toBeInstanceOf(THREE.CanvasTexture);
  const cells = allTileCodes.map((tile) => atlas.cellFor(tile).join(":"));
  expect(new Set(cells).size).toBe(allTileCodes.length);
  atlas.dispose();
});

it("uses demand rendering and bounded dpr", () => {
  render(<ThreeTable projection={projection} room={room} />);
  expect(mockCanvas).toHaveBeenCalledWith(expect.objectContaining({
    frameloop: "demand",
    dpr: [1, 1.5],
  }));
});

it("keeps DOM fallback facts and children when Canvas fails", () => {
  render(<ThreeTable projection={projection} room={room} />);
  expect(screen.getByRole("status", { name: /3d table unavailable/i })).toHaveTextContent(/East 1/i);
});
```

Also assert Canvas hidden semantics, resource disposal, one fixed camera, no controls component, and instrumentation names.

- [ ] **Step 2: Verify RED**

Run:

```bash
cd frontend
npm test -- --run src/game/three-table.test.tsx
```

Expected: FAIL because the atlas and R3F adapter do not exist.

- [ ] **Step 3: Implement one atlas and instanced tile renderer**

`createTileAtlas()` loads existing face SVG URLs, draws them into one bounded Canvas grid, creates one sRGB `CanvasTexture`, and exposes deterministic cells. It throws an abort-safe error and disposes partial resources.

`InstancedTiles` uses:

- one instanced body mesh for visible tile bodies;
- one atlas-aware instanced plane mesh for visible faces;
- one instanced body/back pair for concealed and wall tiles;
- per-instance matrix and atlas-cell attributes;
- memoized transforms from `MatchSceneLayout`;
- no per-tile React state or ray-cast handlers.

- [ ] **Step 4: Implement the premium procedural table scene**

Create table body, felt inset, rails, center device, restrained bronze hardware, fixed camera, ambient/key/fill/rim lighting, and one shadow source. Use the exact spec envelope. Do not add post-processing, fog, OrbitControls, GLTF, or physics.

- [ ] **Step 5: Implement adapter, error boundary, fallback, and cleanup**

`ThreeTable` owns `<Canvas frameloop="demand" dpr={[1, 1.5]}>`, Suspense/loading state, WebGL fallback, error boundary, ResizeObserver-safe host sizing, instrumentation, and disposal. Fallback includes round, bounded wall count, Dora text, and keeps sibling DOM controls unaffected.

- [ ] **Step 6: Verify GREEN**

Run:

```bash
cd frontend
npm test -- --run src/game/three-table.test.tsx src/game/three-table-layout.test.ts
npm run typecheck
npm run build
```

Expected: tests, typecheck, and production build pass.

- [ ] **Step 7: Commit**

```bash
git add frontend/src/game/tile-atlas.ts frontend/src/game/three-table-scene.tsx frontend/src/game/three-table.tsx frontend/src/game/three-table.test.tsx
git commit -m "feat(frontend): render shared instanced R3F table"
```

---

### Task 3: Integrate Live, 3p, Replay, and semantic overlays

**Files:**
- Modify: `frontend/src/game/gameplay.tsx`
- Modify: `frontend/src/replay.tsx`
- Modify: `frontend/src/styles.css`
- Modify: `frontend/src/replay.test.tsx`
- Modify: `frontend/src/game/task12.test.ts`
- Modify: `frontend/tests/task12.spec.ts`
- Modify: `frontend/tests/task15.spec.ts`
- Modify: `frontend/tests/task16-real-server.spec.ts`
- Delete after replacement: `frontend/src/game/pixi-table.tsx`
- Delete after replacement: `frontend/src/game/table-art.ts`
- Delete after replacement: `frontend/src/game/table-effects.ts`

**Interfaces:**
- Consumes: `ThreeTable`, unchanged `GameplaySurface` action contract, unchanged Replay frames, existing Character Pack helpers.
- Produces:

```ts
export interface TablePlayerOverlayProps {
  projection: ProjectedState;
  room: RoomSnapshot | null;
  surface: "live" | "replay";
}

export function TablePlayerOverlay(props: TablePlayerOverlayProps): JSX.Element;
```

- Player overlays render display name, score, Seat position/Wind, Riichi state, and portrait/fallback in DOM.

- [ ] **Step 1: Update tests to require renderer-neutral R3F output**

Change mocked imports and selectors from `PixiTable`/`pixi-table` to `ThreeTable`/`three-table`. Add assertions that:

```ts
await expect(page.getByTestId("three-table")).toHaveAttribute("data-render-ready", "true");
await expect(page.getByTestId("three-table")).toHaveAttribute("data-rendered-tile-count", /^[1-9]\d*$/);
await expect(page.locator(".table-player-frame")).toHaveCount(mode.startsWith("3p") ? 3 : 4);
await expect(page.locator('.table-player-frame[data-position="top"]')).toHaveCount(mode.startsWith("3p") ? 0 : 1);
```

At every required viewport, assert each player frame, Action deck, candidate dialog, and Replay transport bounding box lies inside the stage and does not intersect the local-hand safe box.

- [ ] **Step 2: Verify RED**

Run:

```bash
cd frontend
npm test -- --run src/replay.test.tsx src/game/task12.test.ts
npx playwright test tests/task12.spec.ts tests/task15.spec.ts --project=chromium
```

Expected: FAIL because consumers and selectors still use Pixi.

- [ ] **Step 3: Replace Live and Replay consumers**

Import `ThreeTable` in `gameplay.tsx` and `replay.tsx`. Preserve all action submission, hit-layer, dialog, Settings, Toast, blocking, results, connection, and Replay transport logic. Keep the local tile hit layer aligned to the approved local hand; do not move action semantics into Canvas.

- [ ] **Step 4: Add shared compact player overlays**

Render portrait thumbnails on outer rails using projection orientation. Use English name/score/Seat/Riichi text and existing portrait fallback. Three-player mode omits top even when malformed fourth data exists. Preserve the hidden full Player status list for screen readers.

- [ ] **Step 5: Replace Pixi visual CSS with approved R3F composition**

Use the v2 concepts as authority:

- table occupies approved stage bounds;
- portraits are tiny outer-rail frames;
- local hand safe box is larger and unobstructed;
- Live action row sits directly above the hand;
- Replay transport stays inside lower rail and event log stays below stage;
- 1024×600, 1280×720, 1600×900, and 1920×1080 stay within bounds;
- Reduced Motion removes transitions/transforms that imply movement.

- [ ] **Step 6: Delete Match-only Pixi modules**

Delete only the three Match-table modules after all imports are gone. Keep `pixi-vignette.tsx` and the Pixi dependency because the entry vignette is outside this redesign.

- [ ] **Step 7: Verify GREEN**

Run:

```bash
cd frontend
npm test -- --run src/replay.test.tsx src/game/task12.test.ts src/game/three-table.test.tsx
npx playwright test tests/task12.spec.ts tests/task15.spec.ts --project=chromium
npm run typecheck
npm run build
```

Expected: focused unit/browser tests, typecheck, and build pass.

- [ ] **Step 8: Commit**

```bash
git add frontend/src/game/gameplay.tsx frontend/src/replay.tsx frontend/src/styles.css frontend/src/replay.test.tsx frontend/src/game/task12.test.ts frontend/tests/task12.spec.ts frontend/tests/task15.spec.ts frontend/tests/task16-real-server.spec.ts frontend/src/game/three-table.tsx
git rm frontend/src/game/pixi-table.tsx frontend/src/game/table-art.ts frontend/src/game/table-effects.ts
git commit -m "feat(frontend): integrate shared R3F Live and Replay table"
```

---

### Task 4: Add bounded event motion, fallback coverage, and focused acceptance

**Files:**
- Create: `frontend/src/game/three-table-motion.ts`
- Create: `frontend/src/game/three-table-motion.test.ts`
- Modify: `frontend/src/game/three-table-scene.tsx`
- Modify: `frontend/src/game/three-table.tsx`
- Modify: `frontend/tests/task12.spec.ts`
- Modify: `frontend/tests/task15.spec.ts`
- Modify: `frontend/tests/task16-real-server.spec.ts`
- Modify: `frontend/playwright.config.ts` only if browser launch flags are required for deterministic WebGL software rendering

**Interfaces:**
- Consumes: `AnimationItem[]`, `reducedMotion`, R3F `invalidate`, newest `MatchSceneLayout`.
- Produces:

```ts
export interface SceneMotion {
  itemId: number;
  startedAt: number;
  durationMs: number;
  kind: "draw" | "discard" | "call" | "riichi" | "score" | "win";
}

export function nextSceneMotion(
  items: AnimationItem[],
  reducedMotion: boolean,
): SceneMotion | null;

export function sceneMotionProgress(
  motion: SceneMotion,
  now: number,
): number;
```

- [ ] **Step 1: Write failing motion tests**

Assert durations stay inside approved ranges, progress clamps to `[0, 1]`, Reduced Motion returns `null`, newest projection cancels stale motion, and completion reports the exact animation ID once.

- [ ] **Step 2: Verify RED**

Run:

```bash
cd frontend
npm test -- --run src/game/three-table-motion.test.ts
```

Expected: FAIL because motion helpers do not exist.

- [ ] **Step 3: Implement bounded event-only motion**

Use a single R3F `useFrame` callback only while a `SceneMotion` exists. Animate transforms/emissive intensity only, call `invalidate()` during the bounded interval, stop after completion, and cancel on projection replacement/unmount. Win accent may adjust camera target/FOV within spec and must return to fixed values. Do not use an idle loop or mix GSAP into the scene.

- [ ] **Step 4: Add browser fallback, motion, and performance instrumentation coverage**

Cover:

- WebGL creation failure shows DOM fallback and leaves legal actions usable;
- Reduced Motion produces immediate state and `data-animation-state="static"`;
- normal event motion reaches idle and consumes the exact item once;
- all three surfaces render non-zero instances at four viewports;
- 3p never exposes top player frame;
- Replay controls remain bounded;
- connection/action identity tests stay unchanged and green;
- browser Performance entries show no persistent animation frame loop after idle; sample frame duration during one event has no major regression versus the previous focused baseline.

- [ ] **Step 5: Run focused acceptance**

Run:

```bash
cd frontend
npm test -- --run src/game/three-table-motion.test.ts src/game/three-table.test.tsx src/game/three-table-layout.test.ts src/game/task12.test.ts src/replay.test.tsx
npx playwright test tests/task12.spec.ts tests/task15.spec.ts tests/task16-real-server.spec.ts --project=chromium
npm run typecheck
npm run build
```

Expected: all commands pass with zero failures.

- [ ] **Step 6: Commit**

```bash
git add frontend/src/game/three-table-motion.ts frontend/src/game/three-table-motion.test.ts frontend/src/game/three-table-scene.tsx frontend/src/game/three-table.tsx frontend/tests/task12.spec.ts frontend/tests/task15.spec.ts frontend/tests/task16-real-server.spec.ts frontend/playwright.config.ts
git commit -m "feat(frontend): add bounded R3F table motion and fallback"
```

---

## Plan self-review

- Spec coverage: dependencies, React 19/R3F v9, fixed camera, procedural premium table, instancing, atlas, demand loop, bounded DPR/shadow, Live/Replay sharing, 3p omission, DOM semantics, fallback, motion, Reduced Motion, action contracts, four viewports, and focused acceptance all have owning tasks.
- Placeholder scan: no deferred implementation placeholders are present.
- Type consistency: `MatchSceneLayout`, `ThreeTableProps`, `TileAtlas`, and `SceneMotion` are defined once and consumed by later tasks under the same names.
- Review Focus coverage: wall bounds belongs to Task 1; disconnected action behavior remains in Task 3 browser regression; WebGL fallback belongs to Tasks 2 and 4; malformed fourth Seat belongs to Tasks 1 and 3; projection replacement cancellation belongs to Task 4.
- YAGNI: the plan keeps Pixi only for the unrelated entry vignette, adds no Drei/post-processing/physics/GLTF dependency, and limits new modules to pure layout, atlas, scene/adapter, and bounded motion.
