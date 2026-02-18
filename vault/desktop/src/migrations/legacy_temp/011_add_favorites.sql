-- Migration: Add favorites table
-- Version: 011
-- Description: Creates table for managing favorite documents

CREATE TABLE IF NOT EXISTS favorites (
    id TEXT PRIMARY KEY NOT NULL,
    document_id TEXT NOT NULL UNIQUE,
    added_at TEXT NOT NULL,
    FOREIGN KEY (document_id) REFERENCES documents(id) ON DELETE CASCADE
) STRICT;

-- Index for chronological queries (most recent first)
CREATE INDEX IF NOT EXISTS idx_favorites_added_at ON favorites(added_at DESC);

-- Index for document lookups
CREATE INDEX IF NOT EXISTS idx_favorites_document_id ON favorites(document_id);
