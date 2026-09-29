CREATE TABLE benchmark_runs (
    run_id TEXT PRIMARY KEY NOT NULL,
    room_code TEXT NOT NULL,
    game_mode TEXT NOT NULL,
    target INTEGER NOT NULL CHECK (target BETWEEN 1 AND 1000),
    completed INTEGER NOT NULL DEFAULT 0 CHECK (completed >= 0 AND completed <= target),
    status TEXT NOT NULL CHECK (status IN ('running', 'completed', 'stopped', 'failed', 'interrupted')),
    reason TEXT,
    failed_match_id TEXT
);

CREATE TABLE benchmark_roster (
    run_id TEXT NOT NULL REFERENCES benchmark_runs(run_id) ON DELETE CASCADE,
    participant_id TEXT NOT NULL,
    display_name TEXT NOT NULL,
    participant_kind TEXT NOT NULL,
    initial_seat INTEGER NOT NULL CHECK (initial_seat BETWEEN 0 AND 3),
    character_id TEXT,
    PRIMARY KEY (run_id, participant_id),
    UNIQUE (run_id, initial_seat)
);

CREATE TABLE benchmark_matches (
    run_id TEXT NOT NULL REFERENCES benchmark_runs(run_id) ON DELETE CASCADE,
    sequence INTEGER NOT NULL CHECK (sequence BETWEEN 1 AND 1000),
    match_id TEXT NOT NULL UNIQUE REFERENCES matches(match_id) ON DELETE RESTRICT,
    PRIMARY KEY (run_id, sequence)
);

CREATE TABLE benchmark_results (
    run_id TEXT NOT NULL,
    sequence INTEGER NOT NULL,
    participant_id TEXT NOT NULL,
    seat INTEGER NOT NULL CHECK (seat BETWEEN 0 AND 3),
    final_score INTEGER NOT NULL,
    rank INTEGER NOT NULL CHECK (rank BETWEEN 1 AND 4),
    PRIMARY KEY (run_id, sequence, participant_id),
    UNIQUE (run_id, sequence, seat),
    FOREIGN KEY (run_id, sequence) REFERENCES benchmark_matches(run_id, sequence) ON DELETE CASCADE,
    FOREIGN KEY (run_id, participant_id) REFERENCES benchmark_roster(run_id, participant_id)
);
