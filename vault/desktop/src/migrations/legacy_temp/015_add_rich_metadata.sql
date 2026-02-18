-- Migration: Add rich metadata fields for enhanced search and analytics
-- Version: 015
-- Description: Adds language detection, categorization, quality scoring, access tracking, and content metrics to documents and text_chunks tables

-- Documents table: Add metadata fields
ALTER TABLE documents ADD COLUMN language TEXT DEFAULT 'unknown';
ALTER TABLE documents ADD COLUMN category TEXT DEFAULT 'uncategorized';
ALTER TABLE documents ADD COLUMN quality_score REAL DEFAULT 0.5 CHECK(quality_score >= 0.0 AND quality_score <= 1.0);
ALTER TABLE documents ADD COLUMN access_count INTEGER DEFAULT 0;
ALTER TABLE documents ADD COLUMN last_accessed_at INTEGER;
ALTER TABLE documents ADD COLUMN word_count INTEGER DEFAULT 0;

-- Text chunks table: Add metadata fields
ALTER TABLE text_chunks ADD COLUMN language TEXT DEFAULT 'unknown';
ALTER TABLE text_chunks ADD COLUMN token_count INTEGER DEFAULT 0;

-- Indexes for documents metadata queries
CREATE INDEX IF NOT EXISTS idx_documents_language ON documents(language);
CREATE INDEX IF NOT EXISTS idx_documents_category ON documents(category);
CREATE INDEX IF NOT EXISTS idx_documents_quality_score ON documents(quality_score DESC);
CREATE INDEX IF NOT EXISTS idx_documents_access_count ON documents(access_count DESC);
CREATE INDEX IF NOT EXISTS idx_documents_last_accessed ON documents(last_accessed_at DESC);

-- Composite indexes for common query patterns
CREATE INDEX IF NOT EXISTS idx_documents_category_quality ON documents(category, quality_score DESC);
CREATE INDEX IF NOT EXISTS idx_documents_language_category ON documents(language, category);

-- Note: reading_time_minutes already exists from migration 007 (web ingestion)
-- No need to add it again
