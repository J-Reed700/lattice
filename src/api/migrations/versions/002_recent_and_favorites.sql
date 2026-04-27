-- Migration: Add Recent Documents and Favorites tracking
-- Date: 2025-11-10

-- Document access tracking
CREATE TABLE IF NOT EXISTS document_access (
    id TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
    user_id TEXT NOT NULL DEFAULT 'default',
    document_id TEXT NOT NULL,
    accessed_at INTEGER NOT NULL DEFAULT (unixepoch()),
    access_count INTEGER NOT NULL DEFAULT 1,
    created_at INTEGER NOT NULL DEFAULT (unixepoch()),
    updated_at INTEGER NOT NULL DEFAULT (unixepoch()),
    FOREIGN KEY (document_id) REFERENCES documents(id) ON DELETE CASCADE
) STRICT;

-- Favorites
CREATE TABLE IF NOT EXISTS favorites (
    id TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
    user_id TEXT NOT NULL DEFAULT 'default',
    document_id TEXT NOT NULL,
    favorited_at INTEGER NOT NULL DEFAULT (unixepoch()),
    note TEXT,
    UNIQUE(user_id, document_id)
) STRICT;

-- Indexes for performance
CREATE INDEX IF NOT EXISTS idx_document_access_user_doc ON document_access(user_id, document_id);
CREATE INDEX IF NOT EXISTS idx_document_access_accessed ON document_access(accessed_at DESC);
CREATE INDEX IF NOT EXISTS idx_document_access_count ON document_access(access_count DESC);
CREATE INDEX IF NOT EXISTS idx_favorites_user ON favorites(user_id);
CREATE INDEX IF NOT EXISTS idx_favorites_user_doc ON favorites(user_id, document_id);
CREATE INDEX IF NOT EXISTS idx_favorites_favorited ON favorites(favorited_at DESC);

-- Trigger: Update access count on repeated access (upsert logic)
CREATE TRIGGER IF NOT EXISTS trigger_document_access_upsert
    BEFORE INSERT ON document_access
    FOR EACH ROW
    WHEN EXISTS (
        SELECT 1 FROM document_access
        WHERE user_id = NEW.user_id AND document_id = NEW.document_id
    )
BEGIN
    UPDATE document_access
    SET
        accessed_at = NEW.accessed_at,
        access_count = access_count + 1,
        updated_at = NEW.accessed_at
    WHERE user_id = NEW.user_id AND document_id = NEW.document_id;

    SELECT RAISE(IGNORE);
END;

-- View: Recent documents with details
CREATE VIEW IF NOT EXISTS v_recent_documents AS
SELECT
    da.id as access_id,
    da.user_id,
    da.document_id,
    da.accessed_at,
    da.access_count,
    d.path as file_path,
    d.filename,
    d.extension,
    d.mime_type,
    d.size_bytes,
    d.file_modified_at,
    (f.id IS NOT NULL) as is_favorite
FROM document_access da
JOIN documents d ON da.document_id = d.id
LEFT JOIN favorites f ON da.user_id = f.user_id AND da.document_id = f.document_id
ORDER BY da.accessed_at DESC;

-- View: Favorites with details
CREATE VIEW IF NOT EXISTS v_favorite_documents AS
SELECT
    f.id as favorite_id,
    f.user_id,
    f.document_id,
    f.favorited_at,
    f.note,
    d.path as file_path,
    d.filename,
    d.extension,
    d.mime_type,
    d.size_bytes,
    d.file_modified_at,
    da.access_count,
    da.accessed_at as last_accessed_at
FROM favorites f
JOIN documents d ON f.document_id = d.id
LEFT JOIN document_access da ON f.user_id = da.user_id AND f.document_id = da.document_id
ORDER BY f.favorited_at DESC;
