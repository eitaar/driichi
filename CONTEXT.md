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
