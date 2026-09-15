# double-riichi v1 Implementation Specification

**Status:** Conditional Design Freeze  
**Project:** `double-riichi`  
**Server binary:** `driichi`  
**MCP bridge:** `driichi-mcp`

This document is the integrated v1 implementation contract. A formal Design Freeze requires the external evidence gates in §5 to pass.

## 1. Product boundary

`double-riichi` is a local-first, self-hosted riichi mahjong server. It supports:

- Browser Human play
- Four-player and three-player Room Matches
- riichi.dev-compatible MJAI Bots
- MCP-controlled agents
- MJSON replay persistence and viewing
- Replaceable cosmetic Character Packs
- Four-player replay consumption by `yamai`

The server has one Admin account and no ordinary user account system. Although it binds to localhost by default, every network client is treated as untrusted because operators may expose it through a reverse proxy.

Priority order when requirements conflict:

1. Rule correctness and visibility security
2. MJSON and MJAI compatibility
3. Human Match completion
4. MCP usability
5. Character presentation

### 1.1 v1 release scope

All Definition of Done items in §26 are required for v1.0. Implementation proceeds through vertical milestones:

1. External-contract probes
2. Four-player Match machine and replay
3. Room Actor, persistence, and Human four-player play
4. Three-player Human play
5. riichi.dev compatibility
6. MCP play
7. Replay Viewer and Character presentation
8. Release hardening

### 1.2 Non-goals

v1 does not provide:

- Ordinary user accounts
- Ratings or MMR
- A matchmaking ecosystem or public Room browser
- Room passwords or bans
- Advanced Room permissions
- An Admin debug console
- Reveal-all live mode
- Match pause, Kyoku restart, Room cloning, or seed override UI
- Custom scoring rules or a detailed rule editor
- Live2D, animated Character assets, BGM, or gameplay abilities
- Docker, installers, launch scripts, or service units
- macOS release artifacts
- Three-player MJAI
- Hot reload
- Bot Token scopes or rotation
- Public Replay access or raw Replay download
- Complex Replay seeking, branching, or debug cloning
- Mobile gameplay layout
- Formal Pixi keyboard/screen-reader support
- Reduced-motion presentation

Ordinary DOM controls retain native semantics where this requires no special table implementation.

## 2. Ubiquitous language

The canonical glossary is `CONTEXT.md`. In particular:

- A **Room** is a gathering place that can host multiple Matches.
- A **Match** is one east-only or east-south contest.
- A **Kyoku** is one dealt hand inside a Match.
- A **Participant** is a Room identity.
- A **Player** is a Participant selected into a Match Seat.
- A **Controller** supplies actions without defining Player identity.
- A **Decision** is one bounded opportunity to choose a complete legal Game Action.
- A **Replay** records one Match.

`Game` and `Round` are not canonical domain names. Database and custom API names use `match` and `kyoku` where the external protocol does not require other vocabulary.

## 3. Technology and build

### 3.1 Backend

- Rust
- Axum
- Tokio
- `riichienv-core`
- `sqlx` with bundled SQLite
- `tracing`
- `rust-embed`
- `rmcp`
- rustls for HTTPS in `driichi-mcp`

Important direct dependencies use exact versions. Binary releases include `Cargo.lock`-resolved builds and do not require system SQLite or OpenSSL.

### 3.2 Frontend

- React
- Vite
- TypeScript
- PixiJS
- `@pixi/react`
- Zustand
- TanStack Query
- npm

CI and release builds use `npm ci`. `frontend/dist` is not committed.

Production build order is explicit:

```text
just build-release
  -> npm ci
  -> npm run build
  -> cargo build --release
```

Cargo build scripts never invoke npm. A production Cargo build with no `frontend/dist` fails with a clear error.

## 4. Repository and crate boundaries

```text
double-riichi/
├─ crates/
│  ├─ double_riichi_core/
│  ├─ double_riichi_server/
│  ├─ double_riichi_mjai/
│  ├─ double_riichi_mcp/
│  └─ double_riichi_replay/
├─ frontend/
├─ spec/
├─ tests/
├─ character-pack-template/
├─ config.toml.example
└─ .env.example
```

Dependency direction:

```text
double_riichi_core
  └─ riichienv-core is referenced only by core::engine

double_riichi_replay -> core
double_riichi_mjai   -> core
double_riichi_mcp    -> core
double_riichi_server -> core + replay + mjai + mcp
```

- `driichi` is built by `double_riichi_server`.
- `driichi-mcp` is built by `double_riichi_mcp`.
- No shared `types` or configuration crate is added.
- Core does not depend on Axum, sqlx, rmcp, HTTP DTOs, or filesystem types.

### 4.1 Shared Match machine

A protocol-neutral `MatchMachine` owns game state, legal Decisions, Game Actions, and projections.

```text
RoomActor        owns MatchMachine
CompatMatchActor owns MatchMachine
```

Compat Matches are not hidden Rooms. They have no Lobby, join code, or Rematch.

### 4.2 Room Actor

One Room equals one Tokio Actor. Only that Actor mutates Room state. It communicates with metadata and Replay workers through ordered bounded channels.

Acknowledgement is required for:

- Match persistence open
- Kyoku Replay flush
- Match finalization

Normal Replay appends use an ordered bounded channel. Storage backpressure affects only that Room and records are never silently dropped.

Fixed initial channel capacities:

```text
Room commands:       256
Connection outbound:  64
Replay/effects:      256
```

A full HTTP command queue returns `503 room_busy`; a full Human command queue returns `room_busy`. Channel capacities are not configurable in v1.

### 4.3 Registry and snapshots

The Room registry uses a `tokio::sync::RwLock<HashMap<RoomId, RoomHandle>>` for Room handles, join-code indexes, and cooldowns. It never waits for an Actor while holding the registry lock.

There is no Actor-external canonical snapshot cache in v1. HTTP reads ask the Actor for a snapshot, and audience projection occurs at the Actor boundary.

Each connection has a bounded outbound queue. A full queue closes only that connection with `slow_consumer`; reconnect supplies a complete snapshot. One slow client never blocks the Room.

## 5. External evidence gate

Formal Design Freeze and feature implementation require exact, repository-recorded evidence for:

- `riichienv-core` version or revision
- Successful complete four-player and three-player preset probes
- The authoritative riichi.dev Protocol v2 URL and revision
- Accepted riichi.dev transcripts and fixtures
- Exact `yamai` commit and an executable `ReplayProcessor` probe
- Exact `rmcp` version and initialize/subscription/session probe
- Tile asset source, revision, license, and attribution

Pinned sources and local fixtures are release contracts. A scheduled live riichi.dev comparison detects drift but does not silently redefine the build contract.

No Room or Frontend production implementation begins before these compatibility spikes pass:

1. Complete four-player and three-player `riichienv-core` Matches
2. Generate MJSON and consume it fully with pinned `yamai`
3. Replay pinned riichi.dev client transcripts
4. Complete rmcp initialize, Resource subscribe, revision wait, and reconnect

## 6. Match domain and engine boundary

