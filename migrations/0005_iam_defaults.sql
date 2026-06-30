-- Default allow-all policy: any authenticated user can SELECT any dataset.
-- Add explicit Deny policies (higher priority) to restrict access.
INSERT OR IGNORE INTO iam_subjects (id, subject_type, name)
    VALUES ('00000000-0000-0000-0000-000000000001', 'user', '*');

INSERT OR IGNORE INTO iam_policies (id, subject_id, resource_pattern, action, effect, priority)
    VALUES (
        '00000000-0000-0000-0000-000000000002',
        '00000000-0000-0000-0000-000000000001',
        '*',
        'SELECT',
        'Allow',
        0
    );
