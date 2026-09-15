# double-riichi v1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: use superpowers:subagent-driven-development task-by-task, test-driven development for behavior, fresh task review after every task, and verification before completion.

**Goal:** Build the complete `driichi` self-hosted riichi mahjong server, `driichi-mcp` bridge, browser client, compatibility endpoints, Replay Viewer, Character system, CI, and release artifacts defined by `spec/implementation-v1.md`.

**Architecture:** A protocol-neutral Match machine and Decision boundary live in `double_riichi_core`; Room and Compat Actors own it. Server, MJAI, MCP, Replay, and Frontend adapters consume domain actions, events, and audience projections without receiving engine-specific state.

**Tech Stack:** Rust, Axum, Tokio, riichienv-core, SQLx SQLite, rmcp, React, Vite, TypeScript, PixiJS, Zustand, TanStack Query, GSAP only on the entry route, npm, Playwright.

**Spec:** `spec/implementation-v1.md`

## Global Constraints

- Treat the specification and `CONTEXT.md` as binding; external protocol contracts override local preferences only where the specification says so.
- Use exact dependency versions and commit lockfiles.
- Apply TDD: record a failing behavior test before production behavior, then the minimal passing implementation.
- Keep engine-specific types inside `double_riichi_core::engine`.
- Never serialize canonical Match state to an untrusted client; all client payloads use Player, Public, or ReplayAdmin projection.
- Existing riichi.dev Bots must change only base URL and authentication key for `/ws/ranked` and `/ws/validate`.
- No generated source, Docker, mobile gameplay, public Replay, raw Replay download, custom cache, or speculative crate.
- Use one writer per worktree and one fresh review gate after each task.
- Visual direction is dark broadcast noir; GSAP is isolated to the entry route; basic semantic controls, contrast, focus, and reduced-motion fallbacks are required.

---

### Task 1: Contract alignment and workspace bootstrap

**Produces:** compiling five-crate Cargo workspace, Vite frontend, exact toolchain/build commands, updated accessibility scope.

- Create the root Cargo workspace, five crate manifests, frontend package, `rust-toolchain.toml`, `justfile`, config examples, and smoke tests.
- Update `spec/implementation-v1.md` so basic accessibility and reduced motion are v1 requirements while mobile gameplay and full Pixi narration remain deferred.
- Add one ADR for broadcast-noir UI and the accessibility baseline.
- Keep production build order npm then Cargo; Cargo must fail clearly when `frontend/dist` is absent.
- Verify `cargo check --workspace`, frontend typecheck, spec checks, and binary names.
- Commit `chore: bootstrap double-riichi workspace`.

### Task 2: External contract gate

**Produces:** `spec/external-contracts.toml`, exact pins, executable probes for riichienv, riichi.dev, yamai, rmcp, fonts, and tile art.

- Prove all four Match presets, enumerate real engine actions/events, produce MJSON accepted by pinned yamai, capture authoritative riichi.dev transcripts, and prove rmcp session/subscription/reconnect behavior.
- Stop and report if a required upstream behavior is unsupported; do not hide incompatibility in adapters.
- Verify `just test-contract` and `just test-yamai`.
- Commit `test: pin and prove external contracts`.

### Task 3: Domain types and engine adapter

**Produces:** protocol-neutral `MatchMachine` completing all four presets.

- Test and implement Seats, modes, tiles/red fives, Participants, complete Game Actions, Game Events, Match results, canonical tile order, and the sole riichienv adapter.
- Add deterministic test-only seeds and Match Abort on adapter divergence.
- Verify core Match tests.
- Commit `feat(core): add protocol-neutral match machine`.

### Task 4: Decisions, timing, and projection

**Produces:** unified Decision/action IDs, timeout behavior, and serialized visibility boundary.

- Test and implement simultaneous responses, compound actions, stale IDs, deterministic defaults, casual 30/10 timing, unlimited/watchdog behavior, Temporary Auto, and paused-time tests.
- Test Player, Public, and ReplayAdmin JSON projections for 3p and 4p concealed-information invariants.
- Verify core Decision/time/projection tests.
- Commit `feat(core): enforce decisions timing and visibility`.

### Task 5: MJSON persistence and Replay reconstruction

**Produces:** buffered crash-safe Replay writing and backend ReplayFrame generation.

- Test JSON-lines ordering, calls, Kans, Riichi, multi-Ron, draws, scores, auxiliary positions, failure injection, startup cleanup, corruption, and 64 MiB frame limit.
- Run generated four-player Replay through pinned yamai.
- Verify Replay and yamai tests.
- Commit `feat(replay): persist and reconstruct mjson matches`.

### Task 6: Configuration, SQLite, Admin auth, and Bot Tokens

**Produces:** strict runtime configuration, migration, sessions, token cache/revocation, and audit retention.

