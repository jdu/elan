CREATE TABLE IF NOT EXISTS datasets (
    id                TEXT    PRIMARY KEY,
    name              TEXT    NOT NULL,
    namespace         TEXT    NOT NULL,
    source_type       TEXT    NOT NULL,
    coordinator_id    TEXT    NOT NULL REFERENCES coordinators(id) ON DELETE CASCADE,
    executor_endpoint TEXT    NOT NULL,
    arrow_schema_ipc  BLOB    NOT NULL,
    metadata_json     TEXT,
    registered_at     TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    last_seen_at      TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    is_active         INTEGER NOT NULL DEFAULT 1,
    UNIQUE(namespace, name)
);

CREATE INDEX IF NOT EXISTS idx_datasets_coordinator ON datasets(coordinator_id);
CREATE INDEX IF NOT EXISTS idx_datasets_namespace   ON datasets(namespace);
CREATE INDEX IF NOT EXISTS idx_datasets_active      ON datasets(is_active);
