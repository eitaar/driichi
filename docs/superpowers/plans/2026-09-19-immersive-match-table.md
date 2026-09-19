# Immersive Match Table Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the dashboard-like gameplay layout with one shared, full-screen, faux-3D Pixi mahjong table for Live Match and Replay, backed by original generated table assets and unchanged authoritative game behavior.

**Architecture:** Keep React DOM responsible for controls and status while splitting the existing monolithic Pixi renderer into pure geometry, table art, and effects modules. Live Match and Replay consume the same `PixiTable`; only their DOM overlays differ. No Backend, projection, protocol, or Three.js work is included.

**Tech Stack:** React 19, TypeScript 7, PixiJS 8, Zustand 5, GSAP 3, Vitest 5, Playwright 1.63, Pi Image Generation

**Spec:** `docs/superpowers/specs/2026-09-19-immersive-match-table-design.md`

## Global Constraints

- React owns controls and status UI; PixiJS owns the table, tiles, hands, discards, melds, Character effects, and game animation.
- Live Match and Replay share `PixiTable`.
- Gameplay supports landscape viewports at least 1024 × 600; smaller viewports show guidance and no mobile gameplay is added.
- The logical table remains 1600 × 900 and letterboxes without distorting tiles.
- Human actions submit existing `decision_id` and `action_id` values; the Frontend never reconstructs canonical state or removes tiles optimistically.
- Three-player Matches use bottom, right, and left with no fourth Seat.
- UI copy remains English.
- No runtime CDN is allowed.
- No Three.js, Backend, domain, projection, or protocol changes.
- Generated table assets contain no characters, logos, readable text, UI labels, or copied ornamental marks.
- Reduced Motion removes positional, scale, and continuous motion while preserving state cues.

## Review Focus

- `remaining_wall` is absent, negative, fractional, or unexpectedly large: render a bounded non-negative integral count without crashing or fabricating exact wall geometry; covered in Task 2.
- A generated table texture fails to decode: use the procedural indigo-and-gold fallback while preserving playable tiles and controls; covered in Tasks 3 and 7.
- A Player has a long Display Name or missing Character portrait: keep score and identity legible without overlapping the hand; covered in Tasks 3 and 7.
- The viewport is exactly 1024 × 600 or just below it: render the complete playable surface at the boundary and guidance below it; covered in Tasks 5 and 7.
- An action is pending, rejected, or replaced by a newer Decision: prevent duplicate sends, preserve legal highlights, and permit retry only while the same Decision remains open; covered in Task 5.

---

## Planned File Structure

### New files

- `frontend/src/assets/table/table-felt.webp` — generated seamless indigo playing surface.
- `frontend/src/assets/table/table-rail.webp` — generated lacquer and aged-gold rail material.
- `frontend/src/assets/table/center-device.webp` — generated text-free center-device material.
- `frontend/src/assets/table/tile-back-material.webp` — generated gold-edged tile-back material.
- `frontend/src/assets/table/PROVENANCE.md` — exact generation prompts, method, dates, dimensions, usage statement, and hashes.
- `frontend/src/assets/table/SHA256SUMS` — checksums for generated table assets.
- `frontend/src/game/table-geometry.ts` — pure 1600 × 900 geometry, Seat anchors, perspective bounds, river placement, and wall distribution.
- `frontend/src/game/table-geometry.test.ts` — deterministic geometry and malformed-count tests.
- `frontend/src/game/table-art.ts` — Pixi drawing for the textured field, rails, center device, wall, Player frames, and procedural fallback.
- `frontend/src/game/table-effects.ts` — Pixi event-marker and result-portrait effects with Reduced Motion behavior.

### Modified files

- `frontend/src/game/pixi-table.tsx` — thin Pixi lifecycle adapter and shared Live/Replay scene coordinator.
- `frontend/src/game/gameplay.tsx` — full-table Live DOM overlays, local Action row timer, Toasts, and blocking states.
- `frontend/src/replay.tsx` — Replay controls and status moved into the table overlay.
- `frontend/src/styles.css` — immersive Match/Replay layout, overlay safe areas, focus, fallback, and responsive boundary rules.
- `frontend/src/game/task12.test.ts` — scene/effect invariants that remain independent of a browser.
- `frontend/tests/fixtures/task12-projection.json` — explicit long-name and wall-count cases where needed.
- `frontend/tests/task12.spec.ts` — Live table, interaction, viewport, fallback, and screenshots.
- `frontend/tests/task15.spec.ts` — Replay table and floating transport coverage.
- `frontend/tests/task16-real-server.spec.ts` — real-server accessibility selector and end-to-end screenshot compatibility.

---

### Task 1: Generate and Record the Original Table Asset Set

**Files:**
- Create: `frontend/src/assets/table/table-felt.webp`
- Create: `frontend/src/assets/table/table-rail.webp`
- Create: `frontend/src/assets/table/center-device.webp`
- Create: `frontend/src/assets/table/tile-back-material.webp`
- Create: `frontend/src/assets/table/PROVENANCE.md`
- Create: `frontend/src/assets/table/SHA256SUMS`

**Interfaces:**
- Consumes: the approved palette and image-generation constraints in the spec.
- Produces: four local WebP files imported by `table-art.ts` through `new URL(..., import.meta.url).href`.

- [ ] **Step 1: Generate the table-felt texture with Pi Image Generation**

Use `codex_generate_image` in Pi tool mode with `save: "project"` and this exact prompt:

