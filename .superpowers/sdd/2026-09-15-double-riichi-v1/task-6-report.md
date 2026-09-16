# Task 6 report — configuration, authentication, and storage

## Crash-recovery context

The prior Task 6 runner disappeared after the workflow ran for roughly 335 minutes and left a corrupted status record. Recovery found HEAD `21d6552` and exactly one untracked focused RED test at `crates/double_riichi_server/tests/task6_config_auth_storage.rs`; no Task 6 production files or commit existed. This same-role fallback preserved that test, fixed only its trivial `Cow<str>` Pattern compile expression, and implemented the task from the RED state.

## Implemented

- Strict `RuntimeConfig` TOML parsing with denied unknown fields, duplicate-key errors from TOML decoding, integer duration bounds, strict HTTP(S) origin validation, and config-parent data-root paths.
- Rooted, required, duplicate/unknown-key rejecting `.env` loader for `ADMIN_USERNAME` and `ADMIN_PASSWORD_HASH`; ambient process variables are never consulted.
- Argon2id password hashing with `m=65536,t=3,p=1`, 16-byte random salts, 12–1024 UTF-8 byte validation, generic verification failures, and `driichi hash-password` stdin/TTY CLI input with no password argv support.
- Hashed 32-byte Admin sessions with fixed 12-hour expiry, independent revocation, restart-invalidatable in-memory state, redacted Debug output, and zeroized returned credentials.
- SQLx bundled SQLite storage with embedded `migrations/0001_init.sql`, exact four-connection/WAL/foreign-key/NORMAL/5-second busy-timeout settings, config-root-relative replay directories, path containment checks, startup unfinished-match/file cleanup, and audit retention cleanup.
- Match, immutable player, auxiliary replay, bot-token, and allowlisted audit schema constraints/indexes.
- CSPRNG one-time `driichi_` Bot Token presentation, SHA-256-only persistence/cache, post-commit authority updates, generic unknown/revoked failures, broadcast `TokenRevoked` active-seat signal, reload preservation, and irreversible transactional revocation/audit.

## TDD evidence

### RED

Command:

```text
cargo test -p double_riichi_server --test task6_config_auth_storage
```

Before implementation it failed at compile time with the expected missing-feature errors: unresolved server APIs (`RuntimeConfig`, `Storage`, session/token/auth types), missing `serde_json`, `sqlx`, and `tokio` test dependencies, missing `TokenRevoked`/`hash_token`, plus the focused test's trivial `&Cow<str>` Pattern expression. No production Task 6 implementation existed at that point.

### GREEN

Command:

```text
cargo test -p double_riichi_server --test task6_config_auth_storage
```

Result after implementation: `10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out`.

The focused tests cover strict TOML/data-root and `.env`, Argon2 policy/generic errors, independent hashed sessions, SQLite pragmas/migrations/path containment, startup cleanup, one-time hashed Bot Tokens, post-commit global revocation/reload signal, Debug redaction, allowlisted audit summaries, and 90-day cleanup.

## Files

- `crates/double_riichi_server/src/config.rs`
- `crates/double_riichi_server/src/auth.rs`
- `crates/double_riichi_server/src/storage.rs`
- `crates/double_riichi_server/src/lib.rs`
- `crates/double_riichi_server/src/main.rs`
- `crates/double_riichi_server/migrations/0001_init.sql`
- `crates/double_riichi_server/tests/task6_config_auth_storage.rs`
- `crates/double_riichi_server/Cargo.toml`
- `crates/double_riichi_server/build.rs`
- `Cargo.lock`
- `config.toml.example`

## Migration and pragma evidence

`Storage::connect` creates `double-riichi.db`, `replays/4p`, `replays/3p`, and `replays/.incomplete`, configures a four-connection SQLx pool with `journal_mode=WAL`, `foreign_keys=ON`, `synchronous=NORMAL`, and `busy_timeout=5000`, then applies the embedded migration transactionally. The migration defines `bot_tokens`, `matches`, `match_players`, `replay_auxiliary_events`, and `audit_logs`, with foreign-key cascades, token state/hash/expiry checks, replay completion checks, nonnegative indexes/sequences, JSON validity checks, unique token hashes, immutable-player uniqueness, and retention/path lookup indexes.

## Security invariants

- Raw Admin passwords, session credentials, Bot Tokens, Authorization values, and cookie values are not persisted or included in custom Debug output.
- Bot authentication hashes the presented token and consults only the in-memory active-state cache; SQLite retains only SHA-256 hashes.
- Cache mutations occur after successful SQLite commits; revocation is transactionally audited and cannot be reactivated.
- Admin login returns one generic invalid-credentials result for username/password failure; unknown and revoked Bot Tokens share that result.
- Replay paths reject absolute/prefix/parent traversal and symlink escapes; startup cleanup matches exact incomplete/renamed filename forms and leaves completed replays.
- Audit summaries are action/key allowlisted and reject raw-token-shaped values/targets.

## Verification commands and output

- `cargo fmt --all -- --check` — passed.
- `cargo check --workspace` — passed.
- `cargo test -p double_riichi_server --test task6_config_auth_storage` — passed, 10/10.
- `cargo test --workspace` — passed, all workspace tests including 10 Task 6 tests and 15 Task 5 tests.
- `npm run typecheck --prefix frontend` — passed.
- `printf 'correct horse battery staple\\ncorrect horse battery staple\\n' | cargo run -q -p double_riichi_server -- hash-password` — passed; stdout was a PHC string and prompts stayed on stderr.
- Mismatch CLI probe — passed; exit status 1 and zero stdout bytes.
- `git diff --check` — passed.

## Commit and concerns

Required commit subject: `feat(server): add configuration authentication and storage`.

The HTTP, Room, protocol, Character, and frontend implementations remain intentionally out of scope. The storage API provides startup and explicit audit cleanup; the eventual server runtime still owns scheduling the specified 24-hour cleanup tick. No external yamai or protocol gate is claimed by this task.
