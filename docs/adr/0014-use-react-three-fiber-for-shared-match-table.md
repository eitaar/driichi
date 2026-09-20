# Use React Three Fiber for the shared Match table

## Status

Accepted — 2026-09-19

## Context

Live Match and Replay currently share a PixiJS renderer that simulates depth with 2D transforms, textures, and layered shadows. The implementation preserves authoritative gameplay and accessibility boundaries, but its faux-3D table was visually rejected: the camera, depth, material response, tile presence, and composition do not reach the supplied reference's standard.

The Frontend already uses React 19.3. Current React Three Fiber documentation states that `@react-three/fiber@9` pairs with React 19 and that v9 is the React 19 compatibility release. R3F exposes Three.js through React while preserving a DOM overlay architecture. Its current Canvas API supports on-demand rendering, WebGL fallback content, and error-boundary containment. Three.js provides instanced repeated geometry and physically lit standard materials.

The renderer must remain presentation-only. Existing projection, Decision, action identity, reconnect, retry, Replay, and domain behavior cannot move into the scene.

## Decision

Replace the shared PixiJS Match/Replay table with Three.js and `@react-three/fiber` v9.

- One `ThreeTable` React adapter and one shared `MatchTableScene` render Live 4p, Live 3p, and Replay.
- React DOM continues to own every semantic control, status, dialog, focus behavior, blocking state, and Replay transport.
- The R3F Canvas is visual-only and sits behind DOM overlays.
- The scene uses a fixed perspective camera and procedural geometry. It does not add camera controls, physics, or a major GLTF asset pipeline.
- Tiles use beveled shared geometry, the existing tile faces through one atlas-aware material, and instancing for repeated visible faces and backs.
- The Canvas uses `frameloop="demand"`, bounded DPR, one bounded shadow source, shared resources, and explicit disposal.
- Canvas fallback and an error boundary preserve DOM gameplay when WebGL is unavailable or the renderer fails.
- Sol-approved Live 4p, Live 3p, and Replay concepts are implementation authority. Sol also performs the final screenshot comparison.

The detailed camera, dimensions, materials, lighting, motion, performance, fallback, and acceptance contracts live in `docs/superpowers/specs/2026-09-19-r3f-cinematic-match-table-design.md`.

## Consequences

### Positive

- Real perspective, geometry, lighting, and material response can match the desired cinematic table composition.
- Live and Replay remain visually and technically unified.
- Instancing and demand rendering bound the cost of repeated tiles and static scenes.
- DOM ownership preserves existing keyboard, focus, status, and action contracts.
- Procedural geometry avoids an external model pipeline and keeps placement deterministic and testable.

### Negative

- The Frontend adds `three` and `@react-three/fiber` and must manage WebGL resource lifetimes.
- Tile-atlas addressing and instanced transforms are more specialized than Pixi sprites.
- Browser tests require new render instrumentation and explicit WebGL fallback coverage.
- Visual correctness now includes camera, lighting, material, and performance review in addition to DOM tests.

### Migration

- Keep the existing Pixi implementation only until R3F reaches focused parity and visual approval.
- Move shared projection-to-layout calculations into renderer-neutral modules before deleting Pixi-only drawing code.
- Replace `PixiTable` consumers in Live and Replay with `ThreeTable` in bounded commits.
- Update tests and instrumentation to renderer-neutral or R3F names.
- Remove Pixi table drawing modules and the Pixi dependency only after confirming whether the entry-page vignette has also been replaced or isolated.

## Supersession

This ADR supersedes the Pixi ownership and faux-3D table decisions in ADR 0013 for Live Match and Replay. ADR 0013's accessibility baseline and React ownership of semantic controls remain in force.

The prior Pixi design specification is superseded in full by the R3F specification.

## Documentation basis

- React Three Fiber installation: https://github.com/pmndrs/react-three-fiber/blob/master/docs/getting-started/installation.mdx
- React Three Fiber v9 migration: https://github.com/pmndrs/react-three-fiber/blob/master/docs/tutorials/v9-migration-guide.mdx
- R3F performance scaling: https://github.com/pmndrs/react-three-fiber/blob/master/docs/advanced/scaling-performance.mdx
- R3F Canvas API: https://github.com/pmndrs/react-three-fiber/blob/master/docs/API/canvas.mdx
- Three.js `InstancedMesh`: https://github.com/mrdoob/three.js/blob/dev/docs/pages/InstancedMesh.html
