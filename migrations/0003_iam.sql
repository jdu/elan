CREATE TABLE IF NOT EXISTS iam_subjects (
    id           TEXT PRIMARY KEY,
    subject_type TEXT NOT NULL CHECK(subject_type IN ('user','group')),
    name         TEXT NOT NULL UNIQUE
);

CREATE TABLE IF NOT EXISTS iam_group_members (
    group_id TEXT NOT NULL REFERENCES iam_subjects(id) ON DELETE CASCADE,
    user_id  TEXT NOT NULL REFERENCES iam_subjects(id) ON DELETE CASCADE,
    PRIMARY KEY (group_id, user_id)
);

CREATE TABLE IF NOT EXISTS iam_policies (
    id               TEXT    PRIMARY KEY,
    subject_id       TEXT    NOT NULL REFERENCES iam_subjects(id) ON DELETE CASCADE,
    resource_pattern TEXT    NOT NULL,
    action           TEXT    NOT NULL,
    effect           TEXT    NOT NULL CHECK(effect IN ('Allow','Deny')),
    row_filter       TEXT,
    column_mask_json TEXT,
    priority         INTEGER NOT NULL DEFAULT 0,
    created_at       TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);

CREATE INDEX IF NOT EXISTS idx_policies_subject ON iam_policies(subject_id);
CREATE INDEX IF NOT EXISTS idx_policies_effect  ON iam_policies(effect);
