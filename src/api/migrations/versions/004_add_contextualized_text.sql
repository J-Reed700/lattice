-- Migration: 004_add_contextualized_text
-- Description: Add contextualized_text column to text_embeddings and context_metadata to image_embeddings
-- Reference: Contextual Retrieval Implementation (Anthropic's 49% accuracy improvement)
-- Created: 2025-11-10

-- ============================================================================
-- TEXT EMBEDDINGS: Add Contextualized Text Column
-- ============================================================================

-- Add contextualized_text column to store chunk text with metadata context prepended
ALTER TABLE text_embeddings
ADD COLUMN contextualized_text TEXT;

-- ============================================================================
-- IMAGE EMBEDDINGS: Add Context Metadata Column
-- ============================================================================

-- Add context_metadata JSONB column to store metadata used for contextual retrieval
ALTER TABLE image_embeddings
ADD COLUMN context_metadata JSONB;

-- ============================================================================
-- INDEXES: Performance Optimization
-- ============================================================================

-- Index on contextualized_text for full-text search (if needed)
CREATE INDEX idx_text_embeddings_contextualized_text
ON text_embeddings USING gin(to_tsvector('english', contextualized_text))
WHERE contextualized_text IS NOT NULL;

-- Index on context_metadata for JSON queries
CREATE INDEX idx_image_embeddings_context_metadata
ON image_embeddings USING gin(context_metadata)
WHERE context_metadata IS NOT NULL;

-- ============================================================================
-- COMMENTS: Documentation
-- ============================================================================

COMMENT ON COLUMN text_embeddings.contextualized_text IS 'Original chunk text with metadata context prepended for improved embedding accuracy (Anthropic Contextual Retrieval)';

COMMENT ON COLUMN image_embeddings.context_metadata IS 'Metadata context (file_name, file_path, etc.) stored as JSON for contextual image retrieval';

-- ============================================================================
-- NOTES
-- ============================================================================

-- This migration implements Anthropic's Contextual Retrieval approach:
-- - Prepends metadata context to chunks before embedding
-- - Results in 49% reduction in failed retrievals
-- - 67% improvement when combined with hybrid search + reranking
--
-- Context format for text:
--   "Document: {title}. File: {file_name}. Source: {file_path}. Author: {author}. Pages: {page_count}. Chunk {chunk_idx}.
--
--    {original_chunk_text}"
--
-- Context metadata for images (stored as JSONB):
--   {
--     "file_name": "image.jpg",
--     "file_path": "/path/to/image.jpg",
--     "title": "Image Title",
--     "width": 1920,
--     "height": 1080,
--     "format": "JPEG"
--   }
