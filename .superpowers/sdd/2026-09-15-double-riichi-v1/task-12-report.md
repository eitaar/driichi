# Task 12 report — Pixi table, actions, audio, and reconnect

## RED

The inherited dirty patch rendered a nonblank canvas, but its Pixi sprites used source dimensions. The vendored tile SVGs are 300×400, so applying a table-coordinate scale multiplied every tile into overlapping hands, melds, and discard walls. The inherited character fixtures were uniform-color WebPs, which produced solid red/gray portrait blocks. The supplied RED captures were:

- `frontend/test-results/task-12/3p-red-east-1024x600-decision.png`
- `frontend/test-results/task-12/3p-red-east-1440x900-decision.png`
- `frontend/test-results/task-12/4p-red-east-1024x600-decision.png`
- `frontend/test-results/task-12/4p-red-east-1440x900-decision.png`
- `frontend/test-results/task-12/results-portrait-state.png`

## GREEN changes

- Kept the fixed 1600×900 Pixi scene and responsive letterbox scaling.
- Assigned explicit bounded tile frames: hand 42×56, discard 31×42, meld 38×50, and dora 36×48 table units.
- Bounded icon assets to a 44×44 frame and portrait assets to a 190×220 frame while preserving usable source aspect ratio.
- Added dimension validation and a bounded initials fallback for decoded/unusable or failed portrait media.
- Repositioned melds toward the player edge and centered each meld group so hands, discards, melds, and center data remain separated.
- Replaced uniform fixture WebPs with deterministic legible face/icon artwork for `player-red` and `tsumogiri-bot`.
- Retained render-ready/tile-count instrumentation and added browser regression checks for a 16:9 canvas and bounded ≤3200×1800 backing output.

## Evidence and artifact counts

- Frontend source modules: 10 under `frontend/src/game/`.
- Task 12 browser tests: 8 (3p/4p at 1024×600 and 1440×900, candidate dialog, Riichi highlight, result portrait/standings, unsupported viewport).
- Task 12 Vitest coverage: 11 focused tests; full frontend run: 32 tests passed across 9 suites.
- Rust projection contract: 1 fixture check in the 6-test `task4_projection` suite; the fixture is serialized from the real `AudienceProjection` shape and the room envelope is explicitly synthetic.
- Character fixtures: 4 deterministic WebPs (portrait/icon for two characters).
- Visual artifacts inspected: 5 final decision/result captures at the requested desktop viewports.

## Invariants

- Scene ratio remains `1600 / 900` (`1.7778`); responsive canvas uses current stage scaling.
- Browser checks require `data-render-ready="true"`, a positive rendered tile count, positive table/visual primitive counts, exactly 14 hand hit targets, and exactly one legal discard target in the fixture projection.
- Canvas backing dimensions remain bounded by the renderer's 2× device-pixel cap (`≤3200×1800` in the regression assertion).
- Seat rotation continues to render three seats for 3p and four seats for 4p without a synthetic seat.
- Actions submit only the authoritative decision/action IDs and do not mutate the projected hand optimistically.
- Authoritative projection fields include round/kyoku/dealer/honba/kyotaku and remaining wall; center rendering formats the Rust `Wind` plus numeric kyoku.
- Animation queue cap remains 64; Pixi consumes one item on completion, uses restrained kind-specific visuals, and consumes all pending items immediately for reduced motion.

## GREEN commands

- `cd frontend && npm run typecheck` — passed.
- `cd frontend && npm run test:browser -- tests/task12.spec.ts --workers=1` — 8 passed.
- `cd frontend && npm run test && npm run build` — 32 tests passed; production build passed.
- `cd frontend && npm run test -- --reporter=json --outputFile=../.superpowers/sdd/2026-09-15-double-riichi-v1/task-12-vitest.json` — nonzero JSON artifact.
- `cd frontend && npm run test -- --reporter=junit --outputFile=../.superpowers/sdd/2026-09-15-double-riichi-v1/task-12-vitest-junit.xml` — nonzero JUnit artifact.
- `cd frontend && npx playwright test tests/task12.spec.ts --workers=1 --reporter=json` and `--reporter=junit` — nonzero JSON/JUnit artifacts.
- `git diff --check` — passed.
- `cargo fmt --all -- --check` — passed.
- `cargo check --workspace` — passed.

## Visual observations

The final 3p and 4p captures at both 1024×600 and 1440×900 show bounded, separated tile rows, discard walls, meld groups, dora/center text, seat labels, and persistent legal-action styling without the prior source-dimension overlap. The candidate popup transfers focus, exposes a modal dialog, closes on Escape, and restores focus. The result capture shows a bounded Mangan effect with a legible framed Mika portrait and standings; unrelated room updates do not cancel its expiry timer, and the portrait and standings stay inside the rail with no clipping or overlap.

## Performance and concerns

Asset decoding remains asynchronous and version-guarded; character and Pixi asset loads are bounded by the exact three-second timeout and late completions are ignored. Audio remains failure-tolerant and bounded by the existing ten-second playback guard, including synchronous `play()` failures. Reconnect backoff keeps bounded jitter under the ten-second cap; semantic 4002/4006 closes clear the session and return to Join. The serial targeted browser run is green. No native lock was encountered. No redesign or DOM tile rendering was added.
