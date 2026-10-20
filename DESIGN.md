---
version: alpha
name: Double Riichi
description: Implemented Home, Lobby and Benchmark Run visual system; not release approval.
typography:
  body:
    fontFamily: 'Geist, sans-serif'
---

## Authority and scope

This document describes the built initial three-surface slice: Home, Human Lobby and Benchmark Run. Owner-approved local composition references guide hierarchy, not generated wording, anatomy, numerical data or exact viewport measurements. Actual owner acceptance of final browser screenshots remains separate from source review.

Join/Character selection, Admin management, gameplay, outcomes and Replay retain their existing behavior and regression coverage. Their remaining visual designs are **not approved by this slice**. Earlier blanket approval claims in this document are superseded. No backend capability or release acceptance is implied.

## Shared system

- Use existing shadcn/ui primitives backed by Base UI, their variants and semantic Tailwind tokens. Compose layout with Tailwind; do not add route skins or another stylesheet.
- Geist sans-serif applies to ordinary headings, names, labels and controls. Earlier Segoe/Georgia/limited-serif guidance is superseded.
- Light, Dark and system preference use the existing ThemeProvider and semantic background, foreground, muted, border, input, ring and destructive tokens in `frontend/src/styles.css`.
- Ordinary controls, focus, selection and navigation are near-monochrome. Earlier teal/slate/amber or blue-focus ordinary-control guidance is superseded. Semantic errors retain destructive meaning.
- Muted Participant identity hues are restricted to charts, supplemented by labels and line patterns. Rank distributions remain neutral and patterned. Chart values come from validated persisted Match evidence, never reference images.
- Original table materials, tiles, registered Character art and voices retain their colors and licenses. Theme changes do not recolor them.
- Preserve visible focus, accessible names, keyboard operation, loading/disabled/error states and readable enlarged text. Vertical scrolling is allowed; clipping actions or horizontal page overflow is not.

## Copy

Default **zero flavor, decorative, marketing or redundant filler text**, project-wide. At most three truly indispensable exceptions may be retained across the entire project, with exact location and necessity recorded; this is a ceiling, not an allowance to fill. The current Task5 inventory retains zero exceptions.

Keep necessary functional labels, actionable guidance, errors, accessible names and security/privacy/uncertainty explanations. UI copy is English; Participant and Room display names remain unchanged. Keep original licenses and provenance. Mock wording is not copy authority.

## Home

Use the shared public header and a Join-first form: heading, labeled six-digit Room code field, then Open room. At comfortable desktop widths the form is left and original registered portrait art is right. On phones the form precedes art. Preserve the two-step Room Join flow and all existing validation, navigation and voice behavior. Do not add a tagline or reassurance footer.

## Lobby

Header identifies Lobby, Room, code, connection and selected/ready counts. Participant cards preserve stable Participant identity and names; clicking opens semantic details. Do not invent pre-start Seats or status badges.

Only the portrait frame is slanted. Original art remains upright at natural aspect; a common head-safe crop keeps hats/bows/faces complete while cropping lower body. Do not replace art or add Character-specific crop metadata. Three- and four-player desktop rosters retain their topology; normal phones use at most two columns, and enlarged text can stack cards. Ready remains reachable at the supported 1024×600 boundary. Existing blockers, preloading and server authority remain binding.

## Benchmark Run

The Run header groups identity, authoritative status/progress, Refresh and confirmed Stop. Running desktop observation defaults open. At comfortable widths the actual table and persisted Run leaderboard use a 2:1 grid; narrower supported desktop widths stack. Leaderboard standings are not current Match scores.

Phones are chart-first and do not mount a private observer or renderer. Charts retain independent windows, Participant selection, numerical equivalents, sample/zero/reference explanations and validated persisted data. Match details use the existing Sheet/Drawer behavior; unknown Replay metadata remains unknown with retry.

Fullscreen is an explicit observer action, not a default page mode. The existing Focus Select portals within the fullscreen observer; Escape closes its popup before exiting fullscreen and returning focus. Closing observation, crossing the desktop boundary, authorization loss and Run/Room replacement dispose private state/resources.

## Table and resource boundaries

Only 3D table functionality requires a viewport of at least **1024×600**. Authorized non-3D controls, results and metadata remain available on phones. Unsupported viewports do not open the private observer, issue private GETs, start Canvas/Worker or load renderer resources. Existing Live/Replay table behavior is retained, not redesigned or newly visually approved here.

The existing Three.js/React Three Fiber renderer owns table graphics; React DOM owns semantic controls, legal actions, accessibility and dialogs. No controls or core text are rasterized to match a reference. Original table/tiles and registered art remain intact.

## Assets and verification boundaries

Local owner captures read original runtime Character Packs through isolated fixture HTTP routing. They are **not live-backend evidence**. Original Usagi/FANBOX/Touhou/AquesTalk notices remain unchanged; local use is not permission to redistribute assets in Git. Generated compositions stay local and never become shipping art. Capture scripts, source hashes, screenshot provenance and exact validation logs belong to the Task5 evidence package.

Source checks and Chromium/SwiftShader evidence do not close real hardware calibration, actual owner screenshot acceptance, absent backend SSE/Resume/server-pagination/revision-guarded CAS or durable Stop contracts, other-page-family visual approval, or owner release/publication decisions. No push, merge or deployment follows automatically.
