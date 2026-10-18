-- Keep vector identifiers until the derived summary index confirms removal.
-- This table deliberately has no document foreign key: a deletion tombstone
-- must outlive the document row it is cleaning up.
CREATE TABLE summary_vector_cleanup (
    vector_id TEXT NOT NULL,
    model_identity TEXT NOT NULL,
    requested_at INTEGER NOT NULL DEFAULT (unixepoch()),
    PRIMARY KEY (model_identity, vector_id)
);

CREATE INDEX summary_vector_cleanup_order
    ON summary_vector_cleanup(model_identity, requested_at, vector_id);

-- Capture every row deletion, including document FK cascades and callers that
-- bypass SummaryRepositoryPort::delete_for_document. The cleanup row commits
-- with the source-row deletion and survives the document's cascade.
CREATE TRIGGER document_summaries_capture_vector_delete
AFTER DELETE ON document_summaries
BEGIN
    INSERT OR IGNORE INTO summary_vector_cleanup(vector_id, model_identity)
    VALUES ('summary_' || OLD.id, OLD.model_identity);
END;