```text
Use case: stylized-concept
Asset type: seamless game-table material for a 2D PixiJS mahjong scene
Primary request: create a square deep-indigo woven table-felt texture with subtle tonal variation and premium game-art finish
Style/medium: polished hand-authored game material, physically plausible woven felt, restrained detail
Composition/framing: straight top-down orthographic material swatch, edge-to-edge, visually tileable, no central focal point
Lighting/mood: diffuse even lighting with no cast shadow and no directional hotspot
Color palette: near-black navy, deep indigo, muted blue; no green
Materials/textures: fine woven fibers, very subtle wear, low contrast so ivory mahjong tiles remain dominant
Constraints: no text, no symbols, no logos, no border, no objects, no characters, no watermark, no perspective, no vignette
Avoid: bright speckles, visible seams, dramatic folds, photographic table objects
```

Copy the selected output to `frontend/src/assets/table/table-felt.webp`, leaving the Pi-generated original in place. Inspect all four edges at 100% zoom; if a hard seam or focal hotspot is visible, make one targeted regeneration asking only to remove that defect.

- [ ] **Step 2: Generate the rail texture**

Use a separate `codex_generate_image` call:

```text
Use case: stylized-concept
Asset type: repeatable rail material for a faux-3D mahjong game table
Primary request: create a wide dark lacquer material strip with restrained aged-gold inlay suitable for slicing across table rails
Style/medium: premium Japanese-inspired game UI material, original and non-branded
Composition/framing: straight-on orthographic horizontal material strip, repeatable center section, generous clean margins
Lighting/mood: controlled studio sheen, soft edge highlights, no cast shadow
Color palette: blackened navy lacquer, antique muted gold, tiny warm brown undertones
Materials/textures: smooth lacquer, lightly brushed metal, subtle age without damage
Constraints: no text, no symbols, no logos, no characters, no mahjong tiles, no watermark, no perspective scene
Avoid: ornate copied motifs, bright yellow gold, jewelry-like filigree, strong reflections
```

Copy the selected output to `frontend/src/assets/table/table-rail.webp`.

- [ ] **Step 3: Generate the center-device material**

Use a separate `codex_generate_image` call:

```text
Use case: stylized-concept
Asset type: text-free center console surface for a digital mahjong table
Primary request: create a square top-down center-device surface with a dark indigo body, aged-gold bevels, and four subtle directional quadrants
Style/medium: premium original game UI prop, clean and readable at small size
Composition/framing: exact top-down orthographic square, symmetrical around the center, empty central display area reserved for live Pixi text
Lighting/mood: soft internal edge glow, restrained highlights
Color palette: near-black navy, deep indigo, antique gold, minimal muted vermilion detail
Materials/textures: lacquered composite body and brushed metal trim
Constraints: no text, no numbers, no logos, no characters, no tiles, no watermark, no perspective
Avoid: copied game interfaces, overly ornate decoration, bright neon, busy center area
```

Copy the selected output to `frontend/src/assets/table/center-device.webp`.

- [ ] **Step 4: Generate the tile-back material**

Use a separate `codex_generate_image` call:

```text
Use case: stylized-concept
Asset type: square material swatch for concealed mahjong tile backs
Primary request: create a clean dark-indigo tile-back surface with a thin aged-gold edge treatment
Style/medium: polished original game asset material, readable when cropped onto many small tiles
Composition/framing: flat top-down square swatch, centered, no perspective, no external shadow
Lighting/mood: even soft lighting, restrained bevel highlight
Color palette: deep indigo, near-black navy, antique gold
Materials/textures: smooth enamel center and narrow brushed-metal edge
Constraints: no text, no symbols, no logos, no characters, no watermark
Avoid: intricate patterns, high-frequency noise, bright yellow, photographic background
```

Copy the selected output to `frontend/src/assets/table/tile-back-material.webp`.

- [ ] **Step 5: Record immutable provenance and checksums**

Create `frontend/src/assets/table/PROVENANCE.md` with one section per file containing the exact prompt above, `Pi codex_generate_image` as the method, the backend-reported output metadata, the generation date, inspected pixel dimensions, and this usage statement:

```markdown
These original generated table materials contain no copied characters, logos, text, or proprietary UI elements. They are project-local decorative assets and do not replace the separately licensed Character Pack or tile-face assets.
```

Generate checksums:

```bash
cd frontend/src/assets/table
sha256sum table-felt.webp table-rail.webp center-device.webp tile-back-material.webp > SHA256SUMS
sha256sum -c SHA256SUMS
file *.webp
```

Expected: all four checksum lines report `OK`; `file` reports readable WebP images with non-zero dimensions.

- [ ] **Step 6: Prove the assets enter the production bundle**

Run:

```bash
cd frontend
npm run build
```

Expected: exit 0. The later `table-art.ts` imports do not exist yet, so this step proves the files themselves do not disturb the build; Task 3 proves bundling after imports.

- [ ] **Step 7: Commit**

```bash
git add frontend/src/assets/table
git commit -m "assets: add generated immersive table materials"
```

---

### Task 2: Extract Deterministic Table Geometry

**Files:**
- Create: `frontend/src/game/table-geometry.ts`
- Create: `frontend/src/game/table-geometry.test.ts`
- Modify: `frontend/src/game/pixi-table.tsx`

