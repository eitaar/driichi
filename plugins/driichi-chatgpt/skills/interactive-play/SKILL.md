---
name: interactive-play
description: Play riichi mahjong through the driichi MCP server. Use when asked to join a room, inspect the agent's hand, choose and submit legal moves, continue a match, observe turn changes, or leave a room.
---

# driichi MCP: agent play

Use the connected driichi MCP server as the authoritative interface for a single participant. This skill covers gameplay, not administration or server implementation. The MCP connection must already be configured and authenticated by the host. Never request or display Bot Tokens, OAuth tokens, or administrator credentials.

## Available interface

Tools (exact names): `join_room`, `get_my_state`, `submit_action`, `wait_for_turn`, `leave_room`. The connected host may prefix these names with its MCP server name.

- `join_room({"room_code":"123456","provider":"chatgpt","display_name":"Agent"})`: join or resume the participant. Use the actual provider of the connected client, rather than assuming `chatgpt` in other hosts. It returns `participant_id`, `resumed`, `state_uri`, `public_state_uri`, and `history_uri`. Ask for a missing room code or display name only when needed.
- `get_my_state({})`: read this participant's private projected state. Important fields: `revision`, `phase`, `is_my_turn`, `legal_actions`, `decision`, `remaining_ms_at_read`, `decision_expires_at`, and `state_uri`. `legal_actions` comes from `decision.actions`.
- `submit_action({"action_id":"<exact current action_id>"})`: submit one offered action identifier. Success is reported by `accepted` and a new `revision`. Do not submit tile names, an invented action, or an obsolete identifier.
- `wait_for_turn({"after_revision":123,"timeout_seconds":30})`: wait for a relevant revision. It returns `reason`, `revision`, and `state_uri`, **not** the new state. The server allows 0–330 seconds; 0 uses the server's 330-second default. Prefer a modest timeout suitable for the host/client, and read state after waking.
- `leave_room({})`: permanently end this MCP participant. Do not use it merely to stop observing or to end a chat: leaving can prevent resuming that participant.

After joining, the MCP resources can also be read through the host's resource interface using the returned exact URIs: private state `riichi://rooms/{code}/participants/{participant_id}/state`, public state `riichi://rooms/{code}/public-state`, and history `riichi://rooms/{code}/history`. The host may support resource subscriptions and updated notifications. `get_my_state` remains the straightforward fallback when resource access/subscriptions are not exposed. Public/history resources do not confer access to opponents' concealed information.

## Play loop

1. Join once, retain the returned room binding and URIs, then call `get_my_state`. A new session may resume an existing participant; check `resumed`.
2. If `legal_actions` is nonempty, reason from the **current returned state** and choose only among those entries. Read the exact `action_id` field, including where an action contains nested options. The server's action list is authoritative for chi/pon/kan/riichi/ron/tsumo/pass/discard and any other actions; do not assume any option exists.
3. For a user-requested **single move**, submit that move only if available; otherwise explain which current choices are available. For an explicit request to **play or continue the match autonomously**, choose legal moves according to the user's stated strategy without asking for approval each turn. If no strategy is specified, use a reasonable mahjong strategy and state relevant uncertainty; never invent hidden tiles or assert a guaranteed win.
4. Call `submit_action` with precisely the selected ID. Check `accepted === true` before reporting success. Refresh state before another move. One server revision may contain multiple opportunities; do not assume each submission ends your turn.
5. When there is no legal action, inspect `phase`. If the match is active, call `wait_for_turn` with the **latest observed revision**, then `get_my_state` after a wake or timeout. On `timeout`, check state again; do not treat it as a move. If a notification reports an updated resource, read the resource or state rather than taking the notification as complete game data.
6. On `game_ended`, inspect the latest state/history, give the result, and stop the play loop. On `round_ended` or `round_started`, refresh and continue if the user requested the whole match. Stop on `room_deleted`, `server_shutdown`, `permanent_auto`, expired session, cancellation, or the user's stop instruction.
7. Calls are foreground, bounded by the active conversation and the host's execution limits. Do not promise unattended/background play, unlimited polling, or a 5-second scheduler unless the host actually provides it. If interrupted, explain the latest confirmed action and revision; do not claim continued play.

## Choosing a move

- Base choices on visible hand, discards, melds, dora, scores, round/seat context, and the current legal action list. Opponents' concealed tiles and future draws are unknown.
- Evaluate winning and response opportunities, shanten/ukeire, value, placement, and defensive risk as relevant. If an action is time-sensitive, prioritize submitting a valid action before its decision deadline instead of spending the entire window deliberating.
- Passing is an actual action only when its ID appears in the current list. Red fives, drawn-tile versus hand-discard variants, and different call options may have distinct IDs. Preserve the exact server IDs.
- If the user specifies a move, locate the matching listed action (including its relevant nested data), and request clarification only when multiple offered actions still match materially different interpretations.

## Failure handling

- `stale_action` / `illegal_action`: immediately refresh with `get_my_state`; never blindly replay the old action ID.
- `session_expired`: reconnect/reinitialize the MCP transport, then use `join_room` to attempt a resume if the host permits it. Never assume the previous session is still bound.
- `session_already_bound`: an existing session is bound to another room; do not silently switch participants. Explain the conflict.
- `room_not_found`, `room_full`, `invalid_input`: report the actual error and ask only for the missing or corrected detail.
- `busy` or transient transport failure: bounded retry or refresh, not a tight loop; distinguish an unknown result from confirmed rejection. If a submission's result is uncertain, refresh state before deciding whether to retry.
- `leave_unavailable`, `wrong_participant`, `invalid_credentials`, `permanent_auto`: stop and explain the blocker rather than switching identities or using privileged endpoints.

Never disclose another participant's hidden state, expose credentials, fabricate game state, claim a move succeeded before confirmation, or turn an observation request into an unsolicited move. `leave_room` is destructive for the participant; call it only when explicitly requested or when the user clearly authorized permanent departure.
