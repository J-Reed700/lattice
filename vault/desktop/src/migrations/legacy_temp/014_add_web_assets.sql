-- Migration: Add web assets table
-- Version: 014
-- Description: Creates table for tracking web assets (images, stylesheets, etc.) associated with documents

-- Web assets table: stores metadata about downloaded web assets
CREATE TABLE IF NOT EXISTS web_assets (
    id TEXT PRIMARY KEY,
    document_id TEXT NOT NULL,
    url TEXT NOT NULL,
    local_path TEXT NOT NULL,
    content_type TEXT NOT NULL,
    size_bytes INTEGER NOT NULL,
    checksum TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (document_id) REFERENCES documents(id) ON DELETE CASCADE
) STRICT;

-- Indexes for web assets queries
CREATE INDEX IF NOT EXISTS idx_web_assets_document_id ON web_assets(document_id);
CREATE INDEX IF NOT EXISTS idx_web_assets_checksum ON web_assets(checksum);
CREATE INDEX IF NOT EXISTS idx_web_assets_url ON web_assets(url);