**Interfaces:**
- Produces:
  - `TABLE_WIDTH: 1600`
  - `TABLE_HEIGHT: 900`
  - `TABLE_RATIO: number`
  - `TablePoint { x: number; y: number; rotation: number }`
  - `TableSeatGeometry { seat: number; position: TableSeatPosition; hand: TablePoint; frame: TablePoint }`
  - `tableSeatGeometry(mode: string | undefined, viewerSeat?: number): TableSeatGeometry[]`
  - `discardPlacement(position: TableSeatPosition, index: number): TablePoint`
  - `wallTileCount(value: unknown, maximum?: number): number`
  - `wallPlacements(value: unknown): Array<TablePoint & { edge: TableSeatPosition }>`
- Consumes: `seatPositions()` and `TableSeatPosition` from `orientation.ts`.

- [ ] **Step 1: Write failing geometry tests**

Create `frontend/src/game/table-geometry.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import {
  TABLE_HEIGHT,
  TABLE_RATIO,
  TABLE_WIDTH,
  tableSeatGeometry,
  wallPlacements,
  wallTileCount,
} from "./table-geometry";

describe("immersive table geometry", () => {
  it("keeps the established 1600 by 900 scene", () => {
    expect([TABLE_WIDTH, TABLE_HEIGHT, TABLE_RATIO]).toEqual([
      1600,
      900,
      1600 / 900,
    ]);
  });

  it("puts the viewer at bottom and never invents a fourth sanma Seat", () => {
    expect(tableSeatGeometry("4p-red-east", 2).map(({ seat, position }) => [seat, position])).toEqual([
      [2, "bottom"],
      [3, "right"],
      [0, "top"],
      [1, "left"],
    ]);
    expect(tableSeatGeometry("3p-red-east", 1).map(({ seat, position }) => [seat, position])).toEqual([
      [1, "bottom"],
      [2, "right"],
      [0, "left"],
    ]);
  });

  it("normalizes malformed wall counts without exceeding a full wall", () => {
    expect(wallTileCount(undefined)).toBe(0);
    expect(wallTileCount(-2)).toBe(0);
    expect(wallTileCount(3.9)).toBe(3);
    expect(wallTileCount(999)).toBe(136);
  });

  it("creates one deterministic in-bounds placement per remaining tile", () => {
    const placements = wallPlacements(69);
    expect(placements).toHaveLength(69);
    expect(new Set(placements.map(({ x, y }) => `${x}:${y}`)).size).toBe(69);
    for (const point of placements) {
      expect(point.x).toBeGreaterThanOrEqual(0);
      expect(point.x).toBeLessThanOrEqual(TABLE_WIDTH);
      expect(point.y).toBeGreaterThanOrEqual(0);
      expect(point.y).toBeLessThanOrEqual(TABLE_HEIGHT);
    }
  });
});
```

- [ ] **Step 2: Run the tests and verify RED**

Run:

```bash
cd frontend
npm test -- --run src/game/table-geometry.test.ts
```

Expected: FAIL because `./table-geometry` does not exist.

- [ ] **Step 3: Implement the pure geometry module**

Create `frontend/src/game/table-geometry.ts` with these exact exports and rules:

```ts
import { seatPositions, type TableSeatPosition } from "./orientation";

export const TABLE_WIDTH = 1600;
export const TABLE_HEIGHT = 900;
export const TABLE_RATIO = TABLE_WIDTH / TABLE_HEIGHT;

export interface TablePoint {
  x: number;
  y: number;
  rotation: number;
}

export interface TableSeatGeometry {
  seat: number;
  position: TableSeatPosition;
  hand: TablePoint;
  frame: TablePoint;
}

const SEAT_POINTS: Record<TableSeatPosition, Omit<TableSeatGeometry, "seat" | "position">> = {
  bottom: {
    hand: { x: 800, y: 808, rotation: 0 },
    frame: { x: 174, y: 730, rotation: 0 },
  },
  right: {
    hand: { x: 1460, y: 450, rotation: Math.PI / 2 },
    frame: { x: 1430, y: 132, rotation: 0 },
  },
  top: {
    hand: { x: 800, y: 82, rotation: Math.PI },
    frame: { x: 1180, y: 98, rotation: 0 },
  },
  left: {
    hand: { x: 140, y: 450, rotation: -Math.PI / 2 },
    frame: { x: 170, y: 132, rotation: 0 },
  },
};

export function tableSeatGeometry(
  mode: string | undefined,
  viewerSeat?: number,
): TableSeatGeometry[] {
  return seatPositions(mode, viewerSeat).map(({ seat, position }) => ({
    seat,
    position,
    ...SEAT_POINTS[position],
  }));
}

export function wallTileCount(value: unknown, maximum = 136): number {
  return typeof value === "number" && Number.isFinite(value)
    ? Math.min(maximum, Math.max(0, Math.floor(value)))
    : 0;
}
```

Implement `discardPlacement()` by moving the existing pure function from `pixi-table.tsx` unchanged. Implement `wallPlacements()` with four fixed edge lanes, round-robin distribution in `["bottom", "right", "top", "left"]` order, 34 slots per edge, and Seat-oriented rotation. Use only integer count input from `wallTileCount()`.

- [ ] **Step 4: Replace duplicated constants and coordinate helpers**

In `pixi-table.tsx`, import `TABLE_WIDTH`, `TABLE_HEIGHT`, `TABLE_RATIO`, `discardPlacement`, and `tableSeatGeometry`. Delete the local constants, `playerCoordinates()`, and local `discardPlacement()`. Replace the Seat loop's separate `seatPositions()` and `playerCoordinates()` calls with `tableSeatGeometry(mode, viewerSeat)`.

