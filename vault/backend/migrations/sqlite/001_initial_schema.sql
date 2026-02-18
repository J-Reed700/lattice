-- ============================================================================
-- Migration: 001_initial_schema
-- Description: Initial SQLite schema for Vault with topic discovery
-- Database: SQLite 3.45+ with sqlite-vec extension
-- Created: 2025-11-10
-- ============================================================================

-- ============================================================================
-- CORE TABLES
-- ============================================================================

-- Documents: Core file metadata and content storage
CREATE TABLE documents (
    id TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
    path TEXT NOT NULL UNIQUE,
    filename TEXT NOT NULL,
    extension TEXT NOT NULL,
    mime_type TEXT NOT NULL,
    size_bytes INTEGER NOT NULL,
    hash_sha256 TEXT NOT NULL,
    
    -- Content fields (flexible JSON for early schema)
    content TEXT,
    metadata_json TEXT DEFAULT '{}',
    
    -- Stats
    word_count INTEGER DEFAULT 0,
    char_count INTEGER DEFAULT 0,
    
    -- Topic assignment
    topic_id TEXT REFERENCES topics(id) ON DELETE SET NULL,
    
    -- Timestamps
    file_created_at INTEGER NOT NULL,
    file_modified_at INTEGER NOT NULL,
    indexed_at INTEGER NOT NULL DEFAULT (unixepoch()),
    last_accessed_at INTEGER,
    
    -- Index status tracking
    index_status TEXT NOT NULL DEFAULT 'pending' 
        CHECK (index_status IN ('pending', 'processing', 'indexed', 'failed')),
    index_error TEXT,
    
    created_at INTEGER NOT NULL DEFAULT (unixepoch()),
    updated_at INTEGER NOT NULL DEFAULT (unixepoch())
) STRICT;

-- File Metadata: Track file changes for incremental indexing
CREATE TABLE file_metadata (
    id TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
    document_id TEXT NOT NULL UNIQUE REFERENCES documents(id) ON DELETE CASCADE,
    
    -- Change detection
    last_hash TEXT NOT NULL,
    last_size INTEGER NOT NULL,
    last_modified INTEGER NOT NULL,
    
    -- Sync tracking
    scan_count INTEGER NOT NULL DEFAULT 1,
    last_scan_at INTEGER NOT NULL DEFAULT (unixepoch()),
    needs_reindex INTEGER NOT NULL DEFAULT 0,
    
    created_at INTEGER NOT NULL DEFAULT (unixepoch()),
    updated_at INTEGER NOT NULL DEFAULT (unixepoch())
) STRICT;

-- Topics: Auto-discovered topics from clustering
CREATE TABLE topics (
    id TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
    name TEXT NOT NULL UNIQUE,
    
    -- Cluster metadata
    centroid_blob BLOB NOT NULL,
    keywords_json TEXT NOT NULL DEFAULT '[]',
    document_count INTEGER NOT NULL DEFAULT 0,
    
    -- Quality metrics
    silhouette_score REAL,
    inertia REAL,
    
    -- Topic metadata
    description TEXT,
    color TEXT,
    
    -- Timestamps
    created_at INTEGER NOT NULL DEFAULT (unixepoch()),
    updated_at INTEGER NOT NULL DEFAULT (unixepoch()),
    last_clustered_at INTEGER
) STRICT;

-- Embeddings: Store vector embeddings for all modalities
CREATE TABLE embeddings (
    id TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
    document_id TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    
    -- Embedding metadata
    embedding_type TEXT NOT NULL CHECK (embedding_type IN ('text', 'image', 'audio')),
    chunk_index INTEGER NOT NULL DEFAULT 0,
    chunk_text TEXT,
    
    -- Model info
    model_name TEXT NOT NULL,
    model_version TEXT NOT NULL,
    dimension INTEGER NOT NULL,
    
    -- Topic reference
    topic_id TEXT REFERENCES topics(id) ON DELETE SET NULL,
    
    created_at INTEGER NOT NULL DEFAULT (unixepoch()),
    
    UNIQUE(document_id, embedding_type, chunk_index)
) STRICT;