Only `double_riichi_core::engine` references engine-specific types. Custom HTTP, WebSocket, MCP, Replay Viewer, and Frontend types use domain types such as:

```text
GameAction
GameEvent
TableState
VisibleTableState
VisibleGameEvent
Decision
```

The engine is the sole authority for rule legality, scoring, ranking, tie-breaking, and preset behavior. The application does not recalculate scores.

### 6.1 Modes

Only these presets are exposed:

```text
4p-red-east
4p-red-half
3p-red-east
3p-red-half
```

No red-five switch, custom scoring option, or detailed rules editor is provided.

### 6.2 Seats and winds

- Four-player Match Seats are indexed `0..3`.
- Three-player Match Seats are indexed `0..2`.
- Three-player state has no dummy fourth Player.
- Wind is a separate domain value from Seat index.
- Public arrays have length three or four according to mode.

Seats are randomized with a CSPRNG for every Match and Rematch. The ephemeral engine seed is not stored, exposed, or configurable in production. Tests may inject a seed through an internal test constructor only.

### 6.3 Complete Game Actions

A legal Game Action contains every tile and target required for application. The domain supports the actions exposed by the pinned presets, including:

```text
Discard(tile, tsumogiri)
RiichiDiscard(tile)
Chi(target, called, consumed)
Pon(target, called, consumed)
Daiminkan(target, called, consumed)
Ankan(consumed)
Kakan(called, consumed)
Nuki(tile)
Tsumo
Ron(target)
Pass
AbortiveDraw
```

Human UI labels the three-player `Nuki` action as `Kita`. `Tsumo`, `Kita`, and `Abortive Draw` appear only when legal.

### 6.4 Decisions and action IDs

The Room or Compat Actor opens one `Decision` containing:

```text
decision_id
eligible Participants
complete legal Game Actions
default Game Action
deadline or watchdog
```

Human and MCP adapters assign ephemeral action IDs to stored legal actions. They submit identifiers rather than reconstructed action objects:

```json
{
  "decision_id": "d42",
  "action_id": "a7"
}
```

MJAI submits the upstream-required action shape, which the adapter matches against the same open Decision. Closed-Decision submissions are stale.

A response Decision gathers one accepted response or timeout from every eligible Player before engine resolution. The first received Pon or Ron does not close the Decision. An accepted legal action cannot be changed; an illegal action may be retried before deadline.

### 6.5 Timeout defaults

Every Decision is created with one deterministic safe default:

1. `Pass` if legal
2. Legal tsumogiri
3. For mandatory discard, the first legal discard in canonical tile order
4. If no safe default exists, Match Abort for engine invariant violation

Timeout never chooses Ron, Tsumo, Riichi, Call, Nuki, or Abortive Draw.

### 6.6 Engine failures

If the engine rejects an action previously validated as legal, state has diverged and the Match is aborted. An unexpected Match or engine task panic is contained by its supervisor.

For a Room Match:

- Participants return to Lobby
- Selection and Ready are cleared
- Incomplete persistence is removed
- The Room and server continue

For a Compat Match, the connection receives the closest upstream-compatible failure and closes. Other Rooms and Matches continue.

## 7. Time controls

Room creation offers only:

```text
riichi.dev
casual
unlimited
```

The `riichi.dev` values and semantics come from the pinned upstream contract.

Casual defaults:

```toml
[time_controls.casual]
turn_seconds = 30
response_seconds = 10
```

Each Decision gets a fresh duration. There is no time bank or increment. All eligible Players in one response Decision share its deadline.

The server's monotonic clock is authoritative. There is no latency estimation or network grace, and server time continues during Frontend animation and reconnect asset loading.

### 7.1 Unlimited safety

- Connected Human: no action deadline
- Disconnected Human: five-minute safety watchdog
- MJAI: five-minute safety watchdog whether connected or disconnected
- MCP: five-minute safety watchdog whether connected or disconnected
- Built-in Bot: immediate action

The five-minute value is configurable within the global duration limits.

### 7.2 Temporary Auto

After a disconnect, the first pending or next Decision waits its normal deadline or safety watchdog. If it times out, the Controller becomes `TemporaryAuto`; subsequent defaults are immediate until reconnect. Reconnect immediately restores interactive control unless the Seat is under Permanent Auto.

### 7.3 Configuration bounds

TOML durations are integer seconds:

```text
turn/response: 1..3600
watchdog:     10..3600
cleanup:       1..86400
shutdown:      1..86400
```

Zero, negative, fractional, and out-of-range values are startup errors.

### 7.4 Timer visibility

- A discard Decision shows a timer near the current Player.
- Public response state shows only one central timer.
- A Player View shows private response timing only when that Player is eligible.
- Public state never identifies who can Chi, Ron, or otherwise respond.

## 8. Room identity and lifecycle

Internal Room IDs and Match IDs are canonical uppercase ULIDs. The external Room identifier is a six-character decimal string from `100000` through `999999`.

Join-code generation uses a CSPRNG and checks active codes and in-process cooldown tombstones. Deleted codes remain unavailable for 24 hours in memory. Cooldown state resets on server restart.

Public Room URLs:

```text
/room/{join_code}
/api/v1/rooms/{join_code}
/ws/v1/rooms/{join_code}
/ws/v1/rooms/{join_code}/mjai
```

### 8.1 Room phases

```text
Lobby
Playing(match_id)
PostMatch(match_id)
```

Transitions:

```text
Lobby -> Playing
Playing -> PostMatch
PostMatch -> Playing       via Rematch
PostMatch -> Lobby         via Back to Lobby
```

An engine Match Abort returns a Room to an unselected Lobby rather than producing final results.

Room configuration can be changed only in Lobby. Room deletion is allowed in Lobby or Post-Match, never while Playing.

### 8.2 Room capacity

Defaults:

```text
max_rooms = 32
max_participants_per_room = 32
```

The Participant cap includes connected and retained disconnected Participants. Reconnect does not consume a new slot. A full Room rejects a new join with `room_full`.

### 8.3 Empty Room cleanup

A Room is empty when it has no connected Human, MJAI, or MCP Participant. Built-in Bots do not keep it non-empty.

- Empty Lobby and Post-Match Rooms are removed after 30 minutes by default.
- A Playing Room is never removed by the empty-Room timer.
- After Match completion, the timer starts if no external Participant is connected.

A disconnected, unselected Human, MJAI, or MCP Participant is removed after ten minutes by default. Reconnect cancels that timer. Active Players, selected Lobby Participants, and retained Post-Match Players are exempt.

## 9. Participants, Lobby, and Rematch

Participant kinds are immutable:

```text
Human
MJAI
MCP
BuiltInBot
```

Independent state axes are:

```text
kind
presence: Connected | Disconnected
lobby_selection: Selected | Unselected
match_role: Player(seat) | Spectator
control: Interactive | TemporaryAuto | PermanentAuto(reason)
```

A room-scoped Participant ULID is the identity authority. Display names may duplicate.

### 9.1 Player selection

A joining Participant begins Unselected. Admin selection requires:

