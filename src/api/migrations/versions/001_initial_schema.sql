-- Migration: 001_initial_schema
-- Description: Initial database schema for Vault with pgvector support
-- Created: 2025-11-09

-- ============================================================================
-- EXTENSIONS
-- ============================================================================

CREATE EXTENSION IF NOT EXISTS "uuid-ossp";
CREATE EXTENSION IF NOT EXISTS "vector";
CREATE EXTENSION IF NOT EXISTS "pg_trgm";

-- ============================================================================
-- FUNCTIONS
-- ============================================================================

-- Function to update ElectricSQL timestamp on row changes
CREATE OR REPLACE FUNCTION update_electric_timestamp()
RETURNS TRIGGER AS $$
BEGIN
    NEW.electric_last_modified = CURRENT_TIMESTAMP;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Function to update full-text search vector
CREATE OR REPLACE FUNCTION update_text_search_vector()
RETURNS TRIGGER AS $$
BEGIN
    NEW.search_vector := to_tsvector('english',
        COALESCE(NEW.content, '')
    );
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- ============================================================================
-- TABLES
-- ============================================================================

-- Watch Folders: Directories being monitored for files
CREATE TABLE watch_folders (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    path TEXT NOT NULL UNIQUE,
    recursive BOOLEAN NOT NULL DEFAULT true,
    active BOOLEAN NOT NULL DEFAULT true,
    last_scan_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    electric_user_id TEXT NOT NULL,
    electric_last_modified TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- Files: Core file metadata
CREATE TABLE files (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    path TEXT NOT NULL UNIQUE,
    filename TEXT NOT NULL,
    extension TEXT NOT NULL,
    size_bytes BIGINT NOT NULL,
    mime_type TEXT NOT NULL,
    hash_sha256 TEXT NOT NULL,
    watch_folder_id UUID NOT NULL REFERENCES watch_folders(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    modified_at TIMESTAMPTZ NOT NULL,
    indexed_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    last_accessed_at TIMESTAMPTZ,
    electric_user_id TEXT NOT NULL,
    electric_last_modified TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- Text Content: Extracted text from documents
CREATE TABLE text_content (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    file_id UUID NOT NULL UNIQUE REFERENCES files(id) ON DELETE CASCADE,
    content TEXT NOT NULL,
    word_count INTEGER NOT NULL,
    char_count INTEGER NOT NULL,
    language TEXT,
    search_vector TSVECTOR,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    electric_user_id TEXT NOT NULL,
    electric_last_modified TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- Text Embeddings: Vector embeddings for semantic search
CREATE TABLE text_embeddings (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    text_content_id UUID NOT NULL REFERENCES text_content(id) ON DELETE CASCADE,
    chunk_index INTEGER NOT NULL,
    chunk_text TEXT NOT NULL,
    embedding vector(768) NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    electric_user_id TEXT NOT NULL,
    electric_last_modified TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(text_content_id, chunk_index)
);

-- Images: Image-specific metadata
CREATE TABLE images (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    file_id UUID NOT NULL UNIQUE REFERENCES files(id) ON DELETE CASCADE,
    width INTEGER NOT NULL,
    height INTEGER NOT NULL,
    format TEXT NOT NULL,
    color_mode TEXT,
    has_transparency BOOLEAN NOT NULL DEFAULT false,
    exif_data JSONB,
    thumbnail_path TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    electric_user_id TEXT NOT NULL,
    electric_last_modified TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- Image Embeddings: CLIP embeddings for image similarity search
CREATE TABLE image_embeddings (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    image_id UUID NOT NULL UNIQUE REFERENCES images(id) ON DELETE CASCADE,
    embedding vector(512) NOT NULL,
    model_version TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    electric_user_id TEXT NOT NULL,
    electric_last_modified TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- Tags: User-defined labels for organization
CREATE TABLE tags (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT NOT NULL UNIQUE,
    color TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    electric_user_id TEXT NOT NULL,
    electric_last_modified TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- File Tags: Many-to-many relationship between files and tags
CREATE TABLE file_tags (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    file_id UUID NOT NULL REFERENCES files(id) ON DELETE CASCADE,
    tag_id UUID NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    electric_user_id TEXT NOT NULL,
    electric_last_modified TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(file_id, tag_id)
);

-- Search History: Track user searches for analytics and suggestions
CREATE TABLE search_history (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    query TEXT NOT NULL,
    search_type TEXT NOT NULL CHECK (search_type IN ('text', 'semantic', 'image', 'hybrid')),
    result_count INTEGER NOT NULL,
    execution_time_ms INTEGER NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    electric_user_id TEXT NOT NULL,
    electric_last_modified TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- ============================================================================
-- INDEXES
-- ============================================================================

-- Watch Folders indexes
CREATE INDEX idx_watch_folders_active ON watch_folders(active) WHERE active = true;
CREATE INDEX idx_watch_folders_path ON watch_folders(path);

-- Files indexes
CREATE INDEX idx_files_watch_folder ON files(watch_folder_id);
CREATE INDEX idx_files_extension ON files(extension);
CREATE INDEX idx_files_mime_type ON files(mime_type);
CREATE INDEX idx_files_hash ON files(hash_sha256);
CREATE INDEX idx_files_created_at ON files(created_at DESC);
CREATE INDEX idx_files_modified_at ON files(modified_at DESC);
CREATE INDEX idx_files_indexed_at ON files(indexed_at DESC);
CREATE INDEX idx_files_filename_trgm ON files USING GIN (filename gin_trgm_ops);

-- Text Content indexes
CREATE INDEX idx_text_content_file ON text_content(file_id);
CREATE INDEX idx_text_content_search_vector ON text_content USING GIN (search_vector);
CREATE INDEX idx_text_content_language ON text_content(language);

-- Text Embeddings indexes (HNSW for efficient approximate nearest neighbor search)
CREATE INDEX idx_text_embeddings_text_content ON text_embeddings(text_content_id);
CREATE INDEX idx_text_embeddings_vector ON text_embeddings
    USING hnsw (embedding vector_cosine_ops)
    WITH (m = 16, ef_construction = 64);

-- Images indexes
CREATE INDEX idx_images_file ON images(file_id);
CREATE INDEX idx_images_format ON images(format);
CREATE INDEX idx_images_dimensions ON images(width, height);
CREATE INDEX idx_images_exif ON images USING GIN (exif_data);

-- Image Embeddings indexes (HNSW for efficient approximate nearest neighbor search)
CREATE INDEX idx_image_embeddings_image ON image_embeddings(image_id);
CREATE INDEX idx_image_embeddings_vector ON image_embeddings
    USING hnsw (embedding vector_cosine_ops)
    WITH (m = 16, ef_construction = 64);

-- Tags indexes
CREATE INDEX idx_tags_name ON tags(name);
CREATE INDEX idx_tags_name_trgm ON tags USING GIN (name gin_trgm_ops);

-- File Tags indexes
CREATE INDEX idx_file_tags_file ON file_tags(file_id);
CREATE INDEX idx_file_tags_tag ON file_tags(tag_id);

-- Search History indexes
CREATE INDEX idx_search_history_created_at ON search_history(created_at DESC);
CREATE INDEX idx_search_history_search_type ON search_history(search_type);
CREATE INDEX idx_search_history_query_trgm ON search_history USING GIN (query gin_trgm_ops);

-- ============================================================================
-- TRIGGERS
-- ============================================================================

-- ElectricSQL timestamp triggers for all synced tables
CREATE TRIGGER trigger_watch_folders_electric_timestamp
    BEFORE UPDATE ON watch_folders
    FOR EACH ROW
    EXECUTE FUNCTION update_electric_timestamp();

CREATE TRIGGER trigger_files_electric_timestamp
    BEFORE UPDATE ON files
    FOR EACH ROW
    EXECUTE FUNCTION update_electric_timestamp();

CREATE TRIGGER trigger_text_content_electric_timestamp
    BEFORE UPDATE ON text_content
    FOR EACH ROW
    EXECUTE FUNCTION update_electric_timestamp();

CREATE TRIGGER trigger_text_embeddings_electric_timestamp
    BEFORE UPDATE ON text_embeddings
    FOR EACH ROW
    EXECUTE FUNCTION update_electric_timestamp();

CREATE TRIGGER trigger_images_electric_timestamp
    BEFORE UPDATE ON images
    FOR EACH ROW
    EXECUTE FUNCTION update_electric_timestamp();

CREATE TRIGGER trigger_image_embeddings_electric_timestamp
    BEFORE UPDATE ON image_embeddings
    FOR EACH ROW
    EXECUTE FUNCTION update_electric_timestamp();

CREATE TRIGGER trigger_tags_electric_timestamp
    BEFORE UPDATE ON tags
    FOR EACH ROW
    EXECUTE FUNCTION update_electric_timestamp();

CREATE TRIGGER trigger_file_tags_electric_timestamp
    BEFORE UPDATE ON file_tags
    FOR EACH ROW
    EXECUTE FUNCTION update_electric_timestamp();

CREATE TRIGGER trigger_search_history_electric_timestamp
    BEFORE UPDATE ON search_history
    FOR EACH ROW
    EXECUTE FUNCTION update_electric_timestamp();

-- Full-text search vector update trigger
CREATE TRIGGER trigger_text_content_search_vector
    BEFORE INSERT OR UPDATE ON text_content
    FOR EACH ROW
    EXECUTE FUNCTION update_text_search_vector();

-- ============================================================================
-- COMMENTS
-- ============================================================================

COMMENT ON EXTENSION vector IS 'pgvector extension for vector similarity search';
COMMENT ON EXTENSION pg_trgm IS 'Trigram matching for fuzzy text search';

COMMENT ON TABLE watch_folders IS 'Directories being monitored for file indexing';
COMMENT ON TABLE files IS 'Core file metadata for all indexed files';
COMMENT ON TABLE text_content IS 'Extracted text content from documents';
COMMENT ON TABLE text_embeddings IS 'Semantic embeddings for text chunks (BAAI/bge-base-en-v1.5, 768 dimensions)';
COMMENT ON TABLE images IS 'Image-specific metadata and EXIF data';
COMMENT ON TABLE image_embeddings IS 'CLIP embeddings for images (512 dimensions)';
COMMENT ON TABLE tags IS 'User-defined tags for file organization';
COMMENT ON TABLE file_tags IS 'Many-to-many relationship between files and tags';
COMMENT ON TABLE search_history IS 'Search query history for analytics and autocomplete';

COMMENT ON COLUMN text_embeddings.embedding IS 'BAAI/bge-base-en-v1.5 embedding (768 dimensions)';
COMMENT ON COLUMN image_embeddings.embedding IS 'CLIP ViT-B/32 embedding (512 dimensions)';
COMMENT ON COLUMN text_content.search_vector IS 'Generated tsvector for full-text search';

-- ============================================================================
-- INITIAL DATA
-- ============================================================================

-- No initial data required for base schema
