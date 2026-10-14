ALTER TABLE matches ADD COLUMN authoritative_results_version INTEGER;
ALTER TABLE matches ADD COLUMN authoritative_result_json TEXT;

CREATE TABLE match_kyoku_results (
    match_id TEXT NOT NULL REFERENCES matches(match_id) ON DELETE CASCADE,
    kyoku_sequence INTEGER NOT NULL,
    version INTEGER NOT NULL,
    end_event_index INTEGER NOT NULL,
    payload_json TEXT NOT NULL,
    PRIMARY KEY (match_id, kyoku_sequence)
);