- [ ] **Step 5: Run focused and existing unit tests**

```bash
cd frontend
npm test -- --run src/game/table-geometry.test.ts src/game/task12.test.ts
npm run typecheck
```

Expected: PASS and exit 0.

- [ ] **Step 6: Commit**

```bash
git add frontend/src/game/table-geometry.ts frontend/src/game/table-geometry.test.ts frontend/src/game/pixi-table.tsx
git commit -m "refactor(frontend): extract immersive table geometry"
```

---

### Task 3: Build the Shared Textured Pixi Table Scene

**Files:**
- Create: `frontend/src/game/table-art.ts`
- Modify: `frontend/src/game/pixi-table.tsx`
- Modify: `frontend/src/game/task12.test.ts`
- Modify: `frontend/tests/task12.spec.ts`

**Interfaces:**
- Consumes from Task 2: `TablePoint`, `TableSeatGeometry`, `tableSeatGeometry()`, and `wallPlacements()`.
- Produces:
  - `TableArtAssets { felt: Texture | null; rail: Texture | null; center: Texture | null; tileBack: Texture | null }`
  - `TextureLoader = (source: string) => Promise<Texture | null>`
  - `loadTableArtAssets(loadTexture: TextureLoader): Promise<TableArtAssets>`
  - `drawTableSkin(context: TableArtContext): number`
  - `drawCenterDevice(context: CenterDeviceContext): void`
  - `drawPlayerFrame(context: PlayerFrameContext): Promise<void>`
  - `drawWall(context: WallContext): Promise<number>`
  - Pixi host instrumentation: `data-skin-ready`, `data-skin-fallback`, `data-player-frame-count`, `data-wall-tile-count`.

- [ ] **Step 1: Extend the browser assertion so the old scene fails**

In `frontend/tests/task12.spec.ts`, extend `expectRenderedTable()`:

```ts
await expect(table).toHaveAttribute("data-skin-ready", "true", {
  timeout: 20_000,
});
await expect(table).toHaveAttribute("data-player-frame-count", /^[34]$/);
await expect(table).toHaveAttribute("data-wall-tile-count", /^\d+$/);
```

Inside the existing gameplay matrix test, add:

```ts
await expect(table).toHaveAttribute(
  "data-wall-tile-count",
  mode === "3p-red-east" ? "54" : "69",
);
await expect(table).toHaveAttribute(
  "data-player-frame-count",
  mode === "3p-red-east" ? "3" : "4",
);
```

- [ ] **Step 2: Run one browser case and verify RED**

```bash
cd frontend
npx playwright test tests/task12.spec.ts --grep "captures 4p-red-east authoritative gameplay at 1440x900"
```

Expected: FAIL because the new attributes are absent.

- [ ] **Step 3: Add table-art asset imports and public contexts**

At the top of `table-art.ts`, define the bundled URLs:

```ts
const TABLE_FELT_SOURCE = new URL(
  "../assets/table/table-felt.webp",
  import.meta.url,
).href;
const TABLE_RAIL_SOURCE = new URL(
  "../assets/table/table-rail.webp",
  import.meta.url,
).href;
const CENTER_DEVICE_SOURCE = new URL(
  "../assets/table/center-device.webp",
  import.meta.url,
).href;
const TILE_BACK_MATERIAL_SOURCE = new URL(
  "../assets/table/tile-back-material.webp",
  import.meta.url,
).href;
```

Export `TextureLoader` and implement `loadTableArtAssets()` as one `Promise.all` over the four URLs, calling the supplied loader for each source. `pixi-table.tsx` supplies its existing timeout-aware `loadSource(Assets, source, image => Texture.from(image))` wrapper, so table art and tiles share one failure policy.

Export contexts that receive Pixi constructors and existing callbacks instead of importing React:

```ts
export interface TableArtContext {
  root: import("pixi.js").Container;
  assets: TableArtAssets;
  Graphics: typeof import("pixi.js").Graphics;
  Sprite: typeof import("pixi.js").Sprite;
  drawText: DrawText;
}

export type DrawText = (
  container: import("pixi.js").Container,
  value: string,
  x: number,
  y: number,
  size: number,
  fill?: number,
  family?: string,
) => import("pixi.js").Text;
```

Define equivalent explicit `CenterDeviceContext`, `PlayerFrameContext`, and `WallContext` interfaces with only the projection/player/room/geometry/texture-loader values each function needs.

- [ ] **Step 4: Draw the faux-3D skin with a procedural fallback**

`drawTableSkin()` must:

1. Draw a near-black full-scene background.
2. Draw a trapezoidal outer rail from `(40, 26)`, `(1560, 26)`, `(1510, 874)`, `(90, 874)`.
3. Draw an inset indigo field from `(180, 108)`, `(1420, 108)`, `(1320, 794)`, `(280, 794)`.
4. Use the generated felt and rail textures when both loaded textures are usable.
5. Otherwise use solid `0x101932` field fill, `0x14131a` rail fill, and `0xb99a5d` strokes.
6. Return the number of drawn table primitives for existing instrumentation.

`drawCenterDevice()` must place the generated device at the center when usable, draw a gold-edged procedural octagonal fallback otherwise, and render Kyoku, Honba, Kyotaku, remaining-wall count, and Dora with existing projection values. It must not render Player scores.

