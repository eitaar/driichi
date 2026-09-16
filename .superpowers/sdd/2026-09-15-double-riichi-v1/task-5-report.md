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

## Review round 1/5 follow-up

The authoritative yamai gate remains deferred under the owner ruling; no source was fabricated or substituted. The local Critical/Important findings were addressed as follows:

- Kita now removes one North tile from the sanma concealed hand and therefore updates `hand` and `concealed_count`; a focused three-player Kita frame test covers it.
- Auxiliary records are rejected when outside the event timeline, when `after` is requested before any event, or when a pending `before` record is finalized without a following event.
- Startup cleanup matches the exact `_<match_id>.mjson` filename component instead of using substring matching; unsafe underscore-containing IDs are rejected.
- Finalize validates auxiliary positions and cleans the `.part` path on every post-flush failure, including destination-directory failure.
- Incomplete files now use exactly `.incomplete/<match_id>.mjson.part`; timestamp/mode remain only in the completed destination filename.
- MJSON parsing now requires a `start_kyoku` mode and terminal `end_game` before accepting a ReplayReader input.
- Noncanonical `kyotaku`, `ura_markers`, and `deltas` aliases are rejected; only canonical field names are accepted.
- The 64 MiB boundary now exercises `encode_replay_frames` at exactly the limit and at limit-plus-one; Player/Public projection assertions now serialize real core projections.

### Follow-up RED

```text
cargo test -p double_riichi_replay --test task5_replay
# 14 tests ran with 7 expected failures before the fixes:
# Kita retained 13 concealed tiles; invalid auxiliary positions were dropped;
# cleanup removed ABCDEF for unfinished ABC; finalize left .part; the part path
# still contained timestamp/mode; end_game-only input parsed successfully; and
# the new Player/Public assertion plus encoder boundary assertion failed.
```

### Follow-up GREEN

```text
cargo test -p double_riichi_replay --test task5_replay
# 14 passed; 0 failed
```

Fix commit: `fix(replay): close task 5 review findings` (final hash recorded by `git rev-parse HEAD`).

### Follow-up verification

```text
cargo fmt --all -- --check
# passed

cargo check --workspace
# passed

cargo test --workspace
# all workspace tests passed; replay integration: 14 passed, 0 failed

npm run typecheck --prefix frontend
# tsc --noEmit passed

bash tests/workspace-smoke.sh
# passed

grep -Fq 'Basic accessibility is a v1 requirement.' spec/implementation-v1.md && grep -Fq 'Reduced-motion fallbacks are a v1 requirement.' spec/implementation-v1.md && grep -Fq 'Mobile gameplay layout remains deferred.' spec/implementation-v1.md && grep -Fq 'Full Pixi keyboard and screen-reader narration remains deferred.' spec/implementation-v1.md
# passed

git diff --check
# passed
```

`just check` remains unavailable in this environment; direct equivalent recipes are green. The yamai command was deliberately not run because the authoritative source/revision remains unavailable.

## Review round 2/5 follow-up

The pinned yamai acceptance gate remains owner-deferred because the authoritative source/revision is unavailable; no substitute was fabricated or executed. The local Important finding was fixed by rejecting `Kita` during canonical event validation unless the inferred replay mode is three-player. Four-player reconstruction therefore cannot mutate state from malformed nuki input.

### Follow-up RED

```text
cargo test -p double_riichi_replay --test task5_replay four_player_kita_is_rejected
# failed: the four-player replay containing a North tile and Kita was accepted
# and the assertion requiring a three-player validation error failed
```

### Follow-up GREEN

```text
cargo test -p double_riichi_replay --test task5_replay
# 15 passed; 0 failed
```

### Follow-up verification

```text
cargo fmt --all -- --check
# passed

cargo check --workspace
# passed

cargo test --workspace
# all workspace tests passed; replay integration: 15 passed, 0 failed

npm run typecheck --prefix frontend
# tsc --noEmit passed

bash tests/workspace-smoke.sh
# passed

# required implementation-v1 spec assertions
# passed

git diff --check
# passed
```

Fix commit: `fix(replay): reject four-player Kita` (final hash recorded by `git rev-parse HEAD`).

## Review round 3/5 follow-up

The review confirms the local four-player Kita finding remains addressed: `validate_event` rejects `Kita` for four-player modes before `ReplayState::apply`, and `four_player_kita_is_rejected` remains green. No additional local Critical/Important implementation defect was identified, so no duplicate test or speculative code was added.

The pinned yamai acceptance gate remains owner-deferred. The brief's authoritative yamai repository/revision and `ReplayProcessor` source are still unavailable; no implementation, substitute, execution, or compatibility claim was fabricated. Local replay tests therefore remain evidence only for local behavior, not external yamai acceptance.

### Follow-up verification

```text
cargo test -p double_riichi_replay --test task5_replay
# 15 passed; 0 failed

cargo fmt --all -- --check
# passed

cargo check --workspace
# passed

cargo test --workspace
# all workspace tests passed; replay integration: 15 passed, 0 failed

npm run typecheck --prefix frontend
# tsc --noEmit passed

bash tests/workspace-smoke.sh
# passed

# required implementation-v1 spec assertions
# passed

git diff --check
# passed
```

This round records verification only; no local source change was required beyond the already-committed Kita fix.

## Review round 4/5 follow-up

The local Important finding remains addressed without regression: four-player `Kita` is rejected by mode-aware validation before replay state mutation, and the existing malformed four-player regression remains green. The review package for this round is report-only; no additional local Critical/Important breakage was found, so no duplicate test or speculative implementation was added.

The Critical pinned yamai gate remains owner-deferred. The authoritative repository/package, immutable revision, license, and `ReplayProcessor` source are still unavailable. Per instruction, no yamai implementation, substitute, execution, or compatibility claim was fabricated. Local MJSON round-trip and projection tests do not establish that external acceptance.

### Follow-up verification

```text
cargo test -p double_riichi_replay --test task5_replay
# 15 passed; 0 failed

cargo fmt --all -- --check
# passed

cargo check --workspace
# passed

cargo test --workspace
# all workspace tests passed; replay integration: 15 passed, 0 failed

npm run typecheck --prefix frontend
# tsc --noEmit passed

bash tests/workspace-smoke.sh
# passed

# required implementation-v1 spec assertions
# passed

git diff --check
# passed
```

This round required no local code/test edits; the report records the repeated gate deferral and verification evidence.
