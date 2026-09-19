# Immersive Match Table Redesign

**Status:** Approved design
**Date:** 2026-09-19
**Scope:** Live Match and Replay table presentation

## Intent

Redesign the Match play surface around an immersive, table-first composition inspired by the supplied four-player mahjong screenshot: the table fills the viewport, Player identity sits at the four edges, the hand stays close to its actions, and Match information lives on the table instead of in a dashboard rail.

The redesign must feel like a full game rather than an operator dashboard. It must remain recognizably Double Riichi, preserve authoritative gameplay behavior, and avoid copying proprietary characters, logos, text, or ornamental details from the reference.

Success means:

1. The Match reads immediately as one physical mahjong table rather than a canvas beside a scoreboard.
2. Character portraits, hands, rivers, melds, the wall count, and Kyoku information form one coherent visual scene.
3. Existing rule, projection, action, Replay, accessibility, and minimum-viewport contracts continue to hold.

## Existing constraints

- React owns controls and status UI.
- PixiJS owns the table, tiles, hands, discards, melds, Character effects, and game animation.
- Zustand owns the current authoritative projection and animation queue.
- Live Match and Replay share `PixiTable`.
- Gameplay supports landscape viewports at least 1024 × 600. Smaller viewports show guidance; this redesign does not add mobile gameplay.
- The table uses a fixed 16:9 logical scene and letterboxes rather than distorting tiles.
- Human actions submit existing `decision_id` and `action_id` values. The Frontend never reconstructs canonical state or removes tiles optimistically.
- Three-player Matches contain three Seats only, positioned at bottom, left, and right.
- UI copy remains English.

## Visual direction

The Match surface uses a full-game visual direction rather than the existing broadcast-noir treatment.

- **Palette:** near-black navy surround, deep indigo table field, aged gold rails and accents, warm ivory tiles, and restrained vermilion for Decisions and errors.
- **Depth:** faux 3D in PixiJS. The table field and rails use perspective cues, layered shadows, edge highlights, and seat-aware rotation. Three.js is not introduced.
- **Typography:** retain Geist for Player names and actions and Geist Mono for scores, counters, and machine-like status.
- **Hierarchy:** the table is the dominant surface. Decorative gold remains on edges and state accents so it does not compete with tiles or legal actions.
- **Motion:** Draw and Discard use short movement and light cues; calls and Riichi receive medium emphasis; a Mangan-or-higher result receives the strongest centered portrait treatment. There is no constant particle field or routine screen shake.

## Generated table assets

Use Pi Image Generation to create four original, text-free raster assets as separate generation calls:

1. A seamless deep-indigo table-felt texture.
2. A dark lacquer and aged-gold rail texture.
3. A text-free central table-device surface.
4. A gold-edged tile-back material sheet.

The supplied screenshot is a composition, depth, and mood reference only. Generated assets must contain no characters, logos, readable text, UI labels, or copied ornamental marks.

Every selected asset must be inspected for seams, legibility under tiles, compression artifacts, and suitability at the supported viewport sizes. Record the final prompt, generation method, generation date, pixel dimensions, usage conditions, and SHA-256 digest in `frontend/src/assets/table/PROVENANCE.md`. Project-consumed files must live under `frontend/src/assets/table/`; no runtime CDN is allowed.

Existing Character Pack portraits and existing tile-face SVGs remain authoritative. Generated art does not replace them.

## Rendering architecture

Keep one shared Pixi scene for Live Match and Replay, but split the current monolithic renderer into focused drawing units.

- `PixiTable` remains the React lifecycle adapter. It mounts Pixi, handles resize, schedules renders, and destroys resources.
- A geometry unit owns the 1600 × 900 logical coordinate system, faux-perspective bounds, Seat anchors, wall distribution, and tile placement.
- A table-skin unit owns the field, rails, texture loading, central device, and procedural fallback surfaces.
- Player and tile drawing units own portraits, labels, scores, hands, rivers, melds, Dora, and wall backs.
- An effects unit owns Draw, Discard, call, Riichi, score, and win visuals.

React DOM remains responsible for legal actions, candidate selection, Settings, connection state, Toasts, blocking status messages, and Replay transport controls. This keeps interactive semantics and keyboard behavior out of the Pixi canvas while preserving the established rendering boundary.

## Live Match composition

Remove the fixed top bar and right scoreboard/action rail from the gameplay surface.

- The table occupies the available viewport inside the existing 16:9 letterbox rule.
- Settings and compact connection state float in the upper-right safe area.
- Recoverable notices and action errors appear as temporary Toasts along the upper edge.
- Room deletion, Guest Session expiry, and other non-recoverable states replace the playable scene with a blocking explanation.
- Legal Game Actions appear in one horizontal row directly above the local Player's hand.
- The Decision timer appears only in that local Action row. No portrait timer ring or central timer is added.
- Candidate selection opens adjacent to the Action row and retains dialog semantics, Escape dismissal, and focus return.

The local Player's hand remains front-facing and larger than all other tiles. Other hands, rivers, and melds rotate with their Seat. Legal discard and Riichi-discard candidates retain visible hover and keyboard-focus treatment.

