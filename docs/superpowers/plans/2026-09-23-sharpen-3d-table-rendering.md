# Sharpen 3D Table Rendering Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the shared Live/Replay 3D mahjong table crisp at device pixel ratios up to 2 without adaptive quality drops or regression from the existing desktop motion budget.

**Architecture:** Keep the existing `ThreeTable`/`MatchTableScene` boundary and improve quality at its two actual sampling stages: the WebGL drawing buffer and source textures. The Canvas will use DPR 1–2, MSAA, and high-precision shaders; the tile atlas will use 2× cells and the felt will use bounded anisotropic filtering. Existing instancing, demand rendering, disabled shadows, and DOM interaction/accessibility remain unchanged.

**Tech Stack:** React 19.3, TypeScript 7, Three.js 0.186, React Three Fiber 9.4, Vitest 5, Playwright 1.63.

**Spec:** `docs/superpowers/specs/2026-09-19-r3f-cinematic-match-table-design.md`

## Global Constraints

- Live 4p, Live 3p, and Replay must continue using the same `ThreeTable` and `MatchTableScene` renderer.
- Render quality is fixed for a renderer: use the device DPR clamped to `[1, 2]`; do not lower DPR during motion and do not add automatic or manual quality modes.
- Preserve the existing 1600×900 median frame budget of `≤17.5ms` and the 1920×1080 non-regression bounds of median `≤32ms`, p90 `≤50ms`, and ratio `≤1.5`.
- Preserve `frameloop="demand"` while idle and the bounded `"always"` loop only during authoritative motion.
- Preserve instancing, shared materials/geometries, disabled shadows, no post-processing, and no new runtime dependency.
- Do not change backend, domain, projection, protocol, MJAI, MCP, gameplay state, legal-action DOM controls, or accessibility fallback behavior.
- Keep tile-atlas cells isolated: no adjacent face may bleed into a tile, including red-five cells.

## Review Focus

1. **Device DPR below 1 or above 2:** R3F must clamp the effective renderer ratio to 1–2 and never allocate a drawing buffer above 3840×2160 for the required 1920×1080 viewport. Covered by Task 1 unit and browser tests.
2. **Motion quality drift:** active and idle animation states must report exactly the same renderer pixel ratio. Covered by Task 1 browser performance test.
3. **MSAA or shader precision silently reverting:** Canvas creation must request `antialias: true` and `precision: "highp"`. Covered by Task 1 unit assertions and browser context-attribute assertion.
4. **Atlas enlargement causing neighboring-face bleed or red-five aliasing:** half-texel UV isolation and distinct red-five cells must remain intact. Covered by Task 2 atlas tests.
5. **Higher quality breaking fallback or teardown:** WebGL creation failure, context loss, atlas abort, and texture disposal must retain current behavior. Covered by Task 2 focused suites and final Task 12 browser run.

---

## File Map

- Modify `docs/superpowers/specs/2026-09-19-r3f-cinematic-match-table-design.md`: replace the superseded DPR/antialiasing quality contract.
- Modify `frontend/src/game/three-table.tsx`: request DPR up to 2, MSAA, and high-precision shaders.
- Modify `frontend/src/game/three-table.test.tsx`: pin Canvas quality options without depending on screenshot appearance.
- Modify `frontend/playwright.config.ts`: run Chromium browser coverage at device scale factor 2 so the quality and performance gate exercises the target path.
- Modify `frontend/tests/task12.spec.ts`: assert DPR 2, MSAA, drawing-buffer bounds, motion stability, and the unchanged frame budgets.
- Modify `frontend/src/game/tile-atlas.ts`: double atlas cell resolution while preserving cell addressing and no-mipmap isolation.
- Modify `frontend/src/game/tile-atlas.test.ts`: pin atlas backing dimensions, filters, UV isolation, red-five separation, abort, and disposal behavior.
- Modify `frontend/src/game/table-materials.ts`: raise felt anisotropy from 1 to 4 while retaining mipmaps and color-space handling.
- Modify `frontend/src/game/table-materials.test.ts`: pin the bounded anisotropy setting and existing texture lifecycle.

