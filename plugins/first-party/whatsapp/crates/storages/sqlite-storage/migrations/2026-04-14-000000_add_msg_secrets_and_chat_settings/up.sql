-- Per-message encryption secret storage.
-- Stores secrets from MessageContextInfo for later decryption of
-- poll votes, encrypted reactions, comments, and bot messages.

CREATE TABLE message_secrets (
    device_id INTEGER NOT NULL,
    chat_jid TEXT NOT NULL,
    sender_jid TEXT NOT NULL,
    message_id TEXT NOT NULL,
    secret BLOB NOT NULL,
    PRIMARY KEY (device_id, chat_jid, sender_jid, message_id)
);

-- Local chat settings (mute, pin, archive).
-- Tracks per-chat preferences synced from the app state protocol.

CREATE TABLE chat_settings (
    device_id INTEGER NOT NULL,
    chat_jid TEXT NOT NULL,
    muted_until INTEGER,
    pinned INTEGER NOT NULL DEFAULT 0,
    archived INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (device_id, chat_jid)
);
