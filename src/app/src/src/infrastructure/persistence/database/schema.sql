-- ============================================================================
-- DEPRECATED: This file is for reference only.
-- ============================================================================
-- 
-- The canonical schema is defined in src/db/schema.rs
-- 
-- DO NOT USE THIS FILE FOR INITIALIZATION
-- DO NOT MODIFY THIS FILE
-- 
-- This file is kept for historical reference only. All schema changes must be
-- made in src/db/schema.rs and applied through the migration system.
-- 
-- For the current schema, see: src/db/schema.rs
-- For migrations, see: src/db/migrate.rs
-- 
-- ============================================================================

-- Optimized SQLite Schema v2.0
CREATE TABLE schema_version (version INTEGER PRIMARY KEY, applied_at INTEGER NOT NULL, description TEXT NOT NULL) STRICT;
CREATE TABLE documents (id TEXT PRIMARY KEY, filename TEXT NOT NULL, content TEXT, created_at INTEGER NOT NULL) STRICT, WITHOUT ROWID;

-- FTS5 full-text search table for BM25 keyword search
CREATE VIRTUAL TABLE IF NOT EXISTS documents_fts USING fts5(
    document_id UNINDEXED,
    content,
    tokenize='porter unicode61 remove_diacritics 2',
    content=documents,
    content_rowid=rowid
);

-- Triggers to keep FTS5 table in sync with documents table
CREATE TRIGGER IF NOT EXISTS documents_ai AFTER INSERT ON documents BEGIN
    INSERT INTO documents_fts(rowid, document_id, content) VALUES (new.rowid, new.id, new.content);
END;

CREATE TRIGGER IF NOT EXISTS documents_ad AFTER DELETE ON documents BEGIN
    INSERT INTO documents_fts(documents_fts, rowid, document_id, content) VALUES('delete', old.rowid, old.id, old.content);
END;

CREATE TRIGGER IF NOT EXISTS documents_au AFTER UPDATE ON documents BEGIN
    INSERT INTO documents_fts(documents_fts, rowid, document_id, content) VALUES('delete', old.rowid, old.id, old.content);
    INSERT INTO documents_fts(rowid, document_id, content) VALUES (new.rowid, new.id, new.content);
END;
