# Share a protocol-neutral Match machine

A Room Actor and a Room-less Compat Match Actor each own the same protocol-neutral Match machine, while only the Room adds Lobby, join-code, selection, and Rematch behavior. The core owns domain state but sends ordered persistence effects through bounded channels, keeping engine, HTTP, SQLite, Replay, MJAI, and MCP dependencies acyclic and preventing a second rules implementation.