### Task 1: Raise the WebGL Drawing Quality Contract

**Files:**
- Modify: `docs/superpowers/specs/2026-09-19-r3f-cinematic-match-table-design.md` (`Performance contract` and `Verification and acceptance`)
- Modify: `frontend/src/game/three-table.tsx:484-503`
- Modify: `frontend/src/game/three-table.test.tsx:135-151`
- Modify: `frontend/playwright.config.ts:8-25`
- Modify: `frontend/tests/task12.spec.ts:123-173, 640-738`

**Interfaces:**
- Consumes: R3F `<Canvas dpr gl>` props and existing `data-renderer-pixel-ratio` instrumentation.
- Produces: a Canvas whose effective DPR is `clamp(window.devicePixelRatio, 1, 2)`, whose default framebuffer requests MSAA, and whose shader precision request is `highp`.

- [ ] **Step 1: Change the unit test to state the new Canvas contract**

In `frontend/src/game/three-table.test.tsx`, replace the current DPR assertion and add an exact renderer-options assertion:

```tsx
expect(props.frameloop).toBe("demand");
expect(props.dpr).toEqual([1, 2]);
expect(props.gl).toMatchObject({
  antialias: true,
  alpha: false,
  depth: true,
  stencil: false,
  precision: "highp",
  powerPreference: "high-performance",
});
expect(props.camera).toEqual({
  fov: CAMERA.fov,
  position: CAMERA.position,
  near: CAMERA.near,
  far: CAMERA.far,
});
```

- [ ] **Step 2: Make the browser suite exercise DPR 2 and assert real WebGL MSAA**

In `frontend/playwright.config.ts`, add the target device scale to the Chromium project:

```ts
{
  name: "chromium",
  use: {
    browserName: "chromium",
    deviceScaleFactor: 2,
    launchOptions: {
      args: [
        "--use-angle=swiftshader",
        "--enable-webgl",
        "--enable-unsafe-swiftshader",
      ],
    },
  },
},
```

In `expectRenderedTable` in `frontend/tests/task12.spec.ts`, replace the DPR range and drawing-buffer ceilings with the target values and inspect the real context:

```ts
expect(rendererPixelRatio).toBe(2);

const canvasQuality = await table.evaluate((node) => {
  const canvas = node.querySelector("canvas");
  if (!canvas) return null;
  const rect = canvas.getBoundingClientRect();
  const context = canvas.getContext("webgl2") ?? canvas.getContext("webgl");
  return {
    clientWidth: rect.width,
    clientHeight: rect.height,
    pixelWidth: canvas.width,
    pixelHeight: canvas.height,
    antialias: context?.getContextAttributes()?.antialias ?? false,
  };
});
expect(canvasQuality).not.toBeNull();
expect(canvasQuality?.antialias).toBe(true);
expect(canvasQuality?.pixelWidth).toBeCloseTo(
  (canvasQuality?.clientWidth ?? 0) * 2,
  0,
);
expect(canvasQuality?.pixelHeight).toBeCloseTo(
  (canvasQuality?.clientHeight ?? 0) * 2,
  0,
);
expect(canvasQuality?.pixelWidth).toBeLessThanOrEqual(3840);
expect(canvasQuality?.pixelHeight).toBeLessThanOrEqual(2160);
```

In the existing motion performance test, replace both range checks with exact stability checks:

```ts
const activeDpr = Number(await table.getAttribute("data-renderer-pixel-ratio"));
expect(activeDpr).toBe(2);
await expect(table).toHaveAttribute("data-animation-state", "idle", { timeout: 5_000 });
const idleDpr = Number(await table.getAttribute("data-renderer-pixel-ratio"));
expect(idleDpr).toBe(2);
```

Keep the existing `17.5ms`, `32ms`, `50ms`, and `1.5` performance assertions unchanged.

- [ ] **Step 3: Run the focused tests to verify RED**

Run:

```bash
npm test --prefix frontend -- src/game/three-table.test.tsx
cd frontend && npx playwright test tests/task12.spec.ts --project=chromium -g "accepts a 60fps-class motion budget"
```

