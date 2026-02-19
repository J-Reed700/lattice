-- Files table with content-addressable storage
CREATE TABLE IF NOT EXISTS files (
    id TEXT PRIMARY KEY NOT NULL,
    content_hash TEXT NOT NULL UNIQUE,
    file_name TEXT NOT NULL,
    file_extension TEXT,
    mime_type TEXT NOT NULL,
    size_bytes INTEGER NOT NULL,
    storage_path TEXT NOT NULL,
    is_indexed INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL,
    accessed_at INTEGER NOT NULL,
    ref_count INTEGER NOT NULL DEFAULT 1 CHECK (ref_count >= 0),
    metadata TEXT
) STRICT;

CREATE INDEX IF NOT EXISTS idx_files_hash ON files(content_hash);
CREATE INDEX IF NOT EXISTS idx_files_mime ON files(mime_type);
CREATE INDEX IF NOT EXISTS idx_files_accessed ON files(accessed_at);
CREATE INDEX IF NOT EXISTS idx_files_extension ON files(file_extension);

-- File references (many-to-many with documents)
CREATE TABLE IF NOT EXISTS file_references (
    id TEXT PRIMARY KEY NOT NULL,
    file_id TEXT NOT NULL,
    document_id TEXT NOT NULL,
    reference_type TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    FOREIGN KEY (file_id) REFERENCES files(id) ON DELETE CASCADE,
    FOREIGN KEY (document_id) REFERENCES documents(id) ON DELETE CASCADE,
    UNIQUE(file_id, document_id, reference_type)
) STRICT;

CREATE INDEX IF NOT EXISTS idx_file_refs_file ON file_references(file_id);
CREATE INDEX IF NOT EXISTS idx_file_refs_doc ON file_references(document_id);
