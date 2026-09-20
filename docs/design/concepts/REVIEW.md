# Sol Concept Visual Gate

**Date:** 2026-09-19
**Reference:** `C:\Users\eitab\AppData\Local\Temp\pi-clipboard-44aa8d6d-2a14-4b20-9cd5-b6889077f44c.png`
**Gate owner:** Sol

Scores use a 1–5 scale. Every category must score at least 4. Composition, tile readability, and control placement must score 5.

## Rejected v1 set

| Surface | Composition | Spacing | Tile readability | Local-hand hierarchy | Portrait scale | Materials | Lighting | Controls | Reference fidelity | Verdict |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|
| Live 4p v1 | 4 | 4 | 5 | 5 | 2 | 5 | 4 | 5 | 4 | Reject |
| Live 3p v1 | 5 | 5 | 5 | 5 | 2 | 5 | 4 | 5 | 4 | Reject |
| Replay v1 | 4 | 4 | 5 | 5 | 3 | 5 | 4 | 4 | 4 | Reject |

### Rejection findings

- Portrait frames read as major cards rather than peripheral Seat identity and are several times larger than the reference.
- Generated interface copy is not consistently English.
- Replay gives too much vertical weight to its timeline and compresses the table.
- The material, camera, tile-depth, and local-hand directions are strong and should be preserved.

## Approved v2 set

| Surface | Composition | Spacing | Tile readability | Local-hand hierarchy | Portrait scale | Materials | Lighting | Controls | Reference fidelity | Verdict |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---|
| Live 4p v2 | 5 | 5 | 5 | 5 | 5 | 5 | 5 | 5 | 5 | Approve |
| Live 3p v2 | 5 | 5 | 5 | 5 | 5 | 5 | 5 | 5 | 5 | Approve |
| Replay v2 | 5 | 5 | 5 | 5 | 5 | 5 | 4 | 5 | 4 | Approve |

### Approval findings

- The fixed oblique camera, table occupancy, calm felt field, and bottom-hand emphasis now carry the reference's composition without copying its art.
- Portraits are peripheral thumbnails on the outer rail and do not compete with or overlap tiles.
- The local hand is visibly larger than opponent groups on all three screens.
- Live controls form one compact row above the local hand. Replay transport stays separate from both hand and timeline.
- Three-player composition uses bottom, left, and right only; the top is intentional negative space.
- Graphite, dark walnut, muted bronze, green felt, warm ivory, and restrained studio lighting form one coherent shared table system.
- English DOM labels are explicit. Japanese markings are confined to authentic tile faces.

## Implementation authority

Only these images are approved implementation authority:

- `live-4p-v2.png`
- `live-3p-v2.png`
- `replay-v2.png`

The v1 images remain as rejected audit evidence and must not guide implementation where they conflict with v2.

Implementation must preserve the numerical camera, geometry, material, lighting, motion, performance, accessibility, and failure-state contracts in `docs/superpowers/specs/2026-09-19-r3f-cinematic-match-table-design.md`. When an image and a semantic/product constraint conflict, the semantic/product constraint wins and Sol reviews the resulting screenshot.
