# Cinematic R3F Match Table Redesign

**Status:** Approved implementation specification
**Date:** 2026-09-19
**Scope:** Live Match 4p, Live Match 3p, and Replay table presentation
**Supersedes:** `2026-09-19-immersive-match-table-design.md`

## Authority and intent

This specification replaces the rejected PixiJS faux-3D table with one shared true-3D scene built with Three.js and `@react-three/fiber` v9. Sol owns art direction, concept selection, visual comparison, and final acceptance. Implementation workers receive only approved concept images, this numeric specification, and bounded file/task scopes; they do not decide whether the result is visually acceptable.

The supplied reference image is authority for composition, spatial hierarchy, camera attitude, and information restraint. It is not a source for proprietary characters, logos, readable text, ornamental marks, or copied assets.

Success means the table reads first, the tiles read second, and interface chrome reads only when needed. Live 4p, Live 3p, and Replay must feel like views of the same premium automatic table.

## Current library basis

The project uses React 19.3. React Three Fiber's current installation and v9 migration documentation state that `@react-three/fiber@9` pairs with React 19 and is a compatibility release for React 19. The implementation therefore installs `three` and `@react-three/fiber` v9 and pins the resolved versions in `package-lock.json`.

Current R3F documentation also establishes the architecture used here:

- `<Canvas frameloop="demand">` stops continuous rendering and renders on React changes or explicit invalidation.
- `<Canvas fallback={...}>` provides a DOM fallback when WebGL is unavailable.
- Canvas should be protected by an error boundary for renderer/context failures.
- `eventSource` can share pointer events with a parent containing both Canvas and DOM overlays, although v1 keeps legal gameplay actions in DOM rather than ray-cast controls.

Current Three.js documentation establishes `InstancedMesh` as the draw-call reduction mechanism for repeated geometry and `MeshStandardMaterial` as the physically lit surface model.

Primary documentation:

- https://github.com/pmndrs/react-three-fiber/blob/master/docs/getting-started/installation.mdx
- https://github.com/pmndrs/react-three-fiber/blob/master/docs/tutorials/v9-migration-guide.mdx
- https://github.com/pmndrs/react-three-fiber/blob/master/docs/advanced/scaling-performance.mdx
- https://github.com/pmndrs/react-three-fiber/blob/master/docs/API/canvas.mdx
- https://github.com/mrdoob/three.js/blob/dev/docs/pages/InstancedMesh.html
- https://github.com/mrdoob/three.js/blob/dev/docs/pages/MeshStandardMaterial.html.md

## Product and domain boundaries

The renderer consumes the existing authoritative projection and does not reconstruct canonical state. The redesign preserves:

- authoritative `decision_id` and `action_id` submission;
- no optimistic tile removal, score mutation, or Decision advancement;
- pending, retry, reconnect, terminal-close, and action-result behavior;
- Live and Replay projection semantics;
- the three-player rule of bottom, left, and right only;
- English interface copy;
- existing Character Pack and tile-face assets;
- Backend, domain, projection, protocol, MJAI, MCP, Lobby, and Admin behavior.

## Shared rendering boundary

React DOM owns every semantic or interactive element: legal actions, tile hit targets, candidate dialogs, Settings, connection state, Toasts, blocking states, Results, Replay controls, focus management, and accessibility announcements.

R3F owns only the visual table scene: table body, rails, felt, center device, walls, tile geometry, rivers, melds, Dora, restrained event motion, and visual portrait frames. The Canvas is decorative to assistive technology and receives a concise table label through its DOM host. Legal gameplay remains operable when the WebGL scene cannot render.

Live and Replay use one `ThreeTable` adapter and one `MatchTableScene`. Surface-specific DOM overlays are siblings over the same scene host.

## Coordinate system and camera

Use world-space metres as stable authoring units:

- Table outer body: `13.6 × 9.2`, top surface at `y = 0`.
- Inner felt field: `11.7 × 7.3`.
- Rail width: `0.72`; rail top at `y = 0.34`.
- Center device footprint: `2.55 × 2.05`, top at `y = 0.23`.
- Tile body: local hand `0.62 × 0.18 × 0.86`; opponent, river, and wall `0.48 × 0.16 × 0.67` unless a meld requires the local size for readability.
- Coordinate origin: center device center. Positive X points right; positive Z points toward the local Player.

The camera is fixed and non-interactive:

- Perspective FOV: `31°`.
- Initial position: `[0, 11.8, 12.6]`.
- Look target: `[0, 0.15, 0.25]`.
- Near/far: `0.1 / 60`.
- No OrbitControls, drag, zoom, pan, or free-camera state.

