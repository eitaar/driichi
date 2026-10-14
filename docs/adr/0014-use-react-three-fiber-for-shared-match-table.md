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
- React DOM continues to own every semantic control, status, dialog, focus behavior, blocking state, and Replay transport. The local concealed hand is DOM-only in both Live and Replay; the scene does not duplicate it.
- The R3F root and its canvas are visual-only and sit behind DOM overlays; the execution-thread supplement below keeps rendering in a dedicated Worker.
- The scene uses a fixed perspective camera and procedural geometry. It does not add camera controls, physics, or a major GLTF asset pipeline.
- Tiles use beveled shared geometry, the existing tile faces through one atlas-aware material, and instancing for repeated visible faces and backs.
- The renderer uses `frameloop="demand"`, bounded DPR, one bounded shadow source, shared resources, and explicit disposal.
- Renderer failure or unavailability preserves DOM gameplay through the semantic fallback; it does not silently start a synchronous Window GL renderer.
- Human-approved table/Dora resolutions #8/#9 set the current visual direction: soft Phong-like shading, white tile bodies/atlas backing, ochre backs, rounded dark blue-gray frame and lighter felt, compact dark names, and restrained high-resolution central facts. The original concept review is not integrated human/GPU acceptance under #11.

The original architecture specification is `docs/superpowers/specs/2026-09-19-r3f-cinematic-match-table-design.md`; its supersession note identifies the later visual/acceptance contracts. Current integration is scoped by `docs/superpowers/plans/2026-10-02-approved-table-integration.md` and `DESIGN.md`. Ordinary-Dora face sheen shares a DOM/3D clock and invalidates only during its active pass or event motion; gaps, hidden tabs, no targets and Reduced Motion do not continuously render. Result tiles have no sheen. These are requirements, not claims that integrated browser/GPU gates have passed.

## Dedicated Worker execution supplement

Approved renderer-isolation design: keep the same public `ThreeTable` props and shared `MatchTableScene`, but use one same-origin external module Worker and fresh `OffscreenCanvas` per table lifetime. This is execution isolation, not a replacement scene, lower-quality profile, dependency addition, or integrated acceptance claim.

- Window retains the socket, store, authoritative results, actions, deadlines, focus, keyboard handling and Replay transport. Renderer-only projection copies omit Decision/actions/clocks; credentials and Room progression never enter the Worker protocol.
- Window paints the existing atlas and 2048px center with its fonts/locale, validates and decodes local images without upload or persistence, and transfers owned `ImageBitmap` pixels. Worker owns the existing R3F root, GL, textures, motion and sheen presentation. No DOM shim or blob/eval/CDN Worker is used; the existing CSP remains unchanged.
- Inputs are bounded to one in-flight revision and the latest pending snapshot. `processed` releases that slot, not readiness; readiness requires a current actual frame. Geometry, assets, viewport and DPR invalidate old frames; motion/clock-only updates do not invalidate unchanged geometry. Lifetime and motion-item checks reject obsolete replies.
- The original 3000ms never-created guard belongs to the constructing Worker. Rejected or superseded pixels are closed by their current owner. Teardown acknowledges actual Fiber cleanup, then releases owned textures/bitmaps; a separate fixed 3000ms guard requests termination of an unresponsive owned Worker without extending any result deadline. Browser thread destruction can follow a forcible-termination grace; this is not a physical 3000ms OS/GPU exit guarantee.
- Numeric DPR, AA, depth/stencil, highp, lighting, shadows, atlas filtering, center anisotropy and SRGB/NoToneMapping remain incumbent quality. Constructor `alpha:false` and opaque clear alpha 1 are preserved; Three r186's native context `alpha:true` matches the incumbent, not permission to patch Three or lower opacity.
- Worker/Window time-origin correspondence is presentation-only, not calibration against server/Node clocks or proof of an exact GPU frame. Shared GPU/compositor/OS scheduling can still stall the host. Controlled draw-stall success does not establish all cold reconnect deadlines, native 17.5ms performance, manual Chrome/Edge acceptance or whole-suite acceptance; those gates remain separate.
- Production bundling must retain external Worker chunks and recursive embedded assets with the existing MIME/CSP. The Worker/shared React scene graph is excluded from Window Fast Refresh; other Window HMR remains enabled.

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