- Test strict TOML and `.env`, data-root rules, SQLite pragmas/schema, Argon2id CLI/session behavior, one-time Bot Tokens, global irreversible revocation, secret redaction, and 90-day audit cleanup.
- Verify focused server config/auth/storage tests.
- Commit `feat(server): add configuration authentication and storage`.

### Task 7: Character registry

**Produces:** strict startup Character validation, safe asset routes, and generated CC0 Starter Packs.

- Test IDs, canonical paths, duplicates, unknown fields, usage, headers, limits, required Packs, ETags, and allowlisted routes.
- Add Human Character listing and release-only Python/ffmpeg Starter generation.
- Verify Character parser/server tests.
- Commit `feat(characters): validate and serve licensed packs`.

### Task 8: Room Actor lifecycle

**Produces:** bounded in-memory Room Actor with selection, Ready, Match, reconnect, Auto, cleanup, and Rematch.

- Test all Room phases, roster invariants, Fill with Bots, mode changes, start commit, disconnect/expiry, explicit leave, revocation, slow consumers, storage backpressure, deletion, and shutdown.
- Verify core and server Room tests.
- Commit `feat(core): add room actor lifecycle`.

### Task 9: HTTP and Human WebSocket

**Produces:** secure Admin/Public APIs and complete Human connection flow.

- Implement/test Problem Details, request IDs, limits, rate limits, trusted proxies, Origin/Referer, security headers, Admin commands, public lookup, HttpOnly Human join, snapshots, updates, action results, Ready, reconnect, heartbeat, and close reasons.
- Verify server HTTP/WebSocket integration tests.
- Commit `feat(server): expose admin and human room protocols`.

### Task 10: Frontend foundation and entry shell

**Produces:** broadcast-noir tokens and polished entry/login route.

- Use native CSS/path routing, self-hosted Geist, one Phosphor icon family, real tile assets, and a read-only Pixi vignette.
- Isolate GSAP to entry hierarchy/depth transitions with cleanup and reduced-motion fallback.
- Test loading/error states, contrast, focus, 1024x600 and 1440x900 layout, console, and entry performance.
- Commit `feat(frontend): add broadcast noir entry experience`.

### Task 11: Admin and Lobby UI

**Produces:** Login, Room, roster, Ready, Character, Bot Token, and lifecycle controls.

- Use flat task-focused layouts, semantic DOM controls, explicit loading/empty/error states, and two-second visible-page detail polling.
- Test one-time Token handling and all Admin/Lobby mutations.
- Commit `feat(frontend): add admin and lobby flows`.

### Task 12: Pixi table, actions, audio, and reconnect

**Produces:** playable 3p/4p desktop table from authoritative projected state.

- Test seat rotation, all visible table data, action ID interactions, no optimistic discard, bounded animation, roster preload, audio priorities/limits, portrait effects, and reconnect reset.
- Verify Frontend, visual, and WebSocket integration tests.
- Commit `feat(frontend): add playable pixi table`.

### Task 13: Strict MJAI compatibility

**Produces:** Room MJAI plus drop-in `/ws/ranked`, `/ws/validate`, and `/status`.

- Implement only from pinned Task 2 evidence; test transcripts, request/action IDs, observations, queue/fill, timeout, disconnect, validation, legacy behavior, and the same production Bot with only URL/key changed.
- Verify MJAI and contract tests.
- Commit `feat(mjai): add riichi dev compatible endpoints`.

### Task 14: MCP server and stdio bridge

**Produces:** authenticated revision-safe Resource/Tool loop and `driichi-mcp`.

- Test session ownership, one Token Participant per Room, resume/replacement/expiry, four Tool schemas, three Resources, revisions, deadlines, notification-before-wait, stale actions, revocation, leave, watchdog, and stdio bridge configuration.
- Complete one protocol-Bot Match by wait/read/action ID.
- Verify MCP tests.
- Commit `feat(mcp): add resource driven agent play`.

### Task 15: Replay Admin UI

**Produces:** paginated View/Delete UI using server ReplayFrames and the live renderer.

- Test playback controls, fixed speeds, Kyoku jump, auxiliary order, missing/Ranked Characters, corruption/oversize states, and safe deletion.
- Verify Replay UI and API integration tests.
- Commit `feat(replay): add admin replay viewer`.

### Task 16: Observability, specs, E2E, and release

**Produces:** release candidate and evidence to change the spec to Design Frozen.

- Complete Health, secret-safe tracing, graceful shutdown, OpenAPI/AsyncAPI validation, 3p/4p Human Playwright completions, basic axe checks, generated Starter archives, notices/checksums, platform builds, and release smoke tests.
- Verify `just test-all`, `just build-release`, every archive's `driichi --version`, and scheduled-only `just test-live` separation.
- Change the specification status only when every external/release gate passes.
- Commit `release: complete double-riichi v1 acceptance gates`.
