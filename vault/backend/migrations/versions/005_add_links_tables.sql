-- Migration: 005_add_links_tables
-- Description: Add tables for bidirectional wikilinks support
-- Created: 2025-11-10

-- ============================================================================
-- LINKS TABLE
-- ============================================================================

-- Links: Wikilinks between documents
CREATE TABLE IF NOT EXISTS links (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL,
    source_document_id UUID NOT NULL,
    target_document_id UUID,
    target_text TEXT NOT NULL,
    display_text TEXT,
    header TEXT,
    line_number INTEGER,
    context TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    electric_user_id TEXT,
    electric_last_modified TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (source_document_id) REFERENCES files(id) ON DELETE CASCADE,
    FOREIGN KEY (target_document_id) REFERENCES files(id) ON DELETE SET NULL
);

-- Indexes for efficient querying
CREATE INDEX IF NOT EXISTS idx_links_source ON links(source_document_id);
CREATE INDEX IF NOT EXISTS idx_links_target ON links(target_document_id);
CREATE INDEX IF NOT EXISTS idx_links_user ON links(user_id);
CREATE INDEX IF NOT EXISTS idx_links_target_text ON links(target_text);
CREATE INDEX IF NOT EXISTS idx_links_created_at ON links(created_at);

-- Composite indexes for common queries
CREATE INDEX IF NOT EXISTS idx_links_user_source ON links(user_id, source_document_id);
CREATE INDEX IF NOT EXISTS idx_links_user_target ON links(user_id, target_document_id);

-- Trigger to update electric_last_modified on changes
CREATE TRIGGER update_links_electric_timestamp
    BEFORE UPDATE ON links
    FOR EACH ROW
    EXECUTE FUNCTION update_electric_timestamp();

-- ============================================================================
-- COMMENTS
-- ============================================================================

COMMENT ON TABLE links IS 'Bidirectional wikilinks between documents';
COMMENT ON COLUMN links.source_document_id IS 'Document containing the wikilink';
COMMENT ON COLUMN links.target_document_id IS 'Document being linked to (NULL if target does not exist yet)';
COMMENT ON COLUMN links.target_text IS 'Raw link text as it appears in [[brackets]]';
COMMENT ON COLUMN links.display_text IS 'Custom display text from [[target|display]]';
COMMENT ON COLUMN links.header IS 'Header anchor from [[target#header]]';
COMMENT ON COLUMN links.line_number IS 'Line number where link appears in source document';
COMMENT ON COLUMN links.context IS 'Surrounding text for preview (50 chars before/after)';
