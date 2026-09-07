-- Passage references: a saved excerpt from a document, with enough locator
-- information to reopen the source at the right place.
--
-- Distinct from conversation_message_bookmarks (which reference a chat turn).
-- Both are surfaced together in the Reference inbox; they are different
-- conceptual entities and therefore separate tables (CLAUDE.md SSOT rule 4).
--
-- document_id is intentionally NOT a foreign key: a reference outlives the
-- removal of a document from the index, and the inbox degrades to "source no
-- longer indexed" rather than losing the saved text.

CREATE TABLE IF NOT EXISTS passage_references (
    id TEXT PRIMARY KEY,
    document_id TEXT NOT NULL,
    chunk_id TEXT,
    file_path TEXT NOT NULL,
    file_name TEXT NOT NULL,
    locator TEXT,
    text TEXT NOT NULL,
    title TEXT,
    note TEXT,
    created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_passage_references_created_at
    ON passage_references(created_at DESC);

CREATE INDEX IF NOT EXISTS idx_passage_references_document_id
    ON passage_references(document_id);

CREATE INDEX IF NOT EXISTS idx_passage_references_chunk_id
    ON passage_references(chunk_id);
