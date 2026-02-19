-- Migration: 002_optimize_hnsw_indexes
-- Description: Optimize HNSW index parameters for better search performance
-- Created: 2025-11-10
-- 
-- IMPORTANT: This migration requires REINDEX operations which will:
-- - Acquire SHARE UPDATE EXCLUSIVE locks on tables (blocks writes)
-- - Take significant time proportional to table size
-- - Recommend running during maintenance window
--
-- Performance Impact:
-- - Text embeddings (768-dim): m=24, ef_construction=200
--   * Better recall for high-dimensional data
--   * Slightly larger index size (~50% increase in memory usage)
--   * Improved search quality for complex semantic queries
-- 
-- - Image embeddings (512-dim): ef_construction=200
--   * Maintains m=16 (appropriate for medium dimensions)
--   * Better index quality without excessive memory overhead
--
-- Expected Build Time:
-- - ~1-2 minutes per 100k text embeddings
-- - ~45-90 seconds per 100k image embeddings
-- - Scale linearly with data size

-- ============================================================================
-- FORWARD MIGRATION
-- ============================================================================

-- Drop existing HNSW indexes
DROP INDEX IF EXISTS idx_text_embeddings_vector;
DROP INDEX IF EXISTS idx_image_embeddings_vector;

-- Recreate text embeddings HNSW index with optimized parameters
-- m=24: Better for high-dimensional data (768-dim)
-- ef_construction=200: Higher quality index, better recall
CREATE INDEX idx_text_embeddings_vector ON text_embeddings
    USING hnsw (embedding vector_cosine_ops)
    WITH (m = 24, ef_construction = 200);

-- Recreate image embeddings HNSW index with optimized parameters
-- m=16: Sufficient for medium-dimensional data (512-dim)
-- ef_construction=200: Higher quality index, better recall
CREATE INDEX idx_image_embeddings_vector ON image_embeddings
    USING hnsw (embedding vector_cosine_ops)
    WITH (m = 16, ef_construction = 200);

-- Update comments to reflect new parameters
COMMENT ON INDEX idx_text_embeddings_vector IS 
    'HNSW index for text embeddings with m=24, ef_construction=200 for optimal 768-dim search';
COMMENT ON INDEX idx_image_embeddings_vector IS 
    'HNSW index for image embeddings with m=16, ef_construction=200 for optimal 512-dim search';

-- ============================================================================
-- ROLLBACK MIGRATION
-- ============================================================================
-- To rollback, run:
--
-- DROP INDEX IF EXISTS idx_text_embeddings_vector;
-- DROP INDEX IF EXISTS idx_image_embeddings_vector;
--
-- CREATE INDEX idx_text_embeddings_vector ON text_embeddings
--     USING hnsw (embedding vector_cosine_ops)
--     WITH (m = 16, ef_construction = 64);
--
-- CREATE INDEX idx_image_embeddings_vector ON image_embeddings
--     USING hnsw (embedding vector_cosine_ops)
--     WITH (m = 16, ef_construction = 64);
