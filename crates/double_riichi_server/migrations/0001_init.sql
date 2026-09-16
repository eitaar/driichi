CREATE TABLE bot_tokens (
    token_id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL CHECK (length(name) BETWEEN 1 AND 64),
    token_hash BLOB NOT NULL UNIQUE CHECK (length(token_hash) = 32),
    state TEXT NOT NULL CHECK (state IN ('active', 'revoked')),
    created_at INTEGER NOT NULL,
    revoked_at INTEGER,
    CHECK (
        (state = 'active' AND revoked_at IS NULL)
        OR (state = 'revoked' AND revoked_at IS NOT NULL)
    )
);

CREATE TABLE matches (
    match_id TEXT PRIMARY KEY NOT NULL,
    source TEXT NOT NULL CHECK (source IN ('room', 'ranked')),
    room_name TEXT,
    game_mode TEXT NOT NULL,
    started_at INTEGER NOT NULL,
    completed_at INTEGER,
    status TEXT NOT NULL CHECK (status IN ('writing', 'completed')),
    replay_path TEXT,
    file_size INTEGER CHECK (file_size IS NULL OR file_size >= 0),
    CHECK (
        (status = 'writing' AND completed_at IS NULL)
        OR (status = 'completed' AND completed_at IS NOT NULL AND replay_path IS NOT NULL AND file_size IS NOT NULL)
    )
);

CREATE TABLE match_players (
    match_id TEXT NOT NULL REFERENCES matches(match_id) ON DELETE CASCADE,
    participant_id TEXT NOT NULL,
    display_name TEXT NOT NULL,
    participant_kind TEXT NOT NULL,
    seat INTEGER NOT NULL CHECK (seat BETWEEN 0 AND 3),
    character_id TEXT,
    final_points INTEGER,
    PRIMARY KEY (match_id, seat),
    UNIQUE (match_id, participant_id)
);

CREATE TABLE replay_auxiliary_events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    match_id TEXT NOT NULL REFERENCES matches(match_id) ON DELETE CASCADE,
    line_index INTEGER NOT NULL CHECK (line_index >= 0),
    phase TEXT NOT NULL CHECK (phase IN ('before', 'after')),
    sequence INTEGER NOT NULL CHECK (sequence >= 0),
    payload_json TEXT NOT NULL CHECK (json_valid(payload_json)),
    UNIQUE (match_id, sequence)
);

CREATE TABLE audit_logs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    occurred_at INTEGER NOT NULL,
    request_id TEXT NOT NULL,
    action TEXT NOT NULL,
    target_type TEXT NOT NULL,
    target_id TEXT NOT NULL,
    summary_json TEXT NOT NULL CHECK (json_valid(summary_json))
);

CREATE INDEX idx_bot_tokens_state ON bot_tokens(state);
CREATE INDEX idx_matches_status_started_at ON matches(status, started_at);
CREATE INDEX idx_matches_completed_at ON matches(completed_at);
CREATE INDEX idx_match_players_match_id ON match_players(match_id);
CREATE INDEX idx_replay_auxiliary_events_match_id ON replay_auxiliary_events(match_id, line_index, sequence);
CREATE INDEX idx_audit_logs_occurred_at ON audit_logs(occurred_at);