- [ ] **Step 5: Draw Player frames and the visual wall**

`drawPlayerFrame()` must load `/assets/characters/<id>/portrait.webp`, crop it into a 96 × 112 portrait frame, render a neutral initial/kind fallback on failure, and render Display Name, score, physical position, and `RIICHI` when active. Clamp the visible name to 18 Unicode code points plus `…`, but keep the full name in the Live DOM status text already supplied by the React surface.

`drawWall()` must call `wallPlacements(projection.remaining_wall)`, draw one back per placement from `tile-back-material.webp`, and fall back to the existing Regular `Back.svg` if the generated material cannot load. Return the number drawn.

- [ ] **Step 6: Rebuild the shared scene coordinator**

In `pixi-table.tsx`:

- initialize the four new host attributes to `false` or `0`;
- load table-art assets once during mount;
- call `drawTableSkin()` before drawing state;
- call `drawPlayerFrame()` once for each geometry entry with an actual projected Player;
- call `drawWall()` from `remaining_wall`;
- call `drawCenterDevice()` after rivers and before result portrait effects;
- preserve tile loading, projection version cancellation, resize, destroy, and asset unload behavior;
- stop drawing the old rectangular shell, crosshair, duplicated scoreboard labels, and old center labels.

Set instrumentation only for the active render version:

```ts
host.dataset.skinReady = "true";
host.dataset.skinFallback = tableArtUsesFallback ? "true" : "false";
host.dataset.playerFrameCount = String(renderedPlayerFrames);
host.dataset.wallTileCount = String(renderedWallTiles);
```

- [ ] **Step 7: Add a pure invariant for long names**

Export `displayPlayerName(value: string): string` from `table-art.ts` and add to `task12.test.ts`:

```ts
it("bounds long table labels without changing the source Display Name", () => {
  const name = "A very long participant display name";
  expect(displayPlayerName(name)).toBe("A very long partic…");
  expect(name).toBe("A very long participant display name");
});
```

- [ ] **Step 8: Run focused verification**

```bash
cd frontend
npm test -- --run src/game/table-geometry.test.ts src/game/task12.test.ts
npm run typecheck
npx playwright test tests/task12.spec.ts --grep "captures"
npm run build
```

Expected: all commands exit 0; screenshots show the textured indigo-and-gold table in both 3p and 4p.

- [ ] **Step 9: Commit**

```bash
git add frontend/src/game/table-art.ts frontend/src/game/pixi-table.tsx frontend/src/game/task12.test.ts frontend/tests/task12.spec.ts
git commit -m "feat(frontend): render immersive shared mahjong table"
```

---

### Task 4: Extract Restrained Table Effects and Fallback Behavior

**Files:**
- Create: `frontend/src/game/table-effects.ts`
- Modify: `frontend/src/game/pixi-table.tsx`
- Modify: `frontend/src/game/task12.test.ts`
- Modify: `frontend/tests/task12.spec.ts`

**Interfaces:**
- Consumes: `AnimationItem`, `animationVisualForKind()`, `PortraitEffect`, the active Pixi stage, and a `DrawText` callback.
- Produces:
  - `runTableAnimation(context: TableAnimationContext): () => void`
  - `drawResultPortrait(context: ResultPortraitContext): Promise<boolean>`
  - `effectDuration(kind: AnimationKind, reducedMotion: boolean): number`

- [ ] **Step 1: Write failing Reduced Motion effect tests**

Add to `task12.test.ts`:

```ts
import { effectDuration } from "./table-effects";

it("removes transition time under Reduced Motion", () => {
  expect(effectDuration("discard", true)).toBe(0);
  expect(effectDuration("win", true)).toBe(0);
  expect(effectDuration("discard", false)).toBe(240);
  expect(effectDuration("win", false)).toBe(520);
});
```

- [ ] **Step 2: Verify RED**

```bash
cd frontend
npm test -- --run src/game/task12.test.ts
```

Expected: FAIL because `table-effects.ts` does not exist.

- [ ] **Step 3: Move event marker and portrait rendering into table-effects.ts**

`effectDuration()` returns `0` under Reduced Motion and `animationVisualForKind(kind).duration` otherwise.

`runTableAnimation()` must consume one queue item, draw the same event-specific shape vocabulary already returned by `animationVisualForKind()`, position it around the center device, and return a stop function. Under Reduced Motion it calls `onConsumed(item.id)` synchronously and draws nothing.

`drawResultPortrait()` must preserve the existing Mangan-or-higher content: Display Name, Ron/Tsumo, Han/Fu, limit, and points. Use the new indigo/gold frame, return `false` when the Character portrait is unavailable, and never substitute another Character.

- [ ] **Step 4: Replace the in-component effect code**

Delete the marker ticker and result-portrait drawing blocks from `pixi-table.tsx`. Call `runTableAnimation()` from the scene's `animate()` method and `drawResultPortrait()` from the active render. Preserve `data-portrait-ready`, `data-portrait-effect`, `data-portrait-name`, and `data-portrait-result` exactly because current browser tests and external diagnostics use them.

- [ ] **Step 5: Add generated-texture failure coverage**

In `task12.spec.ts`, add:

```ts
test("falls back to procedural table art when generated textures fail", async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 900 });
  await page.route(/\/(table-felt|table-rail|center-device|tile-back-material)-?.*\.webp$/, (route) => route.abort());
  await installCharacterFixtures(page);
  await installSocket(page, "4p-red-east");
  await page.goto("/room/123456/lobby");
  const table = await expectRenderedTable(page);
  await expect(table).toHaveAttribute("data-skin-fallback", "true");
  await expect(page.locator(".table-tile-hit.is-legal")).toHaveCount(14);
});
```

