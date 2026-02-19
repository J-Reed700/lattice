-- Migration: 003_contextual_retrieval
-- Description: Add contextual retrieval support for 49% accuracy improvement
-- Reference: PHASE1_ARCHITECTURE_REVIEW.md lines 66-71
-- Created: 2025-11-10

-- ============================================================================
-- TEXT EMBEDDINGS: Add Contextual Retrieval Columns
-- ============================================================================

-- contextualized_content: Full chunk text with context prefix (used for embedding)
ALTER TABLE text_embeddings ADD COLUMN contextualized_content TEXT;

-- context_prefix: Just the metadata prefix (for debugging/display)
ALTER TABLE text_embeddings ADD COLUMN context_prefix TEXT;

-- page_number: Page number in source document (NULL for non-PDFs)
ALTER TABLE text_embeddings ADD COLUMN page_number INTEGER;

-- section_title: Section/heading this chunk belongs to
ALTER TABLE text_embeddings ADD COLUMN section_title TEXT;

-- ============================================================================
-- FILES: Add Document Metadata for Context Generation
-- ============================================================================

-- page_count: Total pages in document (NULL for non-PDFs)
ALTER TABLE files ADD COLUMN page_count INTEGER;

-- title: Document title extracted from metadata or filename
ALTER TABLE files ADD COLUMN title TEXT;

-- author: Document author from metadata
ALTER TABLE files ADD COLUMN author TEXT;

-- ============================================================================
-- INDEXES: Performance Optimization
-- ============================================================================

-- Index on contextualized_content for hybrid search
CREATE INDEX idx_text_embeddings_contextualized 
ON text_embeddings(contextualized_content) 
WHERE contextualized_content IS NOT NULL;

-- Index on page_number for filtering
CREATE INDEX idx_text_embeddings_page 
ON text_embeddings(page_number) 
WHERE page_number IS NOT NULL;

-- Index on section_title for filtering
CREATE INDEX idx_text_embeddings_section 
ON text_embeddings(section_title) 
WHERE section_title IS NOT NULL;

-- Index on document title for search
CREATE INDEX idx_files_title 
ON files(title) 
WHERE title IS NOT NULL;

-- Index on author for filtering
CREATE INDEX idx_files_author 
ON files(author) 
WHERE author IS NOT NULL;

-- ============================================================================
-- COMMENTS: Documentation
-- ============================================================================

COMMENT ON COLUMN text_embeddings.contextualized_content IS 'Chunk text with document context prefix for improved embedding quality';
COMMENT ON COLUMN text_embeddings.context_prefix IS 'Just the metadata prefix: "Document: X | Page: Y | Section: Z."';
COMMENT ON COLUMN text_embeddings.page_number IS 'Page number in source document (NULL for non-PDFs)';
COMMENT ON COLUMN text_embeddings.section_title IS 'Section or heading this chunk belongs to';
COMMENT ON COLUMN files.page_count IS 'Total number of pages (NULL for non-PDFs)';
COMMENT ON COLUMN files.title IS 'Document title from metadata or filename';
COMMENT ON COLUMN files.author IS 'Document author from metadata';
