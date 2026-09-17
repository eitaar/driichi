CREATE TABLE admin_audit_pending (
    request_id TEXT PRIMARY KEY NOT NULL,
    occurred_at INTEGER NOT NULL,
    action TEXT NOT NULL,
    target_type TEXT NOT NULL,
    target_id TEXT NOT NULL,
    summary_json TEXT NOT NULL CHECK (json_valid(summary_json)),
    state TEXT NOT NULL CHECK (state IN ('prepared', 'applied'))
);

CREATE INDEX idx_admin_audit_pending_state ON admin_audit_pending(state, occurred_at);