- [ ] **Step 6: Run focused tests**

```bash
cd frontend
npm test -- --run src/game/task12.test.ts
npm run typecheck
npx playwright test tests/task12.spec.ts --grep "Mangan|Reduced Motion|generated textures"
```

Expected: PASS. If the existing file has no Reduced Motion browser case yet, Task 7 adds the complete matrix; do not weaken this command's other cases.

- [ ] **Step 7: Commit**

```bash
git add frontend/src/game/table-effects.ts frontend/src/game/pixi-table.tsx frontend/src/game/task12.test.ts frontend/tests/task12.spec.ts
git commit -m "refactor(frontend): isolate restrained table effects"
```

---

### Task 5: Replace the Live Dashboard Rail with Accessible Table Overlays

**Files:**
- Modify: `frontend/src/game/gameplay.tsx`
- Modify: `frontend/src/styles.css`
- Modify: `frontend/tests/task12.spec.ts`

**Interfaces:**
- Consumes: existing `GameplayProps`, `ProjectedDecision`, `submitAction()`, `AudioManager`, and `PixiTable`.
- Produces:
  - `ActionDeck` receives `remaining: number | null` and renders the timer inside the local Action row.
  - `GameplayToast` renders recoverable connection/action notices.
  - `GameplayControls` renders compact Settings and connection state in the upper-right safe area.
  - `GameplayPlayerStatus` preserves full Player names and scores in visually hidden semantic DOM after the visual score rail is removed.
  - Existing `data-testid="action-deck"` and `data-testid="decision-timer"` remain stable.

- [ ] **Step 1: Update browser expectations to the approved composition**

In the existing capture test, add:

```ts
await expect(page.locator(".gameplay-topbar")).toHaveCount(0);
await expect(page.locator(".gameplay-rail")).toHaveCount(0);
await expect(page.locator(".gameplay-controls")).toBeVisible();
await expect(page.getByTestId("action-deck")).toBeVisible();
```

Add a no-Decision case:

```ts
test("omits the Action row when no Decision is open", async ({ page }) => {
  const state = structuredClone(projectionFixture.projections["4p-red-east"]);
  state.decision = null;
  await page.setViewportSize({ width: 1280, height: 720 });
  await installCharacterFixtures(page);
  await installSocket(page, "4p-red-east", state);
  await page.goto("/room/123456/lobby");
  await expectRenderedTable(page);
  await expect(page.getByTestId("action-deck")).toHaveCount(0);
});
```

- [ ] **Step 2: Verify RED against the current top bar and rail**

```bash
cd frontend
npx playwright test tests/task12.spec.ts --grep "captures 4p-red-east authoritative gameplay at 1440x900|omits the Action row"
```

Expected: FAIL because the old top bar and rail still render and the empty Action deck remains.

- [ ] **Step 3: Move the timer into ActionDeck and omit the empty deck**

Change the `ActionDeck` props to include `remaining: number | null`. When `decision` is absent, return `null`. Inside `.action-deck-label`, render:

```tsx
<span
  data-testid="decision-timer"
  aria-label={`Decision timer: ${formatSeconds(remaining)} seconds`}
>
  {remaining === null ? "TIMER —" : `${formatSeconds(remaining)}s`}
</span>
```

Keep the current buttons, candidate behavior, pending `aria-busy`, action IDs, and disable rules unchanged.

- [ ] **Step 4: Replace Live Match chrome**

In `GameplaySurface`:

- remove `gameplay-topbar`, `table-caption`, `score-rail`, and `gameplay-rail`;
- render one `.gameplay-controls` overlay containing compact connection state and `AudioSettingsPanel`;
- render one `.gameplay-toast-stack` with recoverable `commandError`, `actionError`, and reconnect information using `role="status"` for reconnect and `role="alert"` for action errors;
- render blocking `closeMessage` reasons in `.gameplay-blocking-state` instead of a narrow inline notice;
- render `ActionDeck` inside `.table-letterbox` after `TileHitLayer`, passing `timer`;
- render `ResultsPanel` as `.results-overlay` over the table only in Post-Match;
- render a visually hidden `GameplayPlayerStatus` list from `projection.players`, with each item's full Display Name, formatted score, Seat position, and Riichi state so removing the visual score rail does not remove its semantic information.

Do not change `submit()`, pending clearing, legal-discard mapping, WebSocket state, or result derivation.

- [ ] **Step 5: Replace gameplay CSS with the full-table layout**

Keep non-gameplay rules untouched. Rewrite the Task 12 gameplay block so:

```css
.gameplay-main {
  width: 100%;
  min-height: 100dvh;
  display: grid;
  place-items: center;
  padding: 12px;
}
.table-letterbox {
  width: min(100%, calc(100dvh * 1.7778));
  max-height: 100dvh;
  aspect-ratio: 16 / 9;
  position: relative;
  overflow: hidden;
}
.gameplay-controls {
  position: absolute;
  z-index: 6;
  inset-block-start: max(14px, env(safe-area-inset-top));
  inset-inline-end: max(16px, env(safe-area-inset-right));
}
.action-deck {
  position: absolute;
  z-index: 5;
  inset-inline: 18%;
  inset-block-end: 18%;
}
```

