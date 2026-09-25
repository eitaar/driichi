---
name: interactive-play
description: Use the private driichi MCP connection for interactive Room play when the user asks; join, inspect the connected participant's state, submit requested legal actions, and leave.
---

# Interactive play

Use the driichi MCP tools to play only when the user asks for an action in a Room.

1. Join the requested Room with `join_room`. Ask for a room code or participant name if it is needed and was not provided.
2. Use `get_my_state` to inspect only the private state bound to this MCP participant. Treat the returned `legal_actions` as the available choices; do not infer or disclose other participants' hidden state.
3. Submit an action with `submit_action` only when the user requests it and it appears in the current legal actions. If the requested action is not legal, explain that and wait for the user's next instruction.
4. Use `wait_for_turn` only during the active conversation when the user asks you to wait. Background or automatic turns are not guaranteed.
5. Use `leave_room` when the user asks you to leave or the requested Room task is finished.

Do not ask for, repeat, or expose administrator credentials, Bot Tokens, or OAuth tokens. Do not claim a move succeeded until the MCP tool returns success.
