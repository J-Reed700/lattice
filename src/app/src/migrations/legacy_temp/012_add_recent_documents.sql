-- Migration: Add recent_documents table
-- Version: 012
-- Description: Creates table for tracking recently accessed documents

CREATE TABLE IF NOT EXISTS recent_documents (
    document_id TEXT PRIMARY KEY NOT NULL,
    last_accessed_at TEXT NOT NULL,
    access_count INTEGER NOT NULL DEFAULT 1,
    FOREIGN KEY (document_id) REFERENCES documents(id) ON DELETE CASCADE
) STRICT;

-- Index for chronological queries (most recent first)
CREATE INDEX IF NOT EXISTS idx_recent_documents_last_accessed ON recent_documents(last_accessed_at DESC);

-- Index for access count queries (most accessed first)
CREATE INDEX IF NOT EXISTS idx_recent_documents_access_count ON recent_documents(access_count DESC);