- Connected Human, MJAI, or MCP Participant
- Any Built-in Bot
- An available Match Seat
- No MJAI selection in three-player mode

Selection remains after a later disconnect, but Match start becomes unavailable.

Four-player Matches require exactly four selected Players; three-player Matches require exactly three.

### 9.2 Ready

Only selected Humans explicitly become Ready. MJAI and MCP are Ready while connected. Built-in Bots are always Ready.

A selected Human preloads every selected Player's Character assets. Any selection change, including Fill with Bots, resets all selected Humans to Not Ready.

Human Ready is also cleared by:

- Disconnect
- Deselect
- Game-mode change
- Match start
- Transition to Post-Match
- Back to Lobby

A time-control, replay-save, or Room-name change does not clear Ready.

### 9.3 Fill with Bots

Fill with Bots:

1. Calculates vacant selected Seats
2. Selects existing Unselected Built-in Bots first
3. Creates and selects exactly the remaining count
4. Does nothing when all Seats are selected

Admin may later select, deselect, or remove Built-in Bots.

Built-in strategy is deliberately limited:

```text
Draw decision -> tsumogiri
Response decision -> Pass
```

It does not choose wins, calls, Riichi, Nuki, or strategic discards.

### 9.4 Configuration changes

- Room name, time control, and replay-save changes preserve selection and Ready.
- Game-mode change clears all selection and Ready.
- Four-player to three-player change is rejected while any MJAI Participant remains; Admin must kick it first.
- Configuration changes occur only in Lobby.

### 9.5 Explicit leave

Lobby or Spectator leave removes the Participant and invalidates its credential.

An active Player leave:

- Invalidates its credential
- Retains the immutable Match Player snapshot
- Changes the Seat to `PermanentAuto` with the appropriate reason
- Forbids return to that Match
- Removes the live Room Participant after Match completion

Name, Character, and icon remain those of the original Player for the Match and Results.

Permanent Auto reasons include:

```text
LeftDuringMatch
AgentLeft
TokenRevoked
ConnectionLost where the upstream Compat protocol cannot resume
```

Temporary disconnect and recovery do not create a Results annotation.

### 9.6 Rematch and Back to Lobby

Only Admin initiates Rematch. It requires:

- The same retained Players
- No leave, kick, or Permanent Auto conversion
- All Humans re-Ready
- All MJAI and MCP Players connected

Spectator changes do not affect eligibility. A Rematch creates a new Match ID, CSPRNG seed, and randomized Seats.

Only Admin invokes Back to Lobby. It retains eligible Participants, clears all selection and Human Ready, and excludes already removed Participants.

### 9.7 Room deletion

Deleting a Lobby or Post-Match Room requires Frontend confirmation. The server:

- Notifies connections with `room_deleted`
- Invalidates all Guest Sessions
- Removes the Room
- Preserves completed Replays
- Records one Admin audit entry

## 10. Display names and validation

Room names, Human nicknames, MCP display names, Ranked display-name overrides, Bot Token names, and Character manifest names use the same base validation unless an upstream protocol requires otherwise:

- Trim Unicode whitespace
- Require 1 through 64 Unicode scalar values after trim
- Reject control characters
- Permit Japanese text and emoji
- Permit duplicates
- Do not normalize Unicode
- Render as text, never markup

MCP provider values:

- Are trimmed
- Require 1 through 64 Unicode scalar values
- Reject control characters
- Lowercase ASCII characters only
- Are used only for cosmetic configuration lookup
- Fall back to the generic MCP Character when unknown
- Are never used as paths, filenames, display names, or identity

## 11. Admin authentication and operations

Secrets are read from the required `.env` in the data root:

```text
ADMIN_USERNAME=
ADMIN_PASSWORD_HASH=
```

The username is compared exactly. Login failure does not reveal whether username or password was wrong.

### 11.1 Password hashing

```text
driichi hash-password
```

- Reads and confirms a password without terminal echo
- Never accepts the password through argv
- Requires 12 through 1024 UTF-8 bytes
- Uses Argon2id with 64 MiB memory, three iterations, parallelism one
- Uses a random 16-byte salt
- Writes only a PHC string to stdout
- Writes no hash when confirmation differs

### 11.2 Admin Sessions

- Multiple concurrent sessions are allowed for the single Admin.
- Each uses a 32-byte CSPRNG credential.
- Only its hash is retained in server memory.
- Lifetime is fixed at 12 hours with no sliding extension.
- Cookies are HttpOnly and SameSite Strict.
- HTTPS public origin adds `Secure`.
- Logout invalidates only the current session.
- Server restart invalidates all sessions.
- Credential changes require `.env` update and restart.

Unsafe Admin methods require `Origin == public_origin`; when Origin is absent, an exactly same-origin Referer is the only fallback. Requests with neither are rejected. General Browser CORS is not supported.

### 11.3 Admin operations

Admin mutations use versioned HTTP domain commands, including:

```text
POST   /api/v1/admin/rooms
PATCH  /api/v1/admin/rooms/{join_code}
DELETE /api/v1/admin/rooms/{join_code}
POST   /api/v1/admin/rooms/{join_code}/participants/{participant_id}/select
POST   /api/v1/admin/rooms/{join_code}/participants/{participant_id}/deselect
POST   /api/v1/admin/rooms/{join_code}/participants/{participant_id}/kick
POST   /api/v1/admin/rooms/{join_code}/fill-with-bots
POST   /api/v1/admin/rooms/{join_code}/start
POST   /api/v1/admin/rooms/{join_code}/rematch
POST   /api/v1/admin/rooms/{join_code}/back-to-lobby
```

Room settings are the only generic PATCH. Arbitrary Participant state mutation is not exposed.

The Admin Room list contains:

```text
join_code
room_name
game_mode
phase
connected_count
participant_count
selected_count
created_at
```

Room detail additionally contains Participant ID, kind, presence, selection, Ready, and Character. The selected Room detail is polled every two seconds while its page is visible; mutations trigger immediate refetch. There is no Admin WebSocket or SSE in v1.

Admin cookie identity and an Admin's optional Human Guest Session are separate.

## 12. Human join and WebSocket

Human flow:

```text
Room code -> nickname -> Character -> Join -> Lobby
```

There is no Player/Spectator choice. Admin selects Players.

### 12.1 Public Room lookup

`GET /api/v1/rooms/{join_code}` returns only:

```text
room_name
game_mode
phase
join_allowed
participant_count
participant_limit
```

It does not return Participant names, Characters, Ready state, scores, or IDs.

### 12.2 Join credential

`POST /api/v1/rooms/{join_code}/join` accepts nickname and Human Character ID, creates the Participant, and returns no raw token. It sets an opaque Room-bound HttpOnly Guest cookie scoped to the Room WebSocket path.

- SameSite is Strict.
- Secure is derived from HTTPS `public_origin`.
- The credential remains valid while the Participant remains in the Room.
- Explicit leave, Participant expiry, kick, or Room deletion invalidates it.
- Frontend JavaScript never reads or stores it.

