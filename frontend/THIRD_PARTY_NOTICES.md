# Third-party notices

## shadcn/ui

Base/Nova components were generated with `shadcn@4.21.5` from the explicit `https://ui.shadcn.com/r/styles/base-nova/{name}.json` registry. The MIT license is retained in `frontend/src/vendor/SHADCN-LICENSE.txt` (upstream tag `shadcn@4.21.5`).

`frontend/src/vendor/shadcn-tailwind.css` is an unmodified copy of `shadcn@4.21.5/dist/tailwind.css`, SHA-256 `4c371f7a1ff5d219ae2f7ff28bd256b4346fd546fe46fbae22092e57db2f0fae`. It supplies static variants/animation utilities, not application theme tokens. The CLI is intentionally not an app dependency because its tooling dependencies introduce high-severity audit advisories. When upgrading generated components, compare this pinned CSS against the same upstream CLI version.

## Geist

Geist and Geist Mono are Copyright (c) 2023 Vercel, in collaboration with basement.studio, under the SIL Open Font License 1.1. The self-hosted variable font files are in `frontend/src/assets/fonts/`; the complete license is `frontend/src/assets/fonts/GEIST-LICENSE.txt`. Source package: `geist@1.7.2`, npm tarball integrity `sha512-Gu5lDFa3pLRyoBlBPf0QIFHVdWAnpco7fS1bJm41jyLPFoguBgiubseUN2oLXMgqZ7uxAxDoXcHMhCY/fOTTgg==`.

## Riichi mahjong tiles

The vendored tile SVGs come from [FluffyStuff/riichi-mahjong-tiles](https://github.com/FluffyStuff/riichi-mahjong-tiles) at revision `26e127ba2117f45cdce5ea0225748cc0cfad3169`. The repository states that the assets are in the public domain under CC0 1.0. The upstream license and README are retained in `frontend/src/assets/tiles/`. File SHA-256 checksums are in `frontend/src/assets/SHA256SUMS`.

The tile revision is provisional pending the final Task 2 asset contract. This project does not claim Design Freeze.