Use logical inset properties. Give Action buttons opaque navy surfaces, aged-gold borders, visible focus, and enough contrast over the table. Add a reusable `.visually-hidden` utility using the standard 1px clipped pattern for `GameplayPlayerStatus`; do not use `display: none` or `visibility: hidden`. At `1024 × 600`, reduce gaps and button height without clipping the hand or Action row. Preserve the existing below-minimum guidance media query.

- [ ] **Step 6: Pin pending/rejection behavior at the new location**

Keep the existing authoritative-send and Riichi-pending browser tests. Add an action rejection emission after the first send and assert:

```ts
await expect(page.getByRole("alert")).toContainText("illegal_action");
await expect(page.getByTestId("action-deck")).toHaveAttribute("aria-busy", "false");
await expect(page.locator(".table-tile-hit.is-legal")).toHaveCount(14);
```

Then click once and assert the socket sent exactly two messages with the same current `decision_id`, never a third duplicate while pending.

- [ ] **Step 7: Run Live checks**

```bash
cd frontend
npm run typecheck
npm test -- --run src/game/task12.test.ts
npx playwright test tests/task12.spec.ts
```

Expected: PASS, including the exact 1024 × 600 case and the existing 1023 × 599 guidance case.

- [ ] **Step 8: Commit**

```bash
git add frontend/src/game/gameplay.tsx frontend/src/styles.css frontend/tests/task12.spec.ts
git commit -m "feat(frontend): move live actions onto the table"
```

---

### Task 6: Float Replay Transport Controls over the Shared Table

**Files:**
- Modify: `frontend/src/replay.tsx`
- Modify: `frontend/src/styles.css`
- Modify: `frontend/tests/task15.spec.ts`

**Interfaces:**
- Consumes: existing Replay frame state, `selectPosition()`, speed state, and shared `PixiTable`.
- Produces: `.replay-table-stage` containing the Pixi table, event-status overlay, and `.replay-controls-overlay`; event log remains outside.

- [ ] **Step 1: Write the failing Replay layout assertion**

In the existing Replay-viewer browser test, add:

```ts
const stage = page.locator(".replay-table-stage");
await expect(stage).toBeVisible();
await expect(stage.locator(".replay-controls-overlay")).toBeVisible();
await expect(stage.locator(".replay-status-toast")).toBeVisible();
await expect(page.locator(".replay-event-log")).toBeVisible();
await expect(page.locator(".replay-table-wrap + .replay-controls")).toHaveCount(0);
```

- [ ] **Step 2: Verify RED**

```bash
cd frontend
npx playwright test tests/task15.spec.ts --grep "viewer|Replay"
```

Expected: FAIL because the stage and overlay classes do not exist.

- [ ] **Step 3: Move existing controls without changing Replay state behavior**

In `ReplayViewer`, replace the separate table/status/control siblings with:

```tsx
<div className="replay-table-stage">
  <div className="replay-table-wrap">
    <PixiTable
      projection={frame.visible_state as ProjectedState}
      room={room}
      reducedMotion={reducedMotion}
      portraitEffect={portraitEffect}
    />
  </div>
  <div className="replay-status-toast" role="status" aria-live="polite">
    <span className="state-label">EVENT SIGNAL</span>
    <strong>{statusText}</strong>
  </div>
  <div className="replay-controls replay-controls-overlay" aria-label="Replay controls">
    {/* move the existing Play/Pause, Previous, Next, speed, and Kyoku controls here unchanged */}
  </div>
</div>
```

Move the existing controls verbatim; do not rename accessible labels, change speed values, or alter `selectPosition()`.

- [ ] **Step 4: Style the floating transport safely**

Make `.replay-table-stage` position-relative and keep the table at 16:9. Position `.replay-controls-overlay` along the lower safe edge with a near-opaque navy background and gold border. Position `.replay-status-toast` along the upper-left safe edge. At widths below 900px, allow the controls to wrap within the stage rather than overflow. Keep the event log in normal document flow below the stage.

- [ ] **Step 5: Run Replay checks**

```bash
cd frontend
npm run typecheck
npm test -- --run src/replay.test.tsx
npx playwright test tests/task15.spec.ts
```

Expected: PASS; Play/Pause, Previous/Next, speed, Kyoku jump, generic/silent mode, and event log continue to work.

- [ ] **Step 6: Commit**

```bash
git add frontend/src/replay.tsx frontend/src/styles.css frontend/tests/task15.spec.ts
git commit -m "feat(frontend): float replay controls over the table"
```

---

### Task 7: Complete the Visual, Accessibility, and Viewport Acceptance Matrix

**Files:**
- Modify: `frontend/tests/fixtures/task12-projection.json`
- Modify: `frontend/tests/task12.spec.ts`
- Modify: `frontend/tests/task15.spec.ts`
- Modify: `frontend/tests/task16-real-server.spec.ts`
- Modify: `frontend/src/styles.css`

**Interfaces:**
- Consumes: the completed Live and Replay surfaces.
- Produces: deterministic screenshots and assertions for every supported design state in the spec.

- [ ] **Step 1: Expand the viewport matrix**

Change the Task 12 matrix to:

```ts
for (const viewport of [
  { width: 1024, height: 600, label: "1024x600" },
  { width: 1280, height: 720, label: "1280x720" },
  { width: 1600, height: 900, label: "1600x900" },
  { width: 1920, height: 1080, label: "1920x1080" },
]) {
  // retain both 3p-red-east and 4p-red-east
}
```

