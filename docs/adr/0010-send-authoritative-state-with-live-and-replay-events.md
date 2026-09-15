# Send authoritative state with live and Replay events

Every Human live update carries both a projected event for animation and the latest projected state for rendering, so the Frontend never reconstructs canonical state and can discard excessive or disconnected animation safely. The backend Replay crate likewise produces complete Admin Replay Frames from MJSON, allowing the same table renderer to play Replays without a second Frontend game-state machine.