-- Watch Folders: Directories being monitored
CREATE TABLE watch_folders (
    id TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
    path TEXT NOT NULL UNIQUE,
    recursive INTEGER NOT NULL DEFAULT 1,
    active INTEGER NOT NULL DEFAULT 1,
    
    -- Patterns (stored as JSON arrays)
    include_patterns_json TEXT DEFAULT '["*"]',
    exclude_patterns_json TEXT DEFAULT '[]',
    
    -- Stats
    total_files INTEGER DEFAULT 0,
    indexed_files INTEGER DEFAULT 0,
    failed_files INTEGER DEFAULT 0,
    
    last_scan_at INTEGER,
    created_at INTEGER NOT NULL DEFAULT (unixepoch()),
    updated_at INTEGER NOT NULL DEFAULT (unixepoch())
) STRICT;

-- Tags: User-defined labels
CREATE TABLE tags (
    id TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
    name TEXT NOT NULL UNIQUE,
    color TEXT,
    description TEXT,
    file_count INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL DEFAULT (unixepoch())
) STRICT;

-- Document Tags: Many-to-many
CREATE TABLE document_tags (
    id TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
    document_id TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    tag_id TEXT NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
    created_at INTEGER NOT NULL DEFAULT (unixepoch()),
    UNIQUE(document_id, tag_id)
) STRICT;

-- Search History: Track searches for analytics
CREATE TABLE search_history (
    id TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
    query TEXT NOT NULL,
    search_type TEXT NOT NULL CHECK (search_type IN ('text', 'semantic', 'image', 'hybrid')),
    result_count INTEGER NOT NULL,
    execution_time_ms INTEGER NOT NULL,
    filters_json TEXT DEFAULT '{}',
    created_at INTEGER NOT NULL DEFAULT (unixepoch())
) STRICT;

-- ============================================================================
-- SQLITE-VEC VIRTUAL TABLES (Vector Search)
-- ============================================================================

-- Text embeddings vector index (768 dimensions)
CREATE VIRTUAL TABLE vec_text_embeddings USING vec0(
    embedding_id TEXT PRIMARY KEY,
    embedding FLOAT[768]
);

-- Image embeddings vector index (768 dimensions)
CREATE VIRTUAL TABLE vec_image_embeddings USING vec0(
    embedding_id TEXT PRIMARY KEY,
    embedding FLOAT[768]
);

-- Audio embeddings vector index (768 dimensions)  
CREATE VIRTUAL TABLE vec_audio_embeddings USING vec0(
    embedding_id TEXT PRIMARY KEY,
    embedding FLOAT[768]
);

-- Topic centroids vector index
CREATE VIRTUAL TABLE vec_topic_centroids USING vec0(
    topic_id TEXT PRIMARY KEY,
    centroid FLOAT[768]
);

-- ============================================================================
-- FTS5 TABLES (Full-Text Search)
-- ============================================================================

-- Full-text search on document content
CREATE VIRTUAL TABLE fts_documents USING fts5(
    document_id UNINDEXED,
    filename,
    content,
    metadata,
    tokenize = 'porter unicode61'
);

-- Full-text search on chunk text
CREATE VIRTUAL TABLE fts_chunks USING fts5(
    embedding_id UNINDEXED,
    chunk_text,
    tokenize = 'porter unicode61'
);

-- Full-text search on topics
CREATE VIRTUAL TABLE fts_topics USING fts5(
    topic_id UNINDEXED,
    name,
    keywords,
    description,
    tokenize = 'porter unicode61'
);

-- ============================================================================
-- INDEXES (Performance Critical)
-- ============================================================================

-- Documents indexes
CREATE INDEX idx_documents_path ON documents(path);
CREATE INDEX idx_documents_topic ON documents(topic_id) WHERE topic_id IS NOT NULL;
CREATE INDEX idx_documents_status ON documents(index_status);
CREATE INDEX idx_documents_extension ON documents(extension);
CREATE INDEX idx_documents_mime_type ON documents(mime_type);
CREATE INDEX idx_documents_hash ON documents(hash_sha256);
CREATE INDEX idx_documents_indexed_at ON documents(indexed_at DESC);
CREATE INDEX idx_documents_modified_at ON documents(file_modified_at DESC);
CREATE INDEX idx_documents_needs_reindex ON documents(id) 
    WHERE index_status = 'pending' OR index_status = 'failed';

