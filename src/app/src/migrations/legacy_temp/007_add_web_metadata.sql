-- Add web-specific metadata fields to documents table
-- Migration 007: Web URL Ingestion Support

-- Add source_type column to distinguish local files from web documents
ALTER TABLE documents ADD COLUMN source_type TEXT DEFAULT 'local' CHECK(source_type IN ('local', 'web'));

-- Web metadata fields
ALTER TABLE documents ADD COLUMN canonical_url TEXT;
ALTER TABLE documents ADD COLUMN site_name TEXT;
ALTER TABLE documents ADD COLUMN favicon_url TEXT;
ALTER TABLE documents ADD COLUMN reading_time_minutes INTEGER;
ALTER TABLE documents ADD COLUMN web_author TEXT;
ALTER TABLE documents ADD COLUMN published_at TEXT;
ALTER TABLE documents ADD COLUMN web_description TEXT;
ALTER TABLE documents ADD COLUMN web_keywords TEXT;
ALTER TABLE documents ADD COLUMN web_language TEXT;

-- Create index on source_type for filtering
CREATE INDEX IF NOT EXISTS idx_documents_source_type ON documents(source_type);

-- Create index on canonical_url for web document lookups
CREATE INDEX IF NOT EXISTS idx_documents_canonical_url ON documents(canonical_url) WHERE canonical_url IS NOT NULL;

-- Create composite index for web documents
CREATE INDEX IF NOT EXISTS idx_documents_web ON documents(source_type, indexed_at DESC) WHERE source_type = 'web';

-- Update existing documents to have source_type = 'local'
UPDATE documents SET source_type = 'local' WHERE source_type IS NULL;