The Human WebSocket authenticates the cookie during HTTP upgrade. There is no first-message auth protocol.

### 12.3 One active connection

One Participant has at most one active Human connection. A newly authenticated connection replaces the old one, which receives `connected_elsewhere` and does not auto-reconnect.

Authenticated Room snapshots and events include Room-scoped Participant IDs as stable UI keys. Public Room lookup does not. An ID alone grants no authority.

### 12.4 Connection flow

```text
connect -> authenticate at upgrade -> full projected snapshot -> projected updates
```

Reconnect always starts with a full snapshot and clears the animation queue. Human actions are never queued offline.

Reconnect uses jittered exponential backoff capped at ten seconds. Input is disabled while disconnected. Invalid credentials or Room deletion return the user to the join flow.

### 12.5 Live updates

Every game update contains both a visible event for animation and the latest audience-projected state for rendering:

```json
{
  "type": "game_update",
  "event": {},
  "state": {}
}
```

The Frontend does not reconstruct canonical state from events. A backlog over 64 animation events drops queued animation and renders the latest state immediately.

Human action results use the Decision ID:

```text
accepted(decision_id)
rejected(decision_id, code)
```

No separate client request ID is introduced. Closed Decisions return stale rejection and the latest available state/Decision.

Ready uses a Human WebSocket `set_ready` message. Admin mutations remain HTTP-only.

### 12.6 Human reconnect

Lobby reconnect restores the Participant, resets Ready, preloads the current selected roster again, and requires a new Ready action.

Playing reconnect restores the original Seat without Ready. Character loading may wait up to three seconds client-side; failure uses placeholder visuals and silence without delaying game-state recovery.

### 12.7 Spectators

Human join during Playing creates a Spectator. It receives Public View only. It remains through Post-Match and becomes an Unselected Participant after Back to Lobby. Spectator preload is best effort and never blocks Match start.

## 13. Visibility security

Canonical Match state contains complete information and is never serialized directly to an untrusted client.

There are exactly three visibility policies:

```text
Player(seat)
Public
ReplayAdmin
```

Mapping:

```text
Human Player / Room MJAI / MCP private -> Player(seat)
Human Spectator / MCP public           -> Public
Admin completed Replay Viewer          -> ReplayAdmin
```

Protocol serializers consume projections only. The server-side canonical MJSON writer is the sole non-engine consumer allowed to record complete Game Events directly.

Required serialized invariants:

- A Player View includes only that Player's concealed information.
- Public View includes no concealed Player information.
- Legal actions appear only for the eligible Player.
- ReplayAdmin alone receives complete post-Match information.
- Three-player output has no dummy fourth Player.
- The invariants hold after JSON serialization, not only in Rust structures.

## 14. MJAI

### 14.1 Compatibility requirement

An existing production Bot targeting riichi.dev `/ws/ranked` or `/ws/validate` must run against `driichi` with no source or protocol-logic change. Only base URL and authentication key may change.

Pinned upstream behavior overrides local preferences for:

- Handshake
- Bearer authentication
- Message schema and ordering
- `request_action`
- Protocol `request_id`
- `action_ack`
- Observation serialization
- Timeouts and defaults
- Ping, close, and reconnect semantics
- Validation success
- Legacy behavior

No Room metadata or `driichi` extension is emitted on Compat endpoints.

### 14.2 Compat endpoints

```text
/ws/ranked
/ws/validate
/status
```

`/ws/ranked` uses `4p-red-half`. Authenticated connections enter a FIFO batch. The first connection starts the configured five-second timer; four connections start immediately; timeout fills remaining Seats with Built-in Bots. A fifth connection begins the next batch. Waiting disconnect removes the queue entry.

The default Ranked display name is the Bot Token name. An optional upstream-compatible `display_name` override uses standard display-name validation.

Unless pinned upstream behavior provides an unambiguous resume contract, a Playing Compat disconnect becomes Permanent Auto because the same Bot Token may have multiple simultaneous instances.

`/ws/validate` uses `4p-red-east`. Unless upstream requires different behavior, one inbound Bot starts with three Built-in Bots and succeeds only when it remains connected and submits a valid action before every deadline through one complete Match. Timeout default makes validation fail even though the Match safely completes.

Defaults:

```text
max_compat_matches = 32
max_ranked_queue = 128
```

Compat connections also count against the server-wide connection limit.

Ranked Replay saving defaults true; Validate Replay saving defaults false. Ranked Replays have no Character metadata.

### 14.3 Room MJAI

Room MJAI is supported only in four-player Rooms.

```http
POST /api/v1/rooms/{join_code}/agents/join
Authorization: Bearer <bot-token>
```

The request contains `display_name`. Each successful call creates a new MJAI Participant, even for the same Token, subject to normal rate and Room limits. It returns the Participant ID and relative WebSocket URL.

```text
/ws/v1/rooms/{join_code}/mjai?participant_id={participant_id}
```

The WebSocket requires the creating Bot Token in the Authorization header. The Participant ID and Token together identify reconnect ownership. One Participant has one active WebSocket; a new connection replaces the old one. Losing the Participant ID prevents recovery, and Admin may kick abandoned Lobby entries.

After connection, game protocol is exactly the pinned riichi.dev Protocol v2 shape without Room-specific messages.

## 15. Bot Tokens

Bot Tokens authenticate both MJAI and MCP and deliberately have no scopes.

Format:

```text
driichi_<32 random bytes encoded Base64 URL-safe without padding>
```

SQLite stores only `SHA-256(raw_token)`. Raw Tokens are shown once at creation and never logged or persisted.

Token records contain:

```text
token_id
name
state: active | revoked
created_at
revoked_at
```

Names may duplicate. There is no `last_used_at`, deletion, rotation, or reactivation.

The server loads Token hashes and states into an in-memory authentication cache at startup. Every MJAI and MCP authentication hashes the supplied Token and checks this cache. Create and revoke update the cache only after SQLite commit. Direct database edits while running are unsupported.

Revocation is global and irreversible:

- New authentication fails generically.
- Waiting connections are removed.
- Lobby Participants and active Spectators are removed.
- Active Players become `PermanentAuto(TokenRevoked)`.
- Post-Match Participants are removed and Rematch becomes unavailable.
- MCP sessions terminate.
- All matching connections across Rooms and Compat Matches close.
- One Admin audit event records the revocation.

Unknown and revoked Tokens both return generic `invalid_credentials` externally.

Token creation returns the raw value once with its non-secret record. The Admin UI provides a copy button, warns that it cannot be shown again, discards it from React state when closed, and never uses localStorage.

## 16. MCP

### 16.1 Transport and authentication

Endpoint:

```text
/mcp
```

Transport is MCP Streamable HTTP implemented with the pinned `rmcp`. v1 intentionally uses a private pre-shared Bot Bearer Token instead of the MCP OAuth authorization profile and does not claim OAuth compatibility.

Every HTTP request revalidates the Bearer Token. `Mcp-Session-Id` is routing state, never authentication.

