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
A bot-only Room mode for controlled evaluation of software Participants across Matches, including comparison of playing strength and research experiments. A Benchmark uses its selected three-player **or** four-player game mode exclusively; starting either does not automatically execute the other. Human Participants cannot play; MJAI, MCP, and Built-in Bot Participants may play subject to game-mode restrictions, but missing players are never automatically supplied.
_Avoid_: Ranked mode, bot replacement

**Experiment**:
A research comparison of eligible verified Benchmark Runs, which may include previously completed Runs or newly arranged sequential Runs. An Experiment selects one exact game mode, never automatically both 3p and 4p. Each Run retains its own participant identities, results, and history.
_Avoid_: Benchmark Run, Match

**Verified Benchmark Run**:
A Benchmark Run with complete recorded game conditions and trusted Bot Token-authenticated build/checkpoint declarations. Identity is accepted under the operator's trust policy rather than independently measured at runtime.
_Avoid_: All historical Runs, independent binary attestation

**Benchmark Run**:
A fixed roster's series of one or more completed and recorded Matches, with seats rotated between Matches. Aborted or unrecorded Matches do not count toward its target; results belong to each Participant rather than the Bot Token it uses.
_Avoid_: Rematch, Replay

**Benchmark Admin Observer**:
An authenticated Admin observing a live Benchmark Match with access to every player's concealed information. Ordinary Spectators remain limited to the Public View.
_Avoid_: Spectator, Replay View

**Paired Evaluation**:
A comparison of software Participants using recorded, Seat-balanced blocks of Matches with matched planned seed conditions. Equal seed schedules do not mean equal trajectories when Bot decisions differ.
_Avoid_: Identical replay, same match outcome

**Benchmark Match Attempt**:
A single attempt to execute a scheduled Benchmark Match. Incomplete attempts remain separately recorded as failed; a retry is a new Attempt of the same planned Match and never changes the result of previous attempts.
_Avoid_: Completed Match, Rematch

**Server-observed Latency**:
Response timing observed by the game server, distinguished into time to first response and time until a legal Game Action is accepted. It is different from optional Bot-reported inference time.
_Avoid_: Inference time, guaranteed model compute time

**Experiment Configuration**:
The declared conditions defining which Benchmark Runs may be compared in an Experiment. They become fixed at the first Match start for newly run experiments, or at the first eligible historical Run attachment/explicit retrospective comparison start for historical experiments.
_Avoid_: Annotation, Admin display preferences

**Diagnostic Log**:
Optional detailed evidence collected during a Benchmark, governed by storage and retention limits. Bodies whose secrets cannot safely be redacted are omitted and marked as such. Missing detail is acknowledged separately from durable Match statistics.
_Avoid_: Benchmark result, Replay

**Trusted Bot Declaration**:
Code/build or checkpoint identity claimed for one software Participant by an authenticated holder of an Admin-issued Bot Token, recorded for its Benchmark Run. It is trusted by the operator but not independently proven to be the running executable.
_Avoid_: Runtime attestation, measured binary

**Paired Block**:
A complete A/B comparison under one chosen game mode and matched seed conditions: in 3p, three Matches for A and three for B (six total); in 4p, four for A and four for B (eight total). Each Candidate Model occupies every Seat once against the fixed reference roster. Only fully recorded A/B blocks contribute to paired effect statistics.
_Avoid_: Kyoku, single Match

**Analysis Seed**:
A recorded seed for statistical resampling and uncertainty estimation, kept independent of the random seeds that govern actual Mahjong Matches.
_Avoid_: Root Seed, per-Match seed

**Experiment Run Queue**:
The persisted ordered collection of Benchmark Runs scheduled under one Experiment. Long evaluations can span multiple sequential Runs without increasing any Run's Match limit.
_Avoid_: Current Match, concurrent Runs

**Descriptive-only Run**:
A verified Benchmark Run attached to an Experiment whose results do not meet the Experiment's paired fixture conditions. It appears separately from paired statistics and is excluded from paired effect and paired uncertainty calculations.
_Avoid_: Unverified Run, Paired Block

**Blocked Experiment**:
An Experiment awaiting a potentially recoverable prerequisite, which an Admin may resolve before explicit resumption.
_Avoid_: Terminal failure, Cancelled Experiment

**Failed Experiment**:
An Experiment terminated by exhausted Match retries or an unrecoverable integrity error; existing results and failure evidence remain accessible.
_Avoid_: Blocked Experiment, Cancelled Experiment

**Paired Effect**:
The measured difference between two Candidate Models across complete matched Paired Blocks of one exact game mode. For mean placement, Candidate A advantage is mean placement(B) minus mean placement(A); a positive value favors A. Relative score and first-place rate are secondary, and uncertainty is assessed by block-level bootstrap intervals.
_Avoid_: Single-Match ranking, guaranteed model superiority

**Reference Bot**:
A software Participant held unchanged across the A/B candidate comparison so the other seats provide the same reference opponents.
_Avoid_: Candidate model, alternate test condition

**Candidate Model**:
One of two software implementations, A or B, evaluated in the same candidate Seat positions against an unchanged roster of Reference Bots. A model version is not itself a Participant identity.
_Avoid_: Participant identity, Bot Token

**Match Reset Acknowledgement**:
A trusted, authenticated external software Participant's explicit declaration that it has returned to a clean initial Match state under its recorded build/checkpoint identity. For Built-in Bots, the server records its own reset operation.
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
