-- Migration: Add mentions and document_mentions tables
-- Version: 010
-- Description: Creates tables for storing mentions (@person and [[wikilink]] style) and linking them to documents

CREATE TABLE IF NOT EXISTS mentions (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    type TEXT NOT NULL,
    metadata TEXT,
    created_at TEXT NOT NULL
) STRICT;

-- Unique constraint on mention name
CREATE UNIQUE INDEX IF NOT EXISTS idx_mentions_name ON mentions(name);

-- Index for type-based queries
CREATE INDEX IF NOT EXISTS idx_mentions_type ON mentions(type);

-- Join table linking documents to mentions with context
CREATE TABLE IF NOT EXISTS document_mentions (
    id TEXT PRIMARY KEY NOT NULL,
    document_id TEXT NOT NULL,
    mention_id TEXT NOT NULL,
    context TEXT,
    position INTEGER,
    created_at TEXT NOT NULL,
    FOREIGN KEY (mention_id) REFERENCES mentions(id) ON DELETE CASCADE
) STRICT;

-- Index for finding mentions in a document
CREATE INDEX IF NOT EXISTS idx_document_mentions_doc ON document_mentions(document_id);

-- Index for finding documents mentioning a specific mention (backlinks)
CREATE INDEX IF NOT EXISTS idx_document_mentions_mention ON document_mentions(mention_id);

-- Composite index for efficient JOIN queries
CREATE INDEX IF NOT EXISTS idx_document_mentions_composite ON document_mentions(document_id, mention_id);
