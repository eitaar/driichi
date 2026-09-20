---
version: alpha
name: Double Riichi
description: Dark, table-first interface language for authoritative riichi mahjong play and operations.
colors:
  ink: "#090b0d"
  ink-raised: "#101316"
  ink-panel: "#15191d"
  ink-soft: "#1a1f23"
  line: "#30363b"
  line-strong: "#495158"
  paper: "#f1f0eb"
  muted: "#aab0b4"
  muted-strong: "#c8c9c4"
  accent: "#b93a35"
  accent-bright: "#ef8d84"
  success: "#d7e0d4"
  table-surround: "#0b0e11"
  table-shell: "#171c20"
  table-felt: "#18352f"
  table-wood: "#221b18"
  table-trim: "#9a7042"
  tile-body: "#eee5d2"
  tile-back: "#173a33"
typography:
  sans:
    fontFamily: Geist
  mono:
    fontFamily: Geist Mono
rounded:
  sharp: 3px
  subtle: 10px
---

## Overview

Double Riichi presents authoritative mahjong state with a restrained dark interface. Operational surfaces use flat broadcast-noir structure; Live Match and Replay use one cinematic table-first visual system. The 3D table, tiles, and game state dominate while interface chrome appears only where it enables an action or explains status.

## Colors

Use the ink family for application chrome and semantic DOM surfaces. Vermilion is reserved for primary decisions, errors, and focus emphasis; it is not decorative fill.

Use the table palette only inside Live Match and Replay. Felt stays desaturated, metal stays satin and dark, wood is sparse, and warm trim never becomes a bright ornamental frame. Tile faces keep stronger value contrast than every surrounding material.

## Typography

Use Geist for readable names, actions, headings, and explanatory copy. Use Geist Mono for scores, counters, Seat labels, connection state, timers, revisions, and other compact machine-readable facts.

Player names and legal actions remain readable at the minimum gameplay viewport. Decorative labels never compete with tile faces or the local hand.

## Layout

Operational routes may use rails, lists, and flat sections. Live Match and Replay instead use a fixed cinematic table composition with semantic DOM overlays in outer safe areas.

The local hand is the largest tile group. Opponent hands, rivers, and walls recede toward the center. Portrait frames remain compact and outside the play field. Four-player play uses bottom, right, top, and left; three-player play uses bottom, right, and left without a top placeholder.

Actions form one row directly above the local hand. Replay transport stays along the lower outer rail. Settings, connection state, notices, dialogs, and controls stay clear of tiles and preserve the complete table silhouette.

## Elevation & Depth

Operational DOM surfaces use borders and restrained shadows rather than stacked cards. The Match table uses real geometry, fixed perspective, satin materials, and static studio lighting. Depth clarifies the rail, felt, center device, walls, and tile bodies; it does not create ornamental spectacle.

Meaningful game events may briefly change tile position, emphasis, or camera target. Idle camera drift, animated lighting, continuous particles, blur-heavy post-processing, and routine shake are outside the visual language.

## Shapes

DOM controls and panels use sharp or subtle rounding only. The automatic table may use softened manufactured edges, but tiles retain crisp readable faces with small physical bevels.

## Components

Legal actions, tile hit targets, candidate dialogs, Settings, status, blocking messages, Results, and Replay transport are semantic DOM. The WebGL scene is visual-only and must never become the sole owner of an action or status.

Player presentation contains portrait, display name, score, Seat position or Wind, and Riichi state. The center device contains only Kyoku, Honba, Kyotaku, bounded wall count, and Dora indicators.

WebGL fallback preserves current Match facts and operable DOM controls. Reduced Motion presents authoritative state immediately and retains static cues.

## Do's and Don'ts

- Do make the table and tiles the first read.
- Do keep the local hand visibly larger than every opponent hand.
- Do keep portraits small and outside tile geometry.
- Do use one shared visual scene for Live 4p, Live 3p, and Replay.
- Do preserve fixed camera composition and minimal information density.
- Don't copy proprietary characters, logos, readable text, or ornamental marks from references.
- Don't add free camera controls, physics, runtime CDN assets, or a high-detail external model pipeline.
- Don't duplicate scores, timers, actions, or Match facts across multiple surfaces.
- Don't use bright gold, noisy texture, bloom, or constant motion to manufacture visual importance.
