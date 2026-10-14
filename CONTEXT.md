# Double Riichi

Double Riichi is a self-hosted riichi mahjong service where humans and software agents meet in rooms and play complete matches.

## Language

**Room**:
A persistent gathering place in which participants wait, are selected, and play one or more matches.
_Avoid_: Game room, table

**Lobby**:
The room phase in which participants gather and the Admin selects players for the next match.
_Avoid_: Waiting room, pre-game

**Post-Match**:
The room phase after final results and before either a rematch or a return to the Lobby.
_Avoid_: Results state, game over

**Participant**:
A room-scoped identity representing a human, an external agent, or a built-in bot. Its identity is independent of its connection, selection, seat, and controller.
_Avoid_: User, client, connection

**Participant Kind**:
The immutable origin of a participant: Human, MJAI, MCP, or Built-in Bot.
_Avoid_: Player type, controller type

**Presence**:
Whether a participant currently has a live connection. Presence does not determine identity, selection, or seat ownership.
_Avoid_: Participant state

**Controller**:
The current source of actions for a seated player. A controller may change without changing the player’s identity or seat.
_Avoid_: Participant, player kind

**Temporary Auto**:
Automatic play used while an interactive controller is unavailable. Control returns when that controller reconnects.
_Avoid_: Disconnect, bot replacement

**Permanent Auto**:
The irreversible transfer of a player's control to automatic play for the remainder of a match. The original player identity and seat remain unchanged.
_Avoid_: Bot replacement, disconnect

**Guest Session**:
A Room-bound Human identity authenticated by an opaque credential for the lifetime of that Room.
_Avoid_: User account, Admin session

**Bot Token**:
A named credential that authenticates MJAI and MCP participants. It is not itself a Participant and may authenticate multiple participant identities where the protocol permits.
_Avoid_: Bot, participant ID, MCP session

**Revocation**:
The irreversible invalidation of a Bot Token and every connection authenticated by it.
_Avoid_: Disable, deactivate, logout

**Player**:
A participant selected to take a seat in a match.
_Avoid_: Active participant

**Spectator**:
A participant who observes a match without occupying a seat.
_Avoid_: Viewer, observer

**Match**:
One complete east-only or east-south contest, from seat assignment through final results.
_Avoid_: Game, round

**Room Match**:
A Match arranged through a Room's Lobby for selected Human or software Participants.
_Avoid_: Custom game

**Compat Match**:
A Match arranged through a riichi.dev-compatible endpoint without a Room, Lobby, or join code.
_Avoid_: Hidden room, ranked room

**Benchmark Mode**:
A bot-only Room mode where chosen supported software Participants repeatedly play finite, sequential Matches within one selected three-player or four-player ruleset. The Admin observes one live table and reviews accumulated results; an A/B research Experiment is NOT part of the currently approved design. Benchmark v2 participants must meet the planned version/capability admission protocol. Humans, supplied missing Bots and automatic concurrent 3p+4p modes are not supported.
_Avoid_: Ranked mode, bot replacement

**Benchmark Run**:
An automatically progressing, finite series of Bot-vs-Bot Matches among a fixed roster in one game mode. Matches progress sequentially until the configured 1–1,000 completed-Match target, Admin Stop or failure. The Admin sees live play, outcomes and cumulative statistics; results belong to Participants, not Bot Tokens. Aborted/unrecorded Matches do not count; restart recovery requires explicit Admin Resume.
_Avoid_: Rematch, Replay

**Stopped Benchmark Run**:
A Benchmark Run intentionally ended by an Admin before its configured target; the current Match finishes and is persisted when possible, and the actual completed/target counts and historical results remain accessible. Distinct from Completed (target reached) and Interrupted (unexpected crash), and is not implicitly resumed.
_Avoid_: Completed Run, Interrupted Run, deleting history

**Benchmark Admin Observer**:
An authenticated Admin observing a live Benchmark Match with access to every player's concealed information. Ordinary Spectators remain limited to the Public View.
_Avoid_: Spectator, Replay View

**Benchmark Dashboard**:
The Admin's primary view for managing ordinary Benchmark Runs, watching one live 3D Mahjong table (expandable to fullscreen) and reviewing current progress, per-Match and cumulative Bot statistics and Replay history.
_Avoid_: A/B-only Experiment report, game controller

