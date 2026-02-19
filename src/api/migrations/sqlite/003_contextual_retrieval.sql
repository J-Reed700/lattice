-- ============================================================================
-- Migration: 003_contextual_retrieval
-- Description: Add contextual retrieval support for 49% accuracy improvement
-- Reference: PHASE1_ARCHITECTURE_REVIEW.md lines 66-71
-- Created: 2025-11-10
-- ============================================================================

-- ============================================================================
-- EMBEDDINGS TABLE: Add Contextual Retrieval Columns
-- ============================================================================

-- contextualized_content: Full chunk text with context prefix (used for embedding)
-- Example: "Document: Financial Report Q3, Page: 12, Section: Revenue Analysis. [original chunk text]"
ALTER TABLE embeddings ADD COLUMN contextualized_content TEXT;

-- context_prefix: Just the metadata prefix (for debugging/display)
-- Example: "Document: Financial Report Q3, Page: 12, Section: Revenue Analysis."
ALTER TABLE embeddings ADD COLUMN context_prefix TEXT;

-- page_number: Page number in source document (NULL for non-PDFs)
ALTER TABLE embeddings ADD COLUMN page_number INTEGER;

-- section_title: Section/heading this chunk belongs to (NULL if not detected)
ALTER TABLE embeddings ADD COLUMN section_title TEXT;

-- ============================================================================
-- DOCUMENTS TABLE: Add Document Metadata for Context Generation
-- ============================================================================

-- page_count: Total pages in document (NULL for non-PDFs)
ALTER TABLE documents ADD COLUMN page_count INTEGER;

-- title: Document title extracted from metadata or filename
ALTER TABLE documents ADD COLUMN title TEXT;

-- author: Document author from metadata
ALTER TABLE documents ADD COLUMN author TEXT;

-- ============================================================================
-- INDEXES: Performance Optimization
-- ============================================================================

-- Index on contextualized_content for hybrid search patterns
CREATE INDEX idx_embeddings_contextualized ON embeddings(contextualized_content) 
    WHERE contextualized_content IS NOT NULL;

-- Index on page_number for filtering by page
CREATE INDEX idx_embeddings_page ON embeddings(page_number) 
    WHERE page_number IS NOT NULL;

-- Index on section_title for filtering by section
CREATE INDEX idx_embeddings_section ON embeddings(section_title) 
    WHERE section_title IS NOT NULL;

-- Index on document title for search
CREATE INDEX idx_documents_title ON documents(title) 
    WHERE title IS NOT NULL;

-- ============================================================================
-- FTS5 UPDATE: Add Contextualized Content to Full-Text Search
-- ============================================================================

-- Recreate fts_chunks to include contextualized_content
DROP TRIGGER IF EXISTS trigger_fts_chunks_insert;
DROP TRIGGER IF EXISTS trigger_fts_chunks_delete;

DROP TABLE IF EXISTS fts_chunks;

CREATE VIRTUAL TABLE fts_chunks USING fts5(
    embedding_id UNINDEXED,
    chunk_text,
    contextualized_content,
    section_title,
    tokenize = 'porter unicode61'
);

-- Recreate triggers for new schema
CREATE TRIGGER trigger_fts_chunks_insert
    AFTER INSERT ON embeddings
    FOR EACH ROW
    WHEN NEW.chunk_text IS NOT NULL
BEGIN
    INSERT INTO fts_chunks(embedding_id, chunk_text, contextualized_content, section_title)
    VALUES (
        NEW.id, 
        NEW.chunk_text, 
        COALESCE(NEW.contextualized_content, NEW.chunk_text),
        COALESCE(NEW.section_title, '')
    );
END;

CREATE TRIGGER trigger_fts_chunks_update
    AFTER UPDATE ON embeddings
    FOR EACH ROW
    WHEN NEW.chunk_text IS NOT NULL
BEGIN
    UPDATE fts_chunks
    SET chunk_text = NEW.chunk_text,
        contextualized_content = COALESCE(NEW.contextualized_content, NEW.chunk_text),
        section_title = COALESCE(NEW.section_title, '')
    WHERE embedding_id = NEW.id;
END;

CREATE TRIGGER trigger_fts_chunks_delete
    AFTER DELETE ON embeddings
    FOR EACH ROW
BEGIN
    DELETE FROM fts_chunks WHERE embedding_id = OLD.id;
END;

-- ============================================================================
-- VIEWS: Updated Convenience Queries
-- ============================================================================

-- View for chunks with contextual metadata
CREATE VIEW v_embeddings_with_context AS
SELECT 
    e.id,
    e.document_id,
    e.chunk_index,
    e.chunk_text,
    e.contextualized_content,
    e.context_prefix,
    e.page_number,
    e.section_title,
    e.topic_id,
    d.filename,
    d.title as document_title,
    d.author as document_author,
    d.page_count as document_page_count
FROM embeddings e
JOIN documents d ON e.document_id = d.id
WHERE e.embedding_type = 'text';

-- ============================================================================
-- DATA MIGRATION: Backfill Strategy (Optional)
-- ============================================================================

-- NOTE: This migration does NOT backfill existing data to avoid long-running operations
-- during schema migration. To enable contextual retrieval on existing documents:
--
-- Option 1: Trigger re-indexing (RECOMMENDED)
--   UPDATE documents SET index_status = 'pending' WHERE index_status = 'indexed';
--
-- Option 2: Backfill with basic context (NOT RECOMMENDED - loses accuracy benefit)
--   UPDATE embeddings SET contextualized_content = chunk_text WHERE contextualized_content IS NULL;
--   UPDATE embeddings SET context_prefix = '' WHERE context_prefix IS NULL;

-- ============================================================================
-- MIGRATION COMPLETE
-- ============================================================================

-- Record migration in schema_version table (if exists)
INSERT OR IGNORE INTO schema_version (version, applied_at, description)
VALUES (3, unixepoch(), 'Add contextual retrieval columns for 49% accuracy improvement');
