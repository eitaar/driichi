# Require drop-in riichi.dev compatibility

A production Bot must switch between riichi.dev and `driichi` `/ws/ranked` or `/ws/validate` by changing only base URL and authentication key. The pinned upstream handshake, messages, acknowledgements, observation encoding, timing, heartbeat, reconnect, failure, validation, and legacy behavior override local preferences; Compat Matches reuse the real Match machine but expose no Room metadata or extensions.
