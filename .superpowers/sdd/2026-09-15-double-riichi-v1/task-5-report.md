# Task 5 report — MJSON persistence and Replay reconstruction

Status: `DONE_WITH_CONCERNS`

## Implemented

- Added canonical MJAI/MJSON JSON-lines serialization and strict parsing for all current `GameEvent` variants, including calls, daiminkan/ankan/kakan, riichi, multi-Ron, exhaustive/abortive draws, score fields, red tiles, and three-player validation.
- Added buffered `ReplayWriter` persistence under `.incomplete/` with deterministic line indexes, per-Kyoku flush, final file sync, atomic mode-directory rename, portable relative paths, typed persistence failures, failure injection, and cleanup of failed partial artifacts.
- Added ordered auxiliary replay records (`before` / `after`) that are kept out of canonical MJSON and attached to reconstructed event frames.
- Added relative-path root revalidation and startup cleanup for `.part` files and renamed files tied to unfinished Match IDs.
- Added Admin-only `ReplayFrame` reconstruction from validated canonical events and core `ReplayAdminProjection` values. Frames include zero-based event indexes, canonical visible events, complete projected state, and auxiliary events. Three-player output has exactly three players.
- Added `ReplayReader`, frame encoding, corruption errors, and the exact 64 MiB decompressed frame payload limit.
- No Room, SQLite, HTTP, UI, generic storage-provider abstraction, or Public/Player raw-canonical serializer was added.

## TDD evidence

### RED

Before implementation, the required replay integration test was added and run:

```text
cargo test -p double_riichi_replay --test task5_replay
# failed to compile: unresolved imports for ReplayWriter, ReplayFrame, ReplayError,
# CanonicalEvent, MJSON parser/serializer, auxiliary types, and frame-limit helper;
# this was the expected feature-missing failure.
```

### GREEN

After the minimal implementation and refactors:

```text
cargo test -p double_riichi_replay --test task5_replay
# 11 passed; 0 failed
```

Coverage includes JSON-lines/key ordering, all listed calls/Kans/Riichi/multi-Ron/draw/score cases, frame projection state, auxiliary before/after ordering, injected partial-write failure, flush/sync/rename behavior, startup cleanup, corruption/unknown-field/blank-line rejection, path traversal rejection, three-player no-dummy output, generated four-player reconstruction, and exact 64 MiB boundary checks.

## Verification commands and output

```text
cargo fmt --all -- --check
# passed

cargo check --workspace
# Finished `dev` profile

cargo test --workspace
# all workspace tests passed; replay integration: 11 passed, 0 failed

npm run typecheck --prefix frontend
# tsc --noEmit passed

bash tests/workspace-smoke.sh
# passed

grep -Fq 'Basic accessibility is a v1 requirement.' spec/implementation-v1.md && \
grep -Fq 'Reduced-motion fallbacks are a v1 requirement.' spec/implementation-v1.md && \
grep -Fq 'Mobile gameplay layout remains deferred.' spec/implementation-v1.md && \
grep -Fq 'Full Pixi keyboard and screen-reader narration remains deferred.' spec/implementation-v1.md
# passed

git diff --check
# passed
```

`just check` was attempted but the environment has no `just` executable (`/usr/bin/bash: line 1: just: command not found`); its Rust, frontend, spec, and smoke recipes were run directly above.

## Format invariants

- One Match is newline-delimited UTF-8 MJSON; each non-empty line is exactly one canonical event object with the external `type` vocabulary.
- Event line order is append order; object keys and tile spellings are deterministic. Tiles use MJAI names (`1m`, `5mr`, `E`, etc.), with `kan`, `ankan`, and `kakan` forms preserved.
- Auxiliary metadata is never written as MJSON. It is indexed by zero-based event line and sorted `before`, event, `after`, then deterministic sequence.
- New files begin as `replays/.incomplete/<match>.mjson.part` relative to the supplied replay root and complete as `4p/<timestamp>_<mode>_<match>.mjson` or `3p/...`; stored paths are relative and root-revalidated.
- Frame payloads at exactly `64 * 1024 * 1024` bytes are accepted; any larger decompressed payload returns typed `ReplayTooLarge`.
- Canonical events are exposed only through this Admin replay path; core Player/Public serializers remain projection-only.

## Files changed

- `Cargo.lock`
- `crates/double_riichi_replay/Cargo.toml`
- `crates/double_riichi_replay/src/lib.rs`
- `crates/double_riichi_replay/src/error.rs`
- `crates/double_riichi_replay/src/frames.rs`
- `crates/double_riichi_replay/src/mjson.rs`
- `crates/double_riichi_replay/src/path.rs`
- `crates/double_riichi_replay/src/persistence.rs`
- `crates/double_riichi_replay/src/reader.rs`
- `crates/double_riichi_replay/tests/task5_replay.rs`

## Commit

Required commit subject:

```text
feat(replay): persist and reconstruct mjson matches
```

The final commit hash is recorded by `git rev-parse HEAD` after committing this report and implementation.

## Concerns

- The implementation reconstructs participant display names from `start_game` names and uses stable seat identities because canonical MJSON does not contain the later SQLite Participant snapshots. Task 6/Room integration should supply persistent Match metadata separately.
- Auxiliary records are in-memory artifact metadata only; SQLite persistence and Room channel integration are intentionally deferred to later tasks.
- The replay format code is local to `double_riichi_replay`; no authoritative `yamai/ReplayProcessor` source was available.

## Deferred yamai gate (explicit)

The authoritative yamai repository/package, immutable revision, license, and `ReplayProcessor` source remain unavailable as recorded in Task 2. No yamai implementation was fabricated, vendored, substituted, executed, or marked accepted. The required generated-four-player yamai acceptance gate remains deferred and red/unknown until the owner supplies the authoritative source. Local MJSON generation, parsing, and projection reconstruction are green; this report does **not** claim yamai compatibility or Design Freeze.

## Review package

After the required commit:

```text
C:/Users/eitab/.pi/agent/git/github.com/obra/superpowers/skills/subagent-driven-development/scripts/review-package docs/superpowers/plans/2026-09-15-double-riichi-v1.md 426bdaec6e4f18c36ac9fac25a43da4a527ab173 HEAD
```
