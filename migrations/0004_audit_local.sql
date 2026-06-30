CREATE TABLE IF NOT EXISTS audit_events (
    id             TEXT PRIMARY KEY,
    event_type     TEXT NOT NULL,
    occurred_at    TEXT NOT NULL,
    source_service TEXT NOT NULL,
    user_id        TEXT NOT NULL,
    session_id     TEXT,
    payload_json   TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_audit_occurred   ON audit_events(occurred_at DESC);
CREATE INDEX IF NOT EXISTS idx_audit_event_type ON audit_events(event_type);
CREATE INDEX IF NOT EXISTS idx_audit_user_id    ON audit_events(user_id);