**Benchmark v2 Capability**:
A versioned Participant-scoped Bot identity/build declaration and clean-reset capability required for admission to the future Benchmark v2 protocol. It is trusted without independently proving which model binary executed.
_Avoid_: Bot Token, A/B candidate, runtime attestation

**Benchmark Protocol Mismatch**:
The refusal of a Bot Participant to enter a Benchmark because its advertised capability/protocol version is incompatible with the required Benchmark version. It is an admission failure, not evidence that the Bot played or failed a Match.
_Avoid_: Disconnect during Match, invalid Game Action

**Benchmark Protocol Preflight**:
A non-mutating assessment of a Bot Participant's declared compatibility with the required Benchmark protocol, which can be Compatible, Incompatible or Unknown when evidence cannot be obtained.
_Avoid_: Runtime attestation, connection health, completed Match

**Benchmark Drain**:
The deliberate suspension of scheduling new Benchmark Matches while the currently active Match receives a bounded chance to finish and preserve its result before a planned Benchmark Bot protocol cutover.
_Avoid_: Deleting Run history, restarting a Match, migrating a Bot by assumption

**Benchmark Match Attempt**:
A single attempt to execute a scheduled ordinary Benchmark Run Match. A failed/incomplete Attempt is recorded separately; a retry creates a new Attempt of the same Match without overwriting old evidence.
_Avoid_: Completed Match, Rematch

**Server-observed Latency**:
Response timing observed by the game server, distinguished into time to first response and time until a legal Game Action is accepted. It is different from optional Bot-reported inference time.
_Avoid_: Inference time, guaranteed model compute time

**Trusted Bot Declaration**:
A code/build or checkpoint identity claimed for one software Participant by an authenticated Bot Token holder and recorded for the Benchmark Run. This is an operator-trusted claim, not independent proof of the running executable.
_Avoid_: Runtime attestation, measured binary

**Live Table Fullscreen**:
The focused display of a single active Mahjong 3D table expanded beyond its ordinary dashboard panel, without creating another Match, changing game state or showing three concurrent tables.
_Avoid_: Multi-table viewer, additional Match, simultaneous Run

**Match Reset Acknowledgement**:
A trusted authenticated Bot Participant's explicit, scoped declaration that it returned to a clean Match state for an eligible retry under its recorded build/checkpoint. For Built-in Bots the server records its own reset operation.
_Avoid_: Reconnection, process restart proof

**Observation Focus**:
The software Participant selected by a Benchmark Admin Observer to anchor the table viewing perspective and display an enlarged concealed hand. Focus follows that Participant when Seats rotate between Matches; it never changes the Participant's identity, Seat, Controller, or game state.
_Avoid_: Controller, Player View

**Kyoku**:
One dealt hand within a match, such as East 1.
_Avoid_: Round, hand

**Seat**:
A player's indexed position for one match. A three-player Match has three Seats and a four-player Match has four; no dummy Seat is created.
_Avoid_: Slot

**Wind**:
A mahjong position value used for prevailing and player winds. It is distinct from a Seat index.
_Avoid_: Seat

**Display Name**:
User-visible text naming a Room or Participant. It is not an identity and duplicates are allowed.
_Avoid_: ID, username

**Decision**:
A single opportunity for one or more eligible players to choose from currently legal actions before the opportunity closes.
_Avoid_: Turn, request, prompt

**Game Action**:
One complete legal choice accepted for an open Decision, including every tile and target needed to apply it.
_Avoid_: Command, move, partial action

**Match Abort**:
Termination without final results because the Match implementation can no longer preserve valid game state.
_Avoid_: Draw, game end, server shutdown result

**Player View**:
The table information visible to one seated player, including that player's private information.
_Avoid_: Private state, client state

**Public View**:
The table information visible without occupying a seat, containing no concealed player information.
_Avoid_: Spectator state, shared state

**Replay View**:
The complete post-Match information available to the Admin replay viewer.
_Avoid_: Public view, live state

**Replay**:
The ordered record of one completed match.
_Avoid_: Game log, history

**Rematch**:
A new match played by the same retained players after a completed match, with newly assigned seats.
_Avoid_: Match restart, replay

**Character**:
A participant's cosmetic representation, with no effect on play.
_Avoid_: Player, avatar ability

**Character Pack**:
A licensed, replaceable bundle that defines one Character and its fixed portrait, icon, and voice assets.
_Avoid_: Plugin, mod, gameplay pack