-- Embeddings indexes
CREATE INDEX idx_embeddings_document ON embeddings(document_id);
CREATE INDEX idx_embeddings_type ON embeddings(embedding_type);
CREATE INDEX idx_embeddings_topic ON embeddings(topic_id) WHERE topic_id IS NOT NULL;
CREATE INDEX idx_embeddings_lookup ON embeddings(document_id, embedding_type, chunk_index);

-- Topics indexes
CREATE INDEX idx_topics_name ON topics(name);
CREATE INDEX idx_topics_doc_count ON topics(document_count DESC);
CREATE INDEX idx_topics_updated ON topics(updated_at DESC);
CREATE INDEX idx_topics_quality ON topics(silhouette_score DESC) WHERE silhouette_score IS NOT NULL;

-- File metadata indexes
CREATE INDEX idx_file_metadata_document ON file_metadata(document_id);
CREATE INDEX idx_file_metadata_needs_reindex ON file_metadata(document_id) WHERE needs_reindex = 1;

-- Watch folders indexes
CREATE INDEX idx_watch_folders_active ON watch_folders(active) WHERE active = 1;

-- Tags indexes
CREATE INDEX idx_tags_name ON tags(name);
CREATE INDEX idx_document_tags_document ON document_tags(document_id);
CREATE INDEX idx_document_tags_tag ON document_tags(tag_id);

-- Search history indexes
CREATE INDEX idx_search_history_created ON search_history(created_at DESC);
CREATE INDEX idx_search_history_type ON search_history(search_type);

-- ============================================================================
-- TRIGGERS (Auto-update timestamps and stats)
-- ============================================================================

-- Update timestamps on row modification
CREATE TRIGGER trigger_documents_updated_at
    AFTER UPDATE ON documents
    FOR EACH ROW
BEGIN
    UPDATE documents SET updated_at = unixepoch() WHERE id = NEW.id;
END;

CREATE TRIGGER trigger_file_metadata_updated_at
    AFTER UPDATE ON file_metadata
    FOR EACH ROW
BEGIN
    UPDATE file_metadata SET updated_at = unixepoch() WHERE id = NEW.id;
END;

CREATE TRIGGER trigger_topics_updated_at
    AFTER UPDATE ON topics
    FOR EACH ROW
BEGIN
    UPDATE topics SET updated_at = unixepoch() WHERE id = NEW.id;
END;

CREATE TRIGGER trigger_watch_folders_updated_at
    AFTER UPDATE ON watch_folders
    FOR EACH ROW
BEGIN
    UPDATE watch_folders SET updated_at = unixepoch() WHERE id = NEW.id;
END;

-- Auto-update topic document count
CREATE TRIGGER trigger_document_topic_assign
    AFTER UPDATE OF topic_id ON documents
    FOR EACH ROW
    WHEN NEW.topic_id IS NOT NULL
BEGIN
    UPDATE topics 
    SET document_count = document_count - 1 
    WHERE id = OLD.topic_id AND OLD.topic_id IS NOT NULL;
    
    UPDATE topics 
    SET document_count = document_count + 1 
    WHERE id = NEW.topic_id;
END;

CREATE TRIGGER trigger_document_topic_delete
    AFTER DELETE ON documents
    FOR EACH ROW
    WHEN OLD.topic_id IS NOT NULL
BEGIN
    UPDATE topics 
    SET document_count = document_count - 1 
    WHERE id = OLD.topic_id;
END;

-- Auto-update tag file count
CREATE TRIGGER trigger_tag_count_insert
    AFTER INSERT ON document_tags
    FOR EACH ROW
BEGIN
    UPDATE tags SET file_count = file_count + 1 WHERE id = NEW.tag_id;
END;

CREATE TRIGGER trigger_tag_count_delete
    AFTER DELETE ON document_tags
    FOR EACH ROW