Expected: the unit test reports `[1, 1.5]` instead of `[1, 2]`; the browser test reports renderer DPR `1.5` instead of `2` and/or `antialias: false`.

- [ ] **Step 4: Implement the minimal Canvas quality change**

In `frontend/src/game/three-table.tsx`, change only the quality-bearing props:

```tsx
<Canvas
  aria-hidden="true"
  frameloop={motion ? "always" : "demand"}
  dpr={[1, 2]}
  camera={{
    fov: CAMERA.fov,
    position: [...CAMERA.position],
    near: CAMERA.near,
    far: CAMERA.far,
  }}
  gl={{
    antialias: true,
    alpha: false,
    depth: true,
    stencil: false,
    precision: "highp",
    powerPreference: "high-performance",
  }}
```

Do not add adaptive DPR, post-processing, shadows, or a quality setting.

- [ ] **Step 5: Update the authoritative rendering spec**

In `docs/superpowers/specs/2026-09-19-r3f-cinematic-match-table-design.md`, replace:

```markdown
- DPR clamped to `[1, 1.5]`;
```

with:

```markdown
- device DPR clamped to `[1, 2]`, fixed for the renderer during idle and motion;
- default-framebuffer MSAA enabled and high-precision shaders requested;
```

Under `Verification and acceptance → Automated`, add:

```markdown
- Browser coverage runs the shared scene at device scale factor 2 and verifies the effective DPR, MSAA context attribute, drawing-buffer dimensions, and unchanged motion budgets.
```

- [ ] **Step 6: Run the focused tests to verify GREEN**

Run:

```bash
npm test --prefix frontend -- src/game/three-table.test.tsx
cd frontend && npx playwright test tests/task12.spec.ts --project=chromium -g "accepts a 60fps-class motion budget"
```

Expected: both commands pass; active and idle DPR are `2`; the existing frame-time bounds remain green under the DPR-2 Chromium project.

- [ ] **Step 7: Commit the renderer contract**

```bash
git add docs/superpowers/specs/2026-09-19-r3f-cinematic-match-table-design.md \
  frontend/src/game/three-table.tsx \
  frontend/src/game/three-table.test.tsx \
  frontend/playwright.config.ts \
  frontend/tests/task12.spec.ts
git commit -m "fix(frontend): render the 3d table at dpr 2"
```

### Task 2: Increase Texture Sampling Detail Without Atlas Bleed

**Files:**
- Modify: `frontend/src/game/tile-atlas.ts:9-15, 136-175`
- Modify: `frontend/src/game/tile-atlas.test.ts:65-140`
- Modify: `frontend/src/game/table-materials.ts:28-36`
- Modify: `frontend/src/game/table-materials.test.ts:31-69`

**Interfaces:**
- Consumes: the existing 300×400 SVG tile faces, `atlasCellUvBounds`, `TileAtlas.texture`, and `configureTableTexture`.
- Produces: a 256×342-per-cell Canvas atlas with unchanged cell coordinates and half-texel isolation, plus felt anisotropy capped by Three.js from a requested value of 4.

- [ ] **Step 1: Write failing atlas-resolution and felt-sampling assertions**

In `frontend/src/game/tile-atlas.test.ts`, update the deterministic drawing assertions and add backing-store checks:

```ts
expect(fillRect).toHaveBeenNthCalledWith(
  1,
  0,
  0,
  ATLAS_CELL_WIDTH,
  ATLAS_CELL_HEIGHT,
);
expect(ATLAS_CELL_WIDTH).toBe(256);
expect(ATLAS_CELL_HEIGHT).toBe(342);
expect((atlas.texture.image as HTMLCanvasElement).width).toBe(
  ATLAS_COLUMNS * ATLAS_CELL_WIDTH,
);
expect((atlas.texture.image as HTMLCanvasElement).height).toBe(
  atlas.rows * ATLAS_CELL_HEIGHT,
);
```

Retain the existing assertions that `generateMipmaps` is `false`, both filters are `LinearFilter`, UV bounds remain inside one cell, ordinary copies share a cell, and red fives use distinct cells.

