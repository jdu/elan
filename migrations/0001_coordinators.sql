CREATE TABLE IF NOT EXISTS coordinators (
    id                TEXT    PRIMARY KEY,
    environment       TEXT    NOT NULL,
    hostname          TEXT    NOT NULL,
    executor_endpoint TEXT    NOT NULL,
    registered_at     TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    last_heartbeat_at TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    is_alive          INTEGER NOT NULL DEFAULT 1
);