## Player presentation

Each occupied Seat receives one edge-mounted Player frame containing:

- Character Pack portrait cropped into the frame
- Display Name
- score
- Wind or physical Seat position
- Riichi state when active

The local Player frame receives a stronger gold outline. Character names are not displayed. Missing Character assets retain the existing kind/initial fallback and produce no voice or portrait effect.

Four-player Matches use bottom, right, top, and left. Three-player Matches use bottom, right, and left; no visual or data placeholder creates a fourth Player at the top.

## Central table device

The center device groups the Match facts that apply to the whole table:

- Kyoku
- Honba
- Kyotaku
- remaining wall count
- Dora indicators

Player scores remain with the Player frames and are not repeated in the center.

The visual wall distributes exactly `remaining_wall` tile backs across the table edges. It is a count visualization only. It does not claim to expose tile order, the dead-wall break, or exact live-wall geometry. This limitation must be reflected in naming and tests; no Backend or projection extension is introduced.

## Action behavior

The redesign changes placement and styling, not action semantics.

- Discard and Riichi Discard: select a legal tile from the local hand.
- Ron, Tsumo, Pass, and Abortive Draw: submit from the horizontal Action row.
- Chi, Pon, Kan, and Kita: submit immediately when one candidate exists; open the existing candidate chooser when multiple candidates exist.
- A pending submission disables all action input.
- A rejected action can be retried while the same Decision remains open.
- No action is queued while disconnected.
- No optimistic tile removal or score update is introduced.

## Replay composition

Replay uses the same Pixi scene, table skin, geometry, Player frames, central device, wall-count visualization, and fallback behavior as Live Match.

Replay transport controls float along the table's lower edge and include existing play/pause, step, speed, and Kyoku navigation. The event log remains outside the table as secondary analysis content. Replay does not render the Live Match Action row, connection state, or Settings control.

## State and failure behavior

- **No projection:** show a quiet synchronization state inside the table field.
- **No Decision:** omit the Action row rather than reserving an empty rail.
- **Reconnect:** retain the last authoritative scene, show compact connection state, and clear queued animation on the next full snapshot as today.
- **Recoverable action error:** show a Toast without covering the hand.
- **Non-recoverable connection reason:** replace the playable scene with a blocking explanation.
- **Generated texture failure:** fall back to procedural indigo fills and gold strokes; gameplay remains usable.
- **Character failure:** use the existing neutral kind/initial fallback and silence.
- **Reduced Motion:** remove positional, scale, and continuous motion; update authoritative state immediately and retain static state cues.

## Accessibility

- Keep all actions, candidate choices, Settings, Toasts, blocking messages, and Replay controls in semantic DOM.
- Preserve visible `:focus-visible` treatment over the busier table art.
- Ensure action labels and scores meet usable contrast against their local surfaces.
- Preserve candidate focus entry, Escape dismissal, and focus return.
- Announce action errors and blocking connection states through existing alert/status semantics.
- Decorative Pixi content remains hidden from the accessibility tree; the canvas retains its concise table label.
- Full Pixi keyboard navigation and screen-reader narration remain outside v1 scope.

## Verification

### Unit coverage

- Faux-perspective geometry remains inside the 1600 × 900 logical scene.
- Seat rotation places the viewer at bottom for Live Match and Seat 0 at bottom for spectator and Replay views.
- Three-player mode returns bottom, right, and left without a fourth Seat.
- Wall rendering produces exactly the non-negative integral `remaining_wall` count and distributes it deterministically.
- Central-device labels handle absent optional fields without fabricating state.
- Existing authoritative action-ID, double-submit, retry, animation-cap, and asset-timeout tests remain green.

### Browser coverage

Exercise 1024 × 600, 1280 × 720, 1600 × 900, and 1920 × 1080 for:

- four-player Live Match
- three-player Live Match
- local discard Decision
- multi-candidate call Decision
- no Decision
- reconnect and rejected-action Toasts
- Replay with floating transport controls
- Reduced Motion
- generated-texture fallback
- Character fallback

Verify keyboard focus, candidate dialog behavior, Settings, Replay controls, and automated accessibility checks. Retain Pixi `render-ready`, rendered tile count, and rendered visual primitive instrumentation for both Live Match and Replay.

### Visual acceptance

At every supported viewport:

- the table is the dominant composition;
- the local hand and legal actions are readable without inspecting a side rail;
- Player identity and score are legible at their table edge;
- the center device does not overlap rivers or Dora;
- generated texture does not reduce tile-face contrast;
- the Action row, Toasts, and Replay controls remain inside safe visible bounds;
- three-player mode leaves no fake top Player;
- Reduced Motion conveys the same game state without transitional movement.

## Non-goals

- Three.js, real 3D meshes, camera controls, or ray-cast tile picking
- mobile gameplay
- Backend, domain, projection, or protocol changes
- exact live-wall/dead-wall geometry
- new Character art or replacement tile faces
- localization infrastructure or Japanese UI copy
- new gameplay actions, rule options, or optimistic state
- constant particles, routine screen shake, or BGM
