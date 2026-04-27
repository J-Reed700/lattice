-- Add missing chunk metadata columns to text_chunks table
-- These columns exist in ChunkModel but were never added to the database schema

ALTER TABLE text_chunks ADD COLUMN word_count INTEGER DEFAULT 0;
ALTER TABLE text_chunks ADD COLUMN has_code INTEGER DEFAULT 0;
ALTER TABLE text_chunks ADD COLUMN section TEXT;
ALTER TABLE text_chunks ADD COLUMN language TEXT DEFAULT 'unknown';
ALTER TABLE text_chunks ADD COLUMN token_count INTEGER DEFAULT 0;