BEGIN
    UPDATE tags SET file_count = file_count - 1 WHERE id = OLD.tag_id;
END;

-- Sync FTS tables on document changes
CREATE TRIGGER trigger_fts_documents_insert
    AFTER INSERT ON documents
    FOR EACH ROW
BEGIN
    INSERT INTO fts_documents(document_id, filename, content, metadata)
    VALUES (NEW.id, NEW.filename, COALESCE(NEW.content, ''), NEW.metadata_json);
END;

CREATE TRIGGER trigger_fts_documents_update
    AFTER UPDATE ON documents
    FOR EACH ROW
BEGIN
    UPDATE fts_documents 
    SET filename = NEW.filename,
        content = COALESCE(NEW.content, ''),
        metadata = NEW.metadata_json
    WHERE document_id = NEW.id;
END;

CREATE TRIGGER trigger_fts_documents_delete
    AFTER DELETE ON documents
    FOR EACH ROW
BEGIN
    DELETE FROM fts_documents WHERE document_id = OLD.id;
END;

-- Sync FTS chunks on embedding changes
CREATE TRIGGER trigger_fts_chunks_insert
    AFTER INSERT ON embeddings
    FOR EACH ROW
    WHEN NEW.chunk_text IS NOT NULL
BEGIN
    INSERT INTO fts_chunks(embedding_id, chunk_text)
    VALUES (NEW.id, NEW.chunk_text);
END;

CREATE TRIGGER trigger_fts_chunks_delete
    AFTER DELETE ON embeddings
    FOR EACH ROW
BEGIN
    DELETE FROM fts_chunks WHERE embedding_id = OLD.id;
END;

-- Sync FTS topics
CREATE TRIGGER trigger_fts_topics_insert
    AFTER INSERT ON topics
    FOR EACH ROW
BEGIN
    INSERT INTO fts_topics(topic_id, name, keywords, description)
    VALUES (NEW.id, NEW.name, NEW.keywords_json, COALESCE(NEW.description, ''));
END;

CREATE TRIGGER trigger_fts_topics_update
    AFTER UPDATE ON topics
    FOR EACH ROW
BEGIN
    UPDATE fts_topics
    SET name = NEW.name,
        keywords = NEW.keywords_json,
        description = COALESCE(NEW.description, '')
    WHERE topic_id = NEW.id;
END;

CREATE TRIGGER trigger_fts_topics_delete
    AFTER DELETE ON topics
    FOR EACH ROW
BEGIN
    DELETE FROM fts_topics WHERE topic_id = OLD.id;
END;

-- ============================================================================
-- VIEWS (Convenience queries)
-- ============================================================================

-- Documents with topic info
CREATE VIEW v_documents_with_topics AS
SELECT 
    d.*,
    t.name as topic_name,
    t.keywords_json as topic_keywords,
    t.document_count as topic_doc_count
FROM documents d
LEFT JOIN topics t ON d.topic_id = t.id;

-- Topic statistics
CREATE VIEW v_topic_stats AS
SELECT 
    t.id,
    t.name,
    t.document_count,
    t.silhouette_score,
    COUNT(DISTINCT e.id) as embedding_count,
    AVG(d.word_count) as avg_word_count,
    SUM(d.size_bytes) as total_size_bytes
FROM topics t
LEFT JOIN documents d ON t.id = d.topic_id
LEFT JOIN embeddings e ON t.id = e.topic_id
GROUP BY t.id;

-- Documents needing indexing
CREATE VIEW v_documents_pending_index AS
SELECT 
    d.*,
    fm.needs_reindex,
    fm.last_scan_at
FROM documents d
LEFT JOIN file_metadata fm ON d.id = fm.document_id
WHERE d.index_status IN ('pending', 'failed') OR fm.needs_reindex = 1;

-- ============================================================================
-- INITIAL DATA
-- ============================================================================

-- Create default "Uncategorized" topic with placeholder centroid
INSERT INTO topics (id, name, centroid_blob, keywords_json, description)
VALUES (
    'uncategorized',
    'Uncategorized',
    X'00',
    '["general", "misc"]',
    'Documents not yet assigned to a specific topic'
);
