-- Migration: 003_enable_image_chunking
-- Description: Enable chunk-level indexing for image embeddings
-- Created: 2025-11-10
--
-- This migration enables storing multiple embeddings per image, allowing:
-- - Multi-region image embeddings (crops/patches)
-- - Multiple model versions per image
-- - Temporal embeddings for video frames
--
-- IMPORTANT: This migration:
-- - Drops and recreates constraints (acquires ACCESS EXCLUSIVE lock)
-- - Should be fast (no data rewrite) unless there are many existing rows
-- - Check for existing duplicate image_id values before running

-- ============================================================================
-- FORWARD MIGRATION
-- ============================================================================

-- Drop the UNIQUE constraint on image_id
-- This allows multiple embeddings per image
ALTER TABLE image_embeddings 
    DROP CONSTRAINT IF EXISTS image_embeddings_image_id_key;

-- Add chunk_index column if it doesn't exist
-- NULL for existing single embeddings (backward compatible)
DO $$ 
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'image_embeddings' 
        AND column_name = 'chunk_index'
    ) THEN
        ALTER TABLE image_embeddings 
            ADD COLUMN chunk_index INTEGER;
    END IF;
END $$;

-- Set default chunk_index=0 for existing rows where it's NULL
UPDATE image_embeddings 
    SET chunk_index = 0 
    WHERE chunk_index IS NULL;

-- Make chunk_index NOT NULL after backfilling
ALTER TABLE image_embeddings 
    ALTER COLUMN chunk_index SET NOT NULL;

-- Add composite unique constraint to prevent duplicate chunks
ALTER TABLE image_embeddings 
    ADD CONSTRAINT image_embeddings_image_chunk_unique 
    UNIQUE (image_id, chunk_index);

-- Create composite index for efficient lookups by image
-- This replaces the need for the old idx_image_embeddings_image
DROP INDEX IF EXISTS idx_image_embeddings_image;
CREATE INDEX idx_image_embeddings_image_chunk ON image_embeddings(image_id, chunk_index);

-- Update table comment
COMMENT ON TABLE image_embeddings IS 
    'CLIP embeddings for images with support for multiple chunks per image (512 dimensions)';
COMMENT ON COLUMN image_embeddings.chunk_index IS 
    'Chunk index for multi-region embeddings (0 for full image, 1+ for regions/patches)';

-- ============================================================================
-- ROLLBACK MIGRATION
-- ============================================================================
-- WARNING: Rollback will FAIL if multiple chunks exist for any image_id
-- You must manually remove duplicate chunks before rolling back
--
-- To rollback, run:
--
-- -- Remove composite constraint and index
-- ALTER TABLE image_embeddings DROP CONSTRAINT IF EXISTS image_embeddings_image_chunk_unique;
-- DROP INDEX IF EXISTS idx_image_embeddings_image_chunk;
--
-- -- Recreate original UNIQUE constraint (will fail if duplicates exist)
-- ALTER TABLE image_embeddings ADD CONSTRAINT image_embeddings_image_id_key UNIQUE (image_id);
--
-- -- Recreate original index
-- CREATE INDEX idx_image_embeddings_image ON image_embeddings(image_id);
--
-- -- Optionally remove chunk_index column
-- -- ALTER TABLE image_embeddings DROP COLUMN chunk_index;
