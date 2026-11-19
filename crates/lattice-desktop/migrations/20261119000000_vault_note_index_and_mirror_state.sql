-- Maps each mirrored vault note to its current indexed document row so edits
-- supersede the previous draft instead of duplicating it in RAG and CAS, and
-- removals resolve the stored document id regardless of CAS file_path rewriting.
CREATE TABLE IF NOT EXISTS vault_note_documents (
    note_id TEXT PRIMARY KEY NOT NULL,
    document_id TEXT NOT NULL UNIQUE REFERENCES documents(id) ON DELETE CASCADE
);

-- Tracks the SHA-256 hex digest of the markdown file last observed/written by
-- Lattice so writeback does not overwrite newer external edits and watcher
-- imports do not loop back through the outbox. Stored outside
-- `vault_write_outbox` so settling/acknowledging an outbox entry does not erase
-- the baseline digest for subsequent edits.
CREATE TABLE IF NOT EXISTS vault_mirror_state (
    note_id TEXT PRIMARY KEY NOT NULL,
    last_mirrored_sha256 TEXT NOT NULL
);