For every case, assert non-zero canvas size, 16:9 ratio, correct Player-frame count, exact wall-tile count, visible local hand, visible Action row, and in-viewport bounding boxes for `.gameplay-controls`, `.action-deck`, and all legal tile hits.

- [ ] **Step 2: Add long-name and missing-portrait coverage**

Clone the 4p fixture in the test, set one `display_name` to `A very long participant display name`, and route that Character portrait to 404. Assert `data-player-frame-count="4"`, `data-render-ready="true"`, and that gameplay remains actionable. Save `test-results/immersive-table/4p-long-name-fallback.png`.

- [ ] **Step 3: Add Reduced Motion browser coverage**

Before page load:

```ts
await page.emulateMedia({ reducedMotion: "reduce" });
```

Emit a discard event, assert `data-motion="static"`, and wait for the animation queue to be consumed without a visible animated marker. Save `test-results/immersive-table/4p-reduced-motion.png`.

- [ ] **Step 4: Add accessibility checks to the new overlays**

Import `AxeBuilder` from `@axe-core/playwright` in `task12.spec.ts` and `task15.spec.ts`. Assert no serious or critical violations after opening Settings, after opening the candidate dialog, and on Replay with the floating controls. In `task16-real-server.spec.ts`, replace `axe.include([".gameplay-topbar", ".gameplay-main"])` with `axe.include(".gameplay-main")` because the approved composition removes `.gameplay-topbar`. Also assert keyboard-only access:

1. Tab to Settings and open it.
2. Close it and tab to the first legal Action.
3. Open a multi-candidate dialog.
4. Escape and verify focus returns.
5. On Replay, tab through Play, Previous, Next, speed, and Kyoku select.

- [ ] **Step 5: Cover synchronization and terminal connection states**

Extend `installSocket()` with a final `snapshotDelayMs = 0` parameter and pass it through the init-script argument. Replace the fixed `setTimeout(..., 0)` with `setTimeout(..., snapshotDelayMs)`.

Add the synchronization test:

```ts
test("shows in-table synchronization before the first projection", async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 720 });
  await installCharacterFixtures(page);
  await installSocket(page, "4p-red-east", undefined, 2_000);
  await page.goto("/room/123456/lobby");
  await expect(page.getByText("Waiting for an authoritative projection")).toBeVisible();
  await expect(page.getByTestId("action-deck")).toHaveCount(0);
});
```

Add the terminal close test:

```ts
test("blocks play when the Room is deleted", async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 720 });
  await installCharacterFixtures(page);
  await installSocket(page, "4p-red-east");
  await page.goto("/room/123456/lobby");
  await expectRenderedTable(page);
  await page.evaluate(() => {
    const socket = (window as unknown as {
      __socket: { onclose: ((event: { code: number; reason: string }) => void) | null };
    }).__socket;
    socket.onclose?.({ code: 4002, reason: "room_deleted" });
  });
  await expect(page.getByRole("alert")).toContainText("deleted this Room");
  await expect(page.locator(".table-tile-hit")).toHaveCount(0);
});
```

- [ ] **Step 6: Run the browser matrix and inspect screenshots**

```bash
cd frontend
npx playwright test tests/task12.spec.ts tests/task15.spec.ts
```

Expected: PASS. Manually inspect every file under `test-results/task-12/` and `test-results/immersive-table/` for overlap, clipping, unreadable tile faces, false fourth Seats, or texture dominance. If one is present, change only the responsible geometry or CSS rule and rerun the affected case.

- [ ] **Step 7: Run the complete deterministic frontend gate**

```bash
cd frontend
npm run typecheck
npm test -- --run
npm run build
npm run test:browser
```

Expected: all four commands exit 0.

- [ ] **Step 8: Run repository checks**

From the repository root:

```bash
just fmt-check
just test-frontend
just test-spec
```

Expected: all commands exit 0.

- [ ] **Step 9: Commit**

```bash
git add frontend/tests/fixtures/task12-projection.json frontend/tests/task12.spec.ts frontend/tests/task15.spec.ts frontend/tests/task16-real-server.spec.ts frontend/src/styles.css
git commit -m "test(frontend): verify immersive table acceptance matrix"
```

---

## Final Self-Review Checklist

- [ ] Every generated image has a final project path, exact recorded prompt, dimensions, usage statement, and passing checksum.
- [ ] `pixi-table.tsx` no longer owns geometry, table art, or effect implementation details.
- [ ] Live Match and Replay visibly use the same `PixiTable` scene.
- [ ] No top gameplay bar or right gameplay rail remains.
- [ ] No Three.js or new runtime dependency was added.
- [ ] No Backend, protocol, projection, or domain file changed.
- [ ] The visual wall uses only the bounded `remaining_wall` count and does not imply exact dead-wall geometry.
- [ ] Existing action IDs, pending behavior, retry behavior, candidate focus, and no-optimistic-update guarantees remain tested.
- [ ] 3p shows exactly three Player frames and 4p exactly four.
- [ ] 1024 × 600 is playable and 1023 × 599 shows guidance.
- [ ] Reduced Motion, generated-texture fallback, Character fallback, and Replay generic/silent mode pass.
- [ ] `npm run typecheck`, `npm test -- --run`, `npm run build`, `npm run test:browser`, `just fmt-check`, `just test-frontend`, and `just test-spec` pass from a clean worktree.