A transport session is considered connected while its server-issued session ID remains valid. Default idle TTL is 30 minutes and configurable. DELETE, revocation, replacement, or TTL expiry makes the Participant disconnected. A new session must subscribe to Resources again.

### 16.2 Session and Participant identity

One MCP session binds permanently to one Room when its first `join_room` succeeds. It cannot join another Room even after `leave_room`; a new initialized session is required.

One Bot Token may own at most one MCP Participant in a Room. `join_room` is idempotent for that pair:

- A disconnected existing Participant is resumed.
- A new authenticated session replaces the old session.
- Existing display name and Character remain unchanged.
- Response indicates whether the Participant was resumed.

### 16.3 Tools

v1 exposes exactly four Tools:

```text
join_room
leave_room
submit_action
wait_for_turn
```

State reads remain Resources. `wait_for_turn` is synchronization, not a state-returning Tool.

`join_room` input:

```json
{
  "room_code": "482731",
  "provider": "openai",
  "display_name": "Mahjong Agent"
}
```

Output contains no state:

```json
{
  "participant_id": "01ARZ3NDEKTSV4RRFFQ69G5FAV",
  "resumed": false,
  "display_name": "Mahjong Agent",
  "character_id": "mcp-agent",
  "state_uri": "riichi://rooms/482731/participants/01ARZ3NDEKTSV4RRFFQ69G5FAV/state",
  "public_state_uri": "riichi://rooms/482731/public-state",
  "history_uri": "riichi://rooms/482731/history"
}
```

`submit_action` input contains only the current `action_id`. Success returns:

```json
{
  "accepted": true,
  "revision": 43
}
```

Stale, illegal, or wrong-Participant actions are expected Tool errors with stable codes rather than MCP transport errors.

`wait_for_turn` input:

```json
{
  "after_revision": 42,
  "timeout_seconds": 330
}
```

The maximum and default wait is 330 seconds. It returns immediately when a relevant newer revision exists, otherwise waits until update or timeout. Output contains only reason, revision, and state URI.

Relevant wake reasons are:

```text
selected
deselected
match_started
my_decision
round_started
round_ended
game_ended
permanent_auto
room_deleted
server_shutdown
timeout
```

Other Players' normal discards do not wake it.

`leave_room` removes a Lobby Participant or Spectator. An active Player becomes `PermanentAuto(AgentLeft)`. A Post-Match Participant is removed and Rematch becomes unavailable. The old Participant cannot resume.

### 16.4 Resources

```text
riichi://rooms/{code}/participants/{participant_id}/state
riichi://rooms/{code}/public-state
riichi://rooms/{code}/history
```

There is no separate scores Resource because scores already exist in private and public state.

Private state contains:

```text
revision
kyoku and dealer
scores
honba and riichi sticks
remaining tiles
own hand
public discards and melds
riichi state
dora indicators
is_my_turn
legal actions with action_id
decision_expires_at
remaining_ms_at_read
```

Decision timing fields are null when there is no Decision. The server uses monotonic time; timestamps are advisory to the Agent.

History contains detailed events for the current Kyoku and result summaries for previous Kyoku. It does not resend a complete Match event log on every read.

Resource JSON includes a monotonic revision. Revisions identify freshness and lost-wakeup state; they are not an event replay protocol.

Private-state update notifications are sent only for a relevant Decision, selection change, Match/Kyoku boundary, Permanent Auto, deletion, or shutdown. They identify the URI, and the Agent performs `resources/read`.

### 16.5 `driichi-mcp`

The separate binary bridges:

```text
stdio MCP <-> driichi-mcp <-> Streamable HTTP MCP <-> driichi
```

Example:

```text
driichi-mcp --server http://127.0.0.1:3000/mcp
```

Server URL comes from CLI or bridge configuration. The Token comes only from `DRIICHI_MCP_TOKEN` in the bridge process environment. One stdio bridge instance represents one initialized upstream MCP session.

## 17. Character Packs

Characters are cosmetic and never influence rules or actions.

Assignment:

```text
Human:       selected at Room join
Room MJAI:   configured fixed Character
Built-in:    configured fixed Character
MCP:         normalized provider mapping
Ranked:      no Character metadata
```

Characters cannot change while a Participant remains in a Room.

### 17.1 Distribution and licensing

Real Characters are not bundled. Generated CC0 Starter Packs are included in every normal release archive and are also published as a separate versioned ZIP.

- Generator source uses `MIT OR Apache-2.0`.
- Project-owned generated placeholder images/audio use `CC0-1.0`.
- Each Pack includes the full applicable `LICENSE` text.
- Release notices identify the precise CC0-covered files.
- Real Characters, tile art, trademarks, and third-party assets are excluded from the CC0 dedication.
- `character-packs/STARTER_VERSION` records the bundled Starter version.

Starter Packs:

```text
player-red     Human, red
player-blue    Human, blue
mjai-bot       MJAI, green
tsumogiri-bot  Built-in, gray
mcp-agent      MCP, purple
```

Starter voices are silent OGG files. Release workflow generates binary Starter assets with Python and ffmpeg; they are not committed. Parser fixtures live separately under `tests/fixtures/character-pack/`.

### 17.2 Pack structure

```text
character-packs/{id}/
├─ manifest.json
├─ LICENSE
├─ portrait.webp
├─ icon.webp
└─ voices/
   ├─ chi.ogg
   ├─ pon.ogg
   ├─ kan.ogg
   ├─ riichi.ogg
   ├─ ron.ogg
   └─ tsumo.ogg
```

Manifest fields are exactly:

```json
{
  "id": "zundamon",
  "name": "ずんだもん",
  "usage": "human"
}
```

Allowed usage values:

```text
human
mjai
mcp
builtin
```

Unknown fields make the Pack invalid. Future manifest expansion requires an explicit format version.

### 17.3 Registry validation

The server scans once at startup, sorting folder names before validation. There is no hot reload.

Rules:

- ID is 1 through 64 ASCII lowercase kebab characters matching `[a-z0-9][a-z0-9-]{0,63}`.
- Folder name equals manifest ID exactly.
- Canonical paths remain under the Character root.
- Asset HTTP routes resolve only entries in the startup registry.
- Request path text is never converted directly into an arbitrary filesystem path.
- Duplicate Character IDs fail startup.
- An unreferenced broken Pack logs a warning and is ignored.
- A configured or otherwise required unresolved Pack fails startup.

Required at startup:

- At least one valid Human Pack
- Configured MJAI Pack with matching usage
- Configured Built-in Pack with matching usage
- Configured generic MCP Pack with matching usage
- Every configured MCP provider mapping with matching usage

Bounded header validation requires:

```text
manifest.json <= 64 KiB
LICENSE       <= 1 MiB
icon.webp     <= 2 MiB and RIFF/WEBP header
portrait.webp <= 8 MiB and RIFF/WEBP header
each voice    <= 8 MiB and OggS header
```

Dimensions and audio duration are not decoded server-side.

### 17.4 Asset serving

Only these unauthenticated assets are served:

```text
/assets/characters/{id}/portrait.webp
/assets/characters/{id}/icon.webp
/assets/characters/{id}/voices/chi.ogg
/assets/characters/{id}/voices/pon.ogg
/assets/characters/{id}/voices/kan.ogg
/assets/characters/{id}/voices/riichi.ogg
/assets/characters/{id}/voices/ron.ogg
/assets/characters/{id}/voices/tsumo.ogg
```

Manifest, LICENSE, and arbitrary files are never served.

The startup scan computes SHA-256 ETags. Responses use:

```text
Cache-Control: public, no-cache
ETag: "<content-hash>"
```

### 17.5 Human Character API and selection

`GET /api/v1/characters/human` returns only Character ID and name. Frontend sorts with `name.localeCompare()`.

Selection begins empty, and Join remains disabled until a Character is selected. UI uses an icon list, selected portrait, Character name, and a Voice button that previews `riichi.ogg` after a user gesture.

### 17.6 Missing asset fallback

Missing Replay Character Packs and Spectator asset failures do not map to another Pack. Frontend renders a neutral kind/initial placeholder, no portrait effect, and no voice.

### 17.7 Voice behavior

Voice events are:

```text
chi
pon
kan
riichi
ron
tsumo
```

All Kan forms use `kan.ogg`. Client audio settings stored in localStorage are:

```text
Master
SFX
Voice
Voice enabled
```

Ron/Tsumo interrupts a playing Call/Riichi voice. Equal-priority voices play in event order. Multi-Ron uses resolution order. Any voice is stopped after ten seconds. Decode/playback failure is silent and never delays game or animation. There are no subtitles.

### 17.8 Portraits and Results

Normal table areas show icon, Participant display name, and score. A Mangan-or-higher win may show one centered portrait effect with:

```text
Participant display name
Ron or Tsumo
Han and Fu
Limit label
Points gained
```

The full yaku list belongs in the normal Kyoku result panel. All Mangan-or-higher limits use the same effect intensity. Multi-Ron effects use resolution order.

Results show rank, display name, final points, and Permanent Auto annotation for every Player. Only first place receives a large portrait. Character names are not shown during play or Results.

## 18. Frontend table and interaction

Gameplay officially supports desktop and tablet landscape viewports at least 1024 by 600. Smaller viewports show guidance rather than a separate gameplay layout.

The table maintains a fixed aspect ratio and letterboxes instead of distorting tiles.

Orientation:

- Human Player: own Seat at bottom
- Spectator: Seat 0 at bottom
- Replay Viewer: Seat 0 at bottom
- Three-player rotation uses bottom, left, and right

There is no v1 perspective switch.

### 18.1 Rendering ownership

Pixi renders:

- Table and tiles
- Hands, discards, and melds
- Character portrait effects
- Game animation

React DOM renders:

- Action buttons
- Candidate popups
- Timer text
- Settings
- Status and modal UI

Zustand owns WebSocket state, latest projected state, and animation queue. TanStack Query owns HTTP Admin, Room lookup, Character list, and Replay requests.

### 18.2 Human interaction

- Discard: one tile click submits immediately.
- Ron, Tsumo, Pass: button submits immediately.
- Chi, Pon, Kan, Kita: one legal candidate submits immediately; multiple candidates open a popup.
- Riichi: highlights legal discard candidates; selecting a tile submits the compound action.
- Submission disables input until accepted or rejected.
- The Frontend does not optimistically remove a tile.
- Illegal action rejection permits retry while the Decision remains open.

### 18.3 Animation

v1 animations cover draw, discard, calls, win, Riichi, Kyoku start/end, and score changes. The server never waits for animation.

The animation queue is bounded at 64. Overflow, reconnect, or full snapshot cancels pending animation and synchronizes immediately to the latest projected state.

### 18.4 Audio activation

The first user gesture attempts to unlock browser audio. Failure never blocks Ready or play. Ready preload checks asset retrieval/decoding but not successful audible playback.

## 19. Replay

MJSON is newline-delimited UTF-8 JSON with one canonical event per line. One Match equals one file.

Directories:

```text
replays/
├─ 4p/
├─ 3p/
└─ .incomplete/
```

Filename uses Match start UTC and a Match ULID:

```text
20260915T153845Z_4p-red-half_<match_ulid>.mjson
```

No colon appears in filenames.

### 19.1 Writing and failure

A replay-enabled Match opens:

```text
replays/.incomplete/<match_ulid>.mjson.part
```

Events append through a buffered ordered writer. Each Kyoku end flushes. Normal completion performs:

1. Flush
2. File sync
3. Atomic rename into `4p/` or `3p/`
4. SQLite Match completion and relative path update
5. Room transition to Post-Match

If the final SQLite step fails, the Room still enters Post-Match with in-memory Results and Replay unavailable; the renamed file is deleted immediately when possible and otherwise by startup cleanup.

A mid-Match Replay or auxiliary metadata failure never stops gameplay. The failed persistence path stops, health becomes degraded, the Admin sees Replay unavailable, and any partial metadata/artifacts are deleted. No successful Replay is claimed.

At startup, every unfinished `writing` Match, `.part`, and already-renamed file belonging to that unfinished Match is deleted. Recovery, repair, retention, and incomplete-Replay UI are not provided.

### 19.2 Persistent Replay metadata

Only Replay-enabled Matches persist game metadata. Replay-disabled Room Matches and Validate Matches leave no persistent Match record.

Completed Replay metadata includes:

```text
match_id
source: room | ranked
room_name snapshot or null
game_mode
started_at
completed_at
participants and final points
relative replay path
file_size
```

Join codes and engine seeds are not stored. Ranked Participant Character IDs are null.

A Match-start transaction inserts the `writing` Match and immutable Player snapshots only after roster validation, ID/Seat generation, engine initialization, and successful `.part` open. Failure rolls back and leaves the Room in Lobby.

### 19.3 Auxiliary events

Replay-only auxiliary metadata includes:

```text
disconnected
reconnected
auto_started
left
token_revoked
kicked
```

It is not written into MJSON. The position uses a zero-based MJSON line index and `before | after`. Auxiliary events travel through the same ordered Replay channel, which assigns the current emitted-line count. Equal positions sort by phase around the MJSON event and then SQLite ID ascending.

No auxiliary Replay metadata is retained when Replay saving is disabled or fails.

### 19.4 Replay Admin API

Admin Replay list sorts newest first and uses offset pagination:

```text
default limit = 50
maximum limit = 100
```

UI actions are only:

```text
View
Delete
```

There is no raw MJSON download endpoint.

Replay View API returns metadata and a compressed complete timeline of:

```text
ReplayFrame {
  event_index,
  visible_event,
  visible_state,
  auxiliary_events
}
```

The backend Replay crate builds frames; the Frontend has no MJSON state machine. Decompressed response is capped at 64 MiB. Larger Replay returns `replay_too_large` and remains deletable.

A corrupt Replay is not auto-deleted. It appears unavailable, View returns an Admin problem response, internal details go to logs, health is degraded, and Delete remains available.