The approved concept images may refine the camera within these bounded envelopes without changing the composition model: FOV `28–34°`, camera Y `10.8–12.8`, camera Z `11.6–13.8`, target Z `0–0.6`. Sol chooses the final values during concept approval and records them with the selected concepts before implementation.

## Composition and safe areas

The scene renders into a fixed 16:9 stage that letterboxes rather than distorts. At 1024×600, 1280×720, 1600×900, and 1920×1080:

- the projected table body occupies 82–90% of stage width and 78–88% of stage height;
- the local hand remains fully visible in the lower 22% of the stage;
- the center device remains inside the middle 26% of stage width and never intersects rivers;
- Player portrait frames sit outside the inner felt field and never overlap hands, walls, rivers, melds, or Dora;
- Settings and connection state stay in the upper-right outer safe area;
- transient notices stay in the upper-left outer safe area;
- the Action row sits in one horizontal band immediately above the local hand;
- Replay transport sits in one horizontal band inside the lower outer rail, with the event log outside the table stage.

Four-player Seats are bottom, right, top, and left. Three-player Seats are bottom, right, and left with no top frame, hand, wall label, or placeholder.

## Tile hierarchy and geometry

Tiles are beveled 3D bodies with warm ivory sides and existing face textures. Tile faces remain flat, high-contrast, and unlit enough to stay readable under studio lighting.

- Local hand tiles are at least 1.28× the projected area of opponent hand tiles.
- Local hand uses the full tile body and face; legal hit targets remain aligned DOM buttons above the canvas.
- Opponent concealed hands use backs and smaller geometry.
- Rivers use the opponent size with deterministic seat rotation and six-column grouping.
- Melds use local-size geometry when needed to keep called orientation readable.
- Wall backs visualize the bounded `remaining_wall` count only; they do not claim exact wall order or dead-wall geometry.
- Dora indicators remain adjacent to the center device and cannot overlap river bounds.

Use one generated tile-face atlas derived from the existing project SVGs. One atlas-aware front-face material receives a per-instance face index; body sides use a shared ivory material. Hidden backs use a shared back material. Repeated tiles render with `InstancedMesh`; do not create one mesh/material pair per tile.

## Table and material direction

The table is a restrained premium automatic table, not a fantasy prop and not a dashboard panel.

- Outer surround: near-black charcoal `#0b0e11`.
- Table shell: satin graphite `#171c20` with restrained brushed-metal response.
- Felt: desaturated deep green `#18352f`, low-frequency texture only.
- Rail inset: dark walnut-black `#221b18` used sparingly.
- Metal trim: muted warm bronze `#9a7042`; no bright gold frame.
- Tile body: warm ivory `#eee5d2`.
- Tile back: deep bottle green `#173a33` with a simple project-original relief.
- Decision/error accent: existing restrained vermilion family.

Reuse existing generated table assets only when they survive Sol's concept comparison. No runtime CDN, copied ornament, readable generated text, or proprietary character art is allowed.

## Lighting

Use a static dark-studio rig:

- hemisphere or ambient fill, intensity `0.35–0.55`;
- one warm key area/spot light from upper-left, intensity `1.8–2.6`;
- one cool low-intensity fill from upper-right, intensity `0.35–0.65`;
- one narrow rim for the near rail/local hand, intensity `0.5–0.9`;
- one shadow-casting light maximum, 1024px shadow map maximum;
- wall backs and decorative meshes do not cast shadows;
- tile faces never bloom, glare, or lose markings.

There is no idle camera drift, animated lighting, depth-of-field blur, fog that obscures tiles, or post-processing stack in v1.

## Player presentation

Portraits are compact edge frames outside the play field. Each occupied Seat shows only:

- Character Pack portrait or existing neutral fallback;
- display name;
- score;
- Seat position/Wind;
- Riichi state when active.

The bottom/local frame receives a subtle brighter edge, not a larger card that competes with the hand. Portrait frames use DOM text for guaranteed readability; a matching 3D or CSS frame may provide depth behind them. Character names are not displayed.

## Central device and information density

The center device shows only shared Match facts:

- Kyoku;
- Honba;
- Kyotaku;
- bounded remaining wall count;
- Dora indicators.

Scores remain with Player frames. Do not add analytics, logs, turn history, rule explanations, or duplicate timers to the table.

## Actions, dialogs, and Replay controls

Action behavior is unchanged. The DOM Action row is one line above the local hand. Multi-candidate calls open the existing modal chooser adjacent to the Action row and retain focus entry, Tab containment, Escape dismissal, focus restoration, `aria-haspopup="dialog"`, `aria-expanded`, and `aria-controls`.

