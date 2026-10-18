-- Only identities are queued. The worker reads the latest committed note.
-- Triggers make the note mutation and its writeback intent one transaction.
CREATE TABLE vault_write_outbox (
    note_id TEXT PRIMARY KEY NOT NULL,
    revision TEXT NOT NULL,
    mirror_complete INTEGER NOT NULL DEFAULT 0,
    retry_at INTEGER NOT NULL DEFAULT 0
);

CREATE TRIGGER vault_note_insert AFTER INSERT ON daily_notes_workspace BEGIN
    INSERT INTO vault_write_outbox(note_id, revision)
    VALUES (NEW.id, lower(hex(randomblob(16))))
    ON CONFLICT(note_id) DO UPDATE SET revision = excluded.revision, mirror_complete = 0, retry_at = 0;
END;

CREATE TRIGGER vault_note_update AFTER UPDATE OF title, content ON daily_notes_workspace BEGIN
    INSERT INTO vault_write_outbox(note_id, revision)
    VALUES (NEW.id, lower(hex(randomblob(16))))
    ON CONFLICT(note_id) DO UPDATE SET revision = excluded.revision, mirror_complete = 0, retry_at = 0;
END;

CREATE TRIGGER vault_note_delete AFTER DELETE ON daily_notes_workspace BEGIN
    INSERT INTO vault_write_outbox(note_id, revision)
    VALUES (OLD.id, lower(hex(randomblob(16))))
    ON CONFLICT(note_id) DO UPDATE SET revision = excluded.revision, mirror_complete = 0, retry_at = 0;
END;