Delete accepts only Match ID. The server resolves the registered relative path below the Replay root, deletes the file first with missing treated as success, then transactionally deletes Match/Player/auxiliary rows and inserts the Admin audit record. A DB failure leaves a retryable record rather than an inaccessible orphan file.

### 19.5 Replay Viewer

Controls:

```text
Play
Pause
Previous Event
Next Event
0.5x / 1x / 2x / 4x
Jump to Kyoku
```

Previous, Next, and Jump pause playback. There is no arbitrary timeline slider.

Auxiliary events appear as a status toast and event-log entry, not tile animation. Ordering is `before`, MJSON event, then `after`.

Room Replays use current Character assets and normal local audio rules. Missing Packs and Ranked Replays use generic silent presentation with no portrait effect.

Canonical MJSON is omniscient and Admin-only. It is never delivered through Public View. A future public Replay requires a server-side projected representation.

## 20. SQLite

Startup settings:

```text
pool connections = 4
busy_timeout = 5 seconds
journal_mode = WAL
foreign_keys = ON
synchronous = NORMAL
```

Migrations are embedded and applied transactionally at startup. Failure stops startup. v1 provides no automatic backup or down migration.

SQLite stores:

- Bot Token hashes and state
- Replay-enabled Match metadata
- Immutable Match Player snapshots
- Replay auxiliary events
- Admin audit logs

It does not store:

- Active Room state
- Replay-disabled Match metadata
- A duplicate per-action Match log
- Raw credentials
- Engine seeds

Completed Replay paths are normalized relative to the data root and revalidated below that root for every read or deletion.

### 20.1 Match Player snapshot

Created at Match start and immutable:

```text
match_id
participant_id
display_name
participant_kind
seat
character_id or null
final_points when completed
```

Permanent Auto never changes identity fields.

### 20.2 Audit logs

Audit stores successful state-changing Admin operations only:

- Login and logout
- Room creation, configuration, and deletion
- Selection, deselection, kick, fill, start, Rematch, and Back to Lobby
- Bot Token creation and revocation
- Replay deletion

List, View, Health, failed login, validation error, and gameplay are not audited in SQLite. Failed operations remain in tracing logs.

Audit fields:

```text
id
occurred_at
request_id
action
target_type
target_id
allowlisted summary_json
```

Request bodies, passwords, cookies, Authorization headers, and raw Tokens are forbidden. Retention defaults to 90 days and runs at startup and every 24 hours. Cleanup failure logs a warning without stopping the server.

## 21. HTTP and protocol contract

Custom HTTP uses `/api/v1`; custom WebSocket uses `/ws/v1`. Compat and MCP routes remain outside those prefixes.

### 21.1 Problem Details

Custom HTTP errors use `application/problem+json`:

```json
{
  "type": "about:blank",
  "title": "Room not found",
  "status": 404,
  "detail": "The room does not exist.",
  "code": "room_not_found",
  "request_id": "01ARZ3NDEKTSV4RRFFQ69G5FAV"
}
```

`code` is stable snake-case machine input. `detail` is display text, not a stable parsing contract. Internal errors and paths are never returned.

### 21.2 Request IDs

The server generates an uppercase ULID per HTTP request, returns it in `X-Request-ID`, and records it in tracing and problem bodies. Client-provided request IDs are ignored. This identifier is distinct from MJAI protocol `request_id`.

### 21.3 Serialization

Custom contracts use:

- Snake-case fields and enum values
- UTC RFC 3339 timestamps
- Uppercase canonical ULID strings
- Six-character string join codes, never JSON numbers
- Signed integer points
- Explicit schema-defined nulls

Unknown fields are rejected for `/api/v1` and Human `/ws/v1`. Compat, MCP, and MJSON parsers follow their pinned upstream contracts instead.

Size limits:

```text
Custom HTTP JSON: 64 KiB
Human WS message:  64 KiB
MJAI WS message:    1 MiB
MCP HTTP body:      1 MiB
```

### 21.4 Specs

`spec/openapi.yaml` and `spec/asyncapi.yaml` are hand-authored contract sources. Generated source is neither committed nor produced during normal builds.

CI:

- Validates both documents with exact-pinned tools
- Validates representative fixtures against schemas
- Compares running-server integration responses with fixtures

AsyncAPI completely defines Human `/ws/v1`. Room MJAI documents only its join/auth wrapper and points to the pinned riichi.dev contract. Compat endpoints use upstream fixtures. MCP uses the pinned MCP/RMCP schema rather than a duplicate OpenAPI model.

API docs are OFF by default. When enabled, an Admin-only fixed route serves the embedded raw OpenAPI YAML without adding Swagger UI.

## 22. Network and browser security

### 22.1 Bind and TLS

Default bind is `127.0.0.1`. External bind requires explicit configuration. `driichi` serves HTTP and WS only; Caddy, nginx, Cloudflare, or another reverse proxy terminates HTTPS/WSS.

`public_origin` is required and is the source of truth for Browser Origin checks and Secure cookie behavior:

```toml
public_origin = "http://127.0.0.1:3000"
```

v1 Browser APIs are same-origin only. There is no configurable CORS allowlist.

### 22.2 Origin rules

- Human WS requires Origin equal to `public_origin`.
- Room MJAI permits absent Origin; a present Origin must equal `public_origin`.
- Compat WS follows pinned upstream behavior.
- MCP HTTP permits absent Origin; a present Origin must equal `public_origin`.
- Unsafe Admin requests follow the stricter Origin/Referer rule in §11.

### 22.3 Credentials

- Bot Tokens use `Authorization: Bearer` only.
- Human and Admin credentials use HttpOnly cookies.
- Credentials are never accepted in query strings, URLs, or WebSocket subprotocols.
- Room MJAI keeps only its non-secret Participant ID in the query.

Authenticated clients receive actionable close reasons:

```text
server_shutdown
room_deleted
token_revoked
connected_elsewhere
slow_consumer
session_expired
```

Unauthenticated failures remain generic.

### 22.4 Rate and connection limits

Defaults:

```text
Server connections:       256
Room Participants:         32
Code lookup:               20/minute/IP
Participant creation:      10/minute/IP
Admin login failures:       5/15 minutes/IP
Agent auth failures:       20/minute/IP
```

All are configurable. Rate-limiter entries expire and remain memory-bounded.

Direct peer IP is used unless that peer belongs to a configured trusted proxy CIDR. For a trusted peer, `X-Forwarded-For` is scanned right-to-left and the first untrusted address becomes the rate-limit IP. Malformed headers fall back to the direct peer. Forwarded IP never grants authorization. `X-Forwarded-Proto` is ignored.

### 22.5 WebSocket liveness and malformed input

Custom WebSockets send Ping every 20 seconds and treat absence of Pong for 60 seconds as disconnect. Compat heartbeat follows upstream behavior.

- Malformed custom JSON/schema sends a generic error and closes for policy violation.
- Oversized messages close with code 1009.
- Stale or illegal Game Actions keep the connection open for retry where possible.
- Internal connection errors close generically.

### 22.6 Static security headers

Embedded Frontend responses include:

```text
Content-Security-Policy
X-Content-Type-Options: nosniff
Referrer-Policy: no-referrer
X-Frame-Options: DENY
Permissions-Policy
```

CSP is self-oriented, allows no inline script, and permits only required local Character media and the configured server connection.

Cache policy:

```text
index.html:        no-cache
Vite hashed files: public, max-age=31536000, immutable
Character assets: public, no-cache with ETag
```

SPA fallback excludes `/api`, `/ws`, `/mcp`, and `/assets/characters`.

## 23. Configuration and startup

```text
driichi --config <path>
```

Default config is `config.toml` beside the executable. The loaded config file's parent is the data root for:

```text
.env
double-riichi.db
character-packs/
replays/
```

There is no separate `--data-dir`.

Admin secrets come only from the required data-root `.env`; ambient process variables do not override them. Duplicate keys, missing values, and a missing `.env` are startup errors. `driichi-mcp` separately reads `DRIICHI_MCP_TOKEN` from its own process environment.

Unknown `config.toml` fields, wrong types, and duplicate keys are startup errors. `config.toml.example` documents every setting and default.

Startup automatically creates:

```text
double-riichi.db
replays/
replays/4p/
replays/3p/
replays/.incomplete/
```

`config.toml`, `.env`, `character-packs/`, and required valid Packs must already exist. DB and Replay write probes must pass. Unix group/world-readable `.env` permissions emit a security warning but do not block startup. Windows ACLs are not modified or interpreted.

## 24. Health, logging, and shutdown

### 24.1 Health

`GET /api/v1/health` requires Admin authentication.

It returns:

- Version and short commit
- Uptime
- DB status
- Replay storage status
- Active Rooms
- Active Room Matches
- Active Compat Matches

Healthy returns HTTP 200. DB or Replay degradation returns HTTP 503 with bounded component status and no filesystem paths or internal error chain. Replay storage is probed in a background task every 60 seconds.

Public `/status` returns only the minimum pinned riichi.dev-compatible response.

### 24.2 Logging

`tracing` format is configurable as `text | json`, default text. Request logs contain only:

```text
request_id
route template
method
status
latency
peer IP
authenticated Admin or Token record ID
Room, Match, and Participant IDs where relevant
```

Raw query strings, bodies, response bodies, cookies, Authorization values, passwords, and raw credentials are never logged.

### 24.3 Graceful shutdown

Default deadline is ten seconds and configurable.

1. Stop accepting new connections, Rooms, and Matches.
2. Notify authenticated clients with `server_shutdown`.
3. Wait only for in-flight commands and storage effects until deadline.
4. Abort active Matches.
5. Remove incomplete Replay and DB artifacts where possible.
6. Leave remaining cleanup to the next startup.

The server never tries to auto-play a full Match during shutdown. Active Rooms are not restored after restart.

## 25. Release and CI

### 25.1 Release archive

Each platform archive contains:

```text
driichi
driichi-mcp
README
config.toml.example
.env.example
LICENSE-MIT
LICENSE-APACHE
THIRD_PARTY_NOTICES
character-packs/ with generated CC0 Starter Packs
```

It excludes DB, runtime configuration, secrets, and Replays.

First-run setup remains manual:

```text
Copy config.toml.example to config.toml
Copy .env.example to .env
Run driichi hash-password and place the PHC hash in .env
Run driichi
```

There is no `driichi init` command.

`driichi --version` prints:

```text
driichi <semver> (<short-git-sha>)
```

Release notes and build metadata record dependency revisions and tested Starter version.

Every archive and standalone Starter ZIP has a published SHA-256 checksum. v1 provides no Windows code signing or Sigstore signature.

### 25.2 Targets

```text
Windows x86_64
Linux x86_64
Linux ARM64
```

Linux artifacts build on Ubuntu 22.04 and support compatible later Ubuntu/Debian-family systems. musl and macOS artifacts are not provided.

Gates:

- Windows x86_64: native build and smoke test
- Linux x86_64: native build and full test
- Linux ARM64: native ARM build and smoke test
- Every extracted archive runs `driichi --version`

### 25.3 Tile assets

Licensed SVG/PNG tile sources are pinned and vendored into the repository and Frontend bundle. No runtime CDN is used. Source URL, revision, license, and attribution are recorded in `THIRD_PARTY_NOTICES`.

### 25.4 Commands

```text
just check
just test-rust
just test-frontend
just test-contract
just test-yamai
just test-e2e
just test-all
just test-live
just build-release
```

`test-all` contains deterministic local tests and requires no external credential. `test-live` is manual/scheduled and connects to real riichi.dev.

### 25.5 Test requirements

Required categories:

- Unit tests
- Integration tests
- Serialized visibility tests
- Room Actor tests with controlled Tokio time
- MJAI protocol fixture tests
- MCP tests
- Replay tests
- Frontend tests
- Playwright E2E
- Release smoke tests

Playwright completes two Matches through the real Rust server and visible Human UI:

```text
4p-red-east: 1 Human + 3 Built-in Bots
3p-red-east: 1 Human + 2 Built-in Bots
```

The test selects the first legal UI action until completion and verifies Room creation, Human join, selection, preload Ready, Match start, actions, Results, Replay creation, and MJSON validity.

Pinned `yamai` is fetched into `.cache/yamai/<sha>`, its checked-out SHA is verified, and the generated four-player MJSON is processed to completion by the real `ReplayProcessor`. GitHub Actions cache may avoid repeated downloads; unavailable required source fails the test. No submodule or vendored yamai copy is used.

Scheduled compatibility testing runs the same production-style Bot client against `driichi` and riichi.dev with only base URL and authentication key changed. Drift opens an alert or issue and does not automatically redefine or break the pinned release contract.

## 26. v1 Definition of Done

v1 is complete only when all items pass:

1. `driichi` starts as a self-contained Windows/Linux release binary.
2. Admin can authenticate and create, configure, and delete Rooms.
3. Browser Humans can join with Room code, nickname, and Character.
4. A four-player Human Room Match completes.
5. A three-player Human Room Match completes.
6. Built-in Bots fill vacant selected Seats.
7. An unchanged riichi.dev production Bot plays through `/ws/ranked` after only URL and authentication-key changes.
8. Room MJAI reuses the same pinned game protocol logic.
9. An MCP Agent joins, waits, reads Resources, and submits action IDs through a complete Match.
10. Human, Room MJAI, and MCP disconnect/reconnect behavior matches this specification.
11. Replay-enabled Matches produce complete MJSON.
12. Admin Replay Viewer plays the complete timeline with the specified controls.
13. Pinned `yamai ReplayProcessor` consumes a generated four-player Replay completely.
14. Character Packs pass strict startup loading and allowlisted serving.
15. A release archive starts using its bundled CC0 Starter Packs after documented configuration.
16. Serialized Player/Public projections leak no unauthorized concealed information.
17. Playwright completes both required three-player and four-player flows.
18. Exact external evidence gates in §5 are recorded and green.

At that point the status changes from **Conditional Design Freeze** to **Design Frozen**, and the completed release is **double-riichi v1**.
