CREATE TABLE IF NOT EXISTS custom_collections (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('manual', 'snapshot')),
    parent_id TEXT REFERENCES custom_collections(id) ON DELETE CASCADE DEFERRABLE INITIALLY DEFERRED,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_custom_collections_sibling_name
    ON custom_collections(COALESCE(parent_id, ''), name COLLATE NOCASE);

CREATE TABLE IF NOT EXISTS custom_collection_documents (
    collection_id TEXT NOT NULL REFERENCES custom_collections(id) ON DELETE CASCADE,
    document_id TEXT NOT NULL,
    ordinal INTEGER NOT NULL,
    PRIMARY KEY (collection_id, document_id)
);

CREATE INDEX IF NOT EXISTS idx_custom_collection_documents_document_id
    ON custom_collection_documents(document_id);
