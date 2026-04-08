CREATE TABLE contacts (
    jid TEXT NOT NULL,
    full_name TEXT NOT NULL DEFAULT '',
    first_name TEXT NOT NULL DEFAULT '',
    push_name TEXT NOT NULL DEFAULT '',
    device_id INTEGER NOT NULL DEFAULT 1,
    updated_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
    PRIMARY KEY (jid, device_id)
);
