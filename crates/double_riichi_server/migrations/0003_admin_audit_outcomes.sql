DROP INDEX IF EXISTS idx_admin_audit_pending_state;

ALTER TABLE admin_audit_pending RENAME TO admin_audit_pending_old;

CREATE TABLE admin_audit_pending (
    request_id TEXT PRIMARY KEY NOT NULL,
    occurred_at INTEGER NOT NULL,
    action TEXT NOT NULL,
    target_type TEXT NOT NULL,
    target_id TEXT NOT NULL,
    summary_json TEXT NOT NULL CHECK (json_valid(summary_json)),
    state TEXT NOT NULL CHECK (state IN ('prepared', 'applied', 'rolled_back'))
);

INSERT INTO admin_audit_pending
    (request_id, occurred_at, action, target_type, target_id, summary_json, state)
SELECT request_id, occurred_at, action, target_type, target_id, summary_json, state
FROM admin_audit_pending_old;

DROP TABLE admin_audit_pending_old;

CREATE INDEX idx_admin_audit_pending_state ON admin_audit_pending(state, occurred_at);