Replay uses the same scene and camera. Its transport includes existing play/pause, previous/next, speed, and Kyoku navigation. Replay does not render Live Settings, connection state, or legal actions. Transport and event signal remain within the table stage at all four viewports.

## Motion

Motion occurs only when authoritative state changes:

- Draw or Discard translation: `140–220ms`.
- Call, Riichi, or score emphasis: `180–300ms`.
- Win camera accent: one bounded target/FOV pulse, `350–500ms`, then return to the fixed camera.
- No idle motion, looping particles, routine shake, or decorative parallax.

Reduced Motion applies authoritative state immediately, disables positional/scale/camera interpolation, and preserves static highlights and status text.

## Performance contract

The representative target is a stable 60fps interaction path on a current desktop browser at 1600×900, without a major regression at 1920×1080.

Required implementation choices:

- `<Canvas frameloop="demand">` with explicit invalidation only for bounded event animation;
- device DPR clamped to `[1, 2]`, fixed for the renderer during idle and motion;
- default-framebuffer MSAA enabled and high-precision shaders requested;
- instanced visible faces, concealed backs, wall backs, and repeated table hardware;
- one tile-face atlas and shared materials/geometries;
- memoized projection-to-instance transforms;
- one shadow-casting light and a 1024px shadow map maximum;
- no physics, free camera, GLTF pipeline, runtime model download, continuous post-processing, or per-tile React state;
- dispose geometries, materials, textures, and animation handles on scene teardown.

Instrumentation must expose render readiness, rendered tile count, wall count, scene primitive count, fallback state, and active animation state on the scene host so Live and Replay browser tests can verify real WebGL output without reading pixels alone.

## State, failure, and accessibility behavior

- No projection: keep the scene host mounted and show the existing quiet DOM synchronization state.
- Reconnect: preserve the last authoritative scene and disable actions until connected.
- Terminal close: replace playable content with the existing blocking explanation.
- Action rejection: retain the hand and Decision and allow retry while that Decision remains open.
- WebGL unavailable: render a semantic DOM fallback with current Match facts and legal controls still usable.
- WebGL/context error: catch at the Canvas boundary, report fallback state, and keep DOM controls/status intact.
- Tile-atlas or portrait failure: use existing project fallbacks; never block gameplay.
- The Canvas stays out of the accessibility tree; player status, actions, alerts, dialogs, and Replay controls remain semantic DOM.

## Concept-image gate

Before implementation, generate three separate large horizontal images:

1. Live Match 4p.
2. Live Match 3p.
3. Replay.

Each image must show one complete screen, not a contact sheet. Record source reference path, full prompt, generation method/date, pixel dimensions, intended use, and SHA-256 digest.

Sol scores each candidate from 1–5 on composition, spacing, tile readability, local-hand hierarchy, portrait scale, materials, lighting, control placement, and reference faithfulness. Every category must score at least 4; composition, tile readability, and control placement must score 5. Failed categories are regenerated. Only approved images become implementation authority.

## Verification and acceptance

### Automated

- Unit tests for camera/layout constants, Seat mapping, 3p omission, wall bounds, instance counts, atlas addressing, and Reduced Motion state.
- Existing action identity, pending/retry, reconnect, terminal-state, and Replay behavior tests remain green.
- Browser tests at 1024×600, 1280×720, 1600×900, and 1920×1080 for Live 4p, Live 3p, and Replay.
- Keyboard/focus, dialog containment, Reduced Motion, WebGL fallback, connection states, asset fallback, and accessibility checks.
- Render instrumentation proves non-zero real scene primitives and tiles.
- Browser coverage verifies DPR 2, the MSAA context attribute, drawing-buffer dimensions, and motion-stable quality in a focused high-density case. Default CI uses bounded SwiftShader regression limits; `DRIICHI_HARDWARE_WEBGL=1` headed coverage enforces the stated desktop motion budgets on a hardware renderer.

### Visual

Capture all twelve viewport/surface combinations. Sol compares implementation screenshots side by side with the approved concepts and rejects any material mismatch in composition, hierarchy, readability, overlap, clipping, portrait scale, lighting, spacing, or control placement.

## Non-goals

- Backend, domain, projection, protocol, MJAI, or MCP changes
- Lobby or Admin redesign
- mobile gameplay
- free camera controls
- physics
- high-detail external GLTF assets or Blender pipeline
- runtime CDN assets
- exact physical wall/dead-wall simulation
- new Character art or replacement tile faces
- localization infrastructure
- autoplay sound, constant effects, or decorative animation