In `frontend/src/game/table-materials.test.ts`, change the felt quality assertion:

```ts
expect(TABLE_TEXTURE_SPECS.felt.minFilter).toBe("mipmap");
expect(TABLE_TEXTURE_SPECS.felt.anisotropy).toBe(4);
```

- [ ] **Step 2: Run the texture tests to verify RED**

Run:

```bash
npm test --prefix frontend -- src/game/tile-atlas.test.ts src/game/table-materials.test.ts
```

Expected: atlas constants report `128×171` and felt anisotropy reports `1`.

- [ ] **Step 3: Double tile-atlas cells and raise bounded felt anisotropy**

In `frontend/src/game/tile-atlas.ts`, change only the cell constants:

```ts
export const ATLAS_COLUMNS = 8;
export const ATLAS_CELL_WIDTH = 256;
export const ATLAS_CELL_HEIGHT = 342;
export const ATLAS_CELL_INSET_TEXELS = 0.5;
```

Keep these existing sampling settings unchanged:

```ts
texture.generateMipmaps = false;
texture.minFilter = LinearFilter;
texture.magFilter = LinearFilter;
```

This avoids mip-level cross-cell bleed while supplying enough source pixels for DPR-2 local-hand tiles.

In `frontend/src/game/table-materials.ts`, change the felt spec only:

```ts
felt: {
  wrap: "repeat",
  repeat: [1.5, 1],
  minFilter: "mipmap",
  colorSpace: "srgb",
  anisotropy: 4,
},
```

Do not enlarge or replace the source art and do not add another texture pipeline.

- [ ] **Step 4: Run focused texture and scene tests to verify GREEN**

Run:

```bash
npm test --prefix frontend -- \
  src/game/tile-atlas.test.ts \
  src/game/table-materials.test.ts \
  src/game/three-table-scene.test.ts \
  src/game/three-table-layout.test.ts
```

Expected: all tests pass; atlas UV isolation, red-five separation, texture disposal, and authored scene geometry remain green.

- [ ] **Step 5: Capture and inspect the required quality evidence**

Run:

```bash
cd frontend && npx playwright test tests/task12.spec.ts --project=chromium \
  -g "captures the complete 4p scene during active motion at 1024x600"
```

Inspect both generated files:

- `frontend/test-results/task-12/4p-1024-active-motion.png`
- `frontend/test-results/task-12/4p-1024-idle-motion.png`

Acceptance checklist:

- diagonal table and tile edges no longer show the previous one-pixel staircase pattern at normal zoom;
- local-hand glyphs and circle/bamboo details remain distinct;
- no atlas neighbor color appears along any tile edge;
- red fives remain distinct from ordinary fives;
- active and idle captures have identical sharpness.

- [ ] **Step 6: Run full frontend verification**

Run:

```bash
npm test --prefix frontend
npm run typecheck --prefix frontend
npm run build --prefix frontend
cd frontend && npx playwright test tests/task12.spec.ts --project=chromium
```

Expected: all commands pass, including WebGL fallback/context-loss, accessibility, DPR-2 motion performance, and screenshot capture paths.

- [ ] **Step 7: Commit texture quality changes**

```bash
git add frontend/src/game/tile-atlas.ts \
  frontend/src/game/tile-atlas.test.ts \
  frontend/src/game/table-materials.ts \
  frontend/src/game/table-materials.test.ts
git commit -m "fix(frontend): sharpen 3d table textures"
```

## Final Verification

Run from the repository root:

```bash
git diff --check
cargo fmt --all -- --check
npm test --prefix frontend
npm run typecheck --prefix frontend
npm run build --prefix frontend
cd frontend && npx playwright test tests/task12.spec.ts --project=chromium
```

Record the following evidence in the completion report:

- effective idle and active renderer DPR at 1600×900 and 1920×1080;
- `antialias` context attribute;
- 1600×900 median frame time;
- 1920×1080 median and p90 frame times;
- paths to active and idle screenshots;
- whether the full frontend suite has any unrelated pre-existing failures.

Do not update the runtime binary until the user separately requests deployment.
