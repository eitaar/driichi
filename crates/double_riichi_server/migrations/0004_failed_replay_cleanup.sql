PRAGMA foreign_keys = OFF;

DROP INDEX IF EXISTS idx_matches_status_started_at;
DROP INDEX IF EXISTS idx_matches_completed_at;
DROP INDEX IF EXISTS idx_match_players_match_id;
DROP INDEX IF EXISTS idx_replay_auxiliary_events_match_id;

ALTER TABLE match_players RENAME TO match_players_old;
ALTER TABLE replay_auxiliary_events RENAME TO replay_auxiliary_events_old;
ALTER TABLE matches RENAME TO matches_old;

CREATE TABLE matches (
    match_id TEXT PRIMARY KEY NOT NULL,
    source TEXT NOT NULL CHECK (source IN ('room', 'ranked')),
    room_name TEXT,
    game_mode TEXT NOT NULL,
    started_at INTEGER NOT NULL,
    completed_at INTEGER,
    status TEXT NOT NULL CHECK (status IN ('writing', 'failed', 'completed')),
    replay_path TEXT,
    file_size INTEGER CHECK (file_size IS NULL OR file_size >= 0),
    CHECK (
        (status IN ('writing', 'failed') AND completed_at IS NULL)
        OR (status = 'completed' AND completed_at IS NOT NULL AND replay_path IS NOT NULL AND file_size IS NOT NULL)
    )
);

INSERT INTO matches (
    match_id, source, room_name, game_mode, started_at, completed_at, status,
    replay_path, file_size
)
SELECT match_id, source, room_name, game_mode, started_at, completed_at, status,
       replay_path, file_size
FROM matches_old;

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

INSERT INTO match_players (
    match_id, participant_id, display_name, participant_kind, seat, character_id,
    final_points
)
SELECT match_id, participant_id, display_name, participant_kind, seat, character_id,
       final_points
FROM match_players_old;

CREATE TABLE replay_auxiliary_events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    match_id TEXT NOT NULL REFERENCES matches(match_id) ON DELETE CASCADE,
    line_index INTEGER NOT NULL CHECK (line_index >= 0),
    phase TEXT NOT NULL CHECK (phase IN ('before', 'after')),
    sequence INTEGER NOT NULL CHECK (sequence >= 0),
    payload_json TEXT NOT NULL CHECK (json_valid(payload_json)),
    UNIQUE (match_id, sequence)
);

INSERT INTO replay_auxiliary_events (
    id, match_id, line_index, phase, sequence, payload_json
)
SELECT id, match_id, line_index, phase, sequence, payload_json
FROM replay_auxiliary_events_old;

DROP TABLE match_players_old;
DROP TABLE replay_auxiliary_events_old;
DROP TABLE matches_old;

CREATE INDEX idx_matches_status_started_at ON matches(status, started_at);
CREATE INDEX idx_matches_completed_at ON matches(completed_at);
CREATE INDEX idx_match_players_match_id ON match_players(match_id);
CREATE INDEX idx_replay_auxiliary_events_match_id ON replay_auxiliary_events(match_id, line_index, sequence);

PRAGMA foreign_keys = ON;
