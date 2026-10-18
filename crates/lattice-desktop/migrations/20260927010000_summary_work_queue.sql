CREATE TABLE summary_work_queue (
    document_id TEXT PRIMARY KEY NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    revision TEXT NOT NULL,
    requested_at INTEGER NOT NULL,
    retry_at INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX summary_work_queue_order ON summary_work_queue(requested_at, document_id);

-- Admission is part of the same transaction that publishes the indexed
-- document state. The notification hook only wakes the worker; if it or its
-- connection fails, the durable intent still exists for startup recovery.
CREATE TRIGGER documents_enqueue_summary_work_after_insert
AFTER INSERT ON documents
WHEN NEW.status = 'indexed'
BEGIN
    INSERT INTO summary_work_queue(document_id, revision, requested_at)
    VALUES (NEW.id, lower(hex(randomblob(16))), unixepoch())
    ON CONFLICT(document_id) DO UPDATE SET
        revision = excluded.revision,
        requested_at = excluded.requested_at,
        retry_at = 0;
END;

CREATE TRIGGER documents_enqueue_summary_work_after_index
AFTER UPDATE OF status ON documents
WHEN NEW.status = 'indexed'
BEGIN
    INSERT INTO summary_work_queue(document_id, revision, requested_at)
    VALUES (NEW.id, lower(hex(randomblob(16))), unixepoch())
    ON CONFLICT(document_id) DO UPDATE SET
        revision = excluded.revision,
        requested_at = excluded.requested_at,
        retry_at = 0;
END;
