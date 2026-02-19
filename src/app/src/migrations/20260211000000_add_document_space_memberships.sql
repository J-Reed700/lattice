-- Add schema-level document-to-space memberships for hard retrieval scoping.

CREATE TABLE IF NOT EXISTS document_space_memberships (
    document_id TEXT NOT NULL,
    space_id TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (document_id, space_id),
    FOREIGN KEY (document_id) REFERENCES documents(id) ON DELETE CASCADE,
    FOREIGN KEY (space_id) REFERENCES conversation_spaces(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_document_space_memberships_space_doc
    ON document_space_memberships (space_id, document_id);

CREATE INDEX IF NOT EXISTS idx_document_space_memberships_document_space
    ON document_space_memberships (document_id, space_id);

-- Backfill existing conversation-linked documents into their conversation space.
INSERT OR IGNORE INTO document_space_memberships (document_id, space_id)
SELECT DISTINCT
    cd.document_id,
    c.space_id
FROM conversation_documents cd
INNER JOIN conversations c ON c.id = cd.conversation_id
WHERE cd.document_id IS NOT NULL
  AND TRIM(cd.document_id) <> ''
  AND c.space_id IS NOT NULL
  AND TRIM(c.space_id) <> '';
