# Prefer Match completion over persistence

Once play starts, Replay or metadata failure degrades health and makes Replay unavailable but does not stop the Match or suppress in-memory Results and Rematch. Incomplete Matches are not recoverable: shutdown and startup delete unfinished database rows, partial files, and renamed files tied to unfinished records, while successful Replays use ordered buffering, Kyoku flushes, final sync and rename, then SQLite completion.
