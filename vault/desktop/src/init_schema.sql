-- Initialize complete database schema for SQLx offline mode
-- Generated from src/infrastructure/persistence/database/schema.rs

-- Pragmas
PRAGMA foreign_keys = ON;
PRAGMA journal_mode = WAL;
PRAGMA synchronous = NORMAL;
PRAGMA cache_size = -20000;
PRAGMA temp_store = MEMORY;
PRAGMA mmap_size = 268435456;

-- Documents table
CREATE TABLE IF NOT EXISTS documents (
    id TEXT PRIMARY KEY,
    file_path TEXT NOT NULL UNIQUE,
    file_name TEXT NOT NULL,
    file_type TEXT,
    mime_type TEXT,
    size_bytes INTEGER NOT NULL,
    modified_at TEXT NOT NULL,
    indexed_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    checksum TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending',
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    source_type TEXT DEFAULT 'local' CHECK(source_type IN ('local', 'web')),
    canonical_url TEXT,
    site_name TEXT,
    favicon_url TEXT,
    reading_time_minutes INTEGER,
    web_author TEXT,
    published_at TEXT,
    web_description TEXT,
    web_keywords TEXT,
    web_language TEXT,
    language TEXT DEFAULT 'unknown',
    category TEXT DEFAULT 'uncategorized',
    quality_score REAL DEFAULT 0.5 CHECK(quality_score >= 0.0 AND quality_score <= 1.0),
    access_count INTEGER DEFAULT 0,
    last_accessed_at INTEGER,
    word_count INTEGER DEFAULT 0
);

-- Text chunks table
CREATE TABLE IF NOT EXISTS text_chunks (
    id TEXT PRIMARY KEY,
    document_id TEXT NOT NULL,
    content TEXT NOT NULL,
    contextualized_content TEXT,
    context_prefix TEXT,
    chunk_index INTEGER NOT NULL,
    start_char INTEGER,
    end_char INTEGER,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    language TEXT DEFAULT 'unknown',
    token_count INTEGER DEFAULT 0,
    word_count INTEGER DEFAULT 0,
    has_code BOOLEAN DEFAULT 0,
    section TEXT,
    FOREIGN KEY (document_id) REFERENCES documents(id) ON DELETE CASCADE
);

-- Text embeddings table
CREATE TABLE IF NOT EXISTS text_embeddings (
    id TEXT PRIMARY KEY,
    chunk_id TEXT NOT NULL,
    embedding BLOB NOT NULL,
    model_name TEXT NOT NULL,
    dimension INTEGER NOT NULL,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (chunk_id) REFERENCES text_chunks(id) ON DELETE CASCADE
);

-- Image embeddings table
CREATE TABLE IF NOT EXISTS image_embeddings (
    id TEXT PRIMARY KEY,
    document_id TEXT NOT NULL,
    embedding BLOB NOT NULL,
    model_name TEXT NOT NULL,
    dimension INTEGER NOT NULL,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (document_id) REFERENCES documents(id) ON DELETE CASCADE
);

-- Image metadata table
CREATE TABLE IF NOT EXISTS image_metadata (
    id TEXT PRIMARY KEY,
    document_id TEXT NOT NULL,
    width INTEGER,
    height INTEGER,
    format TEXT,
    has_exif BOOLEAN DEFAULT FALSE,
    exif_data TEXT,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (document_id) REFERENCES documents(id) ON DELETE CASCADE
);

-- Tags table
CREATE TABLE IF NOT EXISTS tags (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE COLLATE NOCASE,
    color TEXT NOT NULL DEFAULT '#6366f1',
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- Document tags junction table
CREATE TABLE IF NOT EXISTS document_tags (
    document_id TEXT NOT NULL,
    tag_id TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (document_id, tag_id),
    FOREIGN KEY (document_id) REFERENCES documents(id) ON DELETE CASCADE,
    FOREIGN KEY (tag_id) REFERENCES tags(id) ON DELETE CASCADE
);

-- Mentions table
CREATE TABLE IF NOT EXISTS mentions (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    type TEXT NOT NULL CHECK(type IN ('person', 'concept', 'wikilink')),
    metadata TEXT,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- Document mentions junction table
CREATE TABLE IF NOT EXISTS document_mentions (
    id TEXT PRIMARY KEY,
    document_id TEXT NOT NULL,
    mention_id TEXT NOT NULL,
    context TEXT,
    position INTEGER,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (document_id) REFERENCES documents(id) ON DELETE CASCADE,
    FOREIGN KEY (mention_id) REFERENCES mentions(id) ON DELETE CASCADE
);

-- Watch folders table
CREATE TABLE IF NOT EXISTS watch_folders (
    id TEXT PRIMARY KEY,
    path TEXT NOT NULL UNIQUE,
    recursive BOOLEAN NOT NULL DEFAULT TRUE,
    enabled BOOLEAN NOT NULL DEFAULT TRUE,
    last_scan TEXT,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- Files table
CREATE TABLE IF NOT EXISTS files (
    id TEXT PRIMARY KEY NOT NULL,
    content_hash TEXT NOT NULL UNIQUE,
    file_name TEXT NOT NULL,
    file_extension TEXT,
    mime_type TEXT NOT NULL,
    size_bytes INTEGER NOT NULL,
    storage_path TEXT NOT NULL,
    is_indexed INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL,
    accessed_at INTEGER NOT NULL,
    ref_count INTEGER NOT NULL DEFAULT 1 CHECK (ref_count >= 0),
    metadata TEXT
);

-- File references junction table
CREATE TABLE IF NOT EXISTS file_references (
    id TEXT PRIMARY KEY NOT NULL,
    file_id TEXT NOT NULL,
    document_id TEXT NOT NULL,
    reference_type TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    FOREIGN KEY (file_id) REFERENCES files(id) ON DELETE CASCADE,
    FOREIGN KEY (document_id) REFERENCES documents(id) ON DELETE CASCADE,
    UNIQUE(file_id, document_id, reference_type)
);

-- FTS5 virtual table for chunk-level search (PRIMARY INDEX)
-- Oracle Decision: Chunk-level indexing for accurate BM25 scoring and retrieval
CREATE VIRTUAL TABLE IF NOT EXISTS chunks_fts USING fts5(
    chunk_id UNINDEXED,
    content,
    tokenize='porter unicode61 remove_diacritics 2'
);

-- FTS5 triggers to sync text_chunks into chunks_fts
CREATE TRIGGER chunks_fts_insert
AFTER INSERT ON text_chunks
BEGIN
    INSERT INTO chunks_fts(chunk_id, content)
    VALUES (new.id, new.content);
END;

CREATE TRIGGER chunks_fts_delete
AFTER DELETE ON text_chunks
BEGIN
    DELETE FROM chunks_fts WHERE chunk_id = old.id;
END;

CREATE TRIGGER chunks_fts_update
AFTER UPDATE OF content ON text_chunks
BEGIN
    DELETE FROM chunks_fts WHERE chunk_id = old.id;
    INSERT INTO chunks_fts(chunk_id, content)
    VALUES (new.id, new.content);
END;

-- Legacy FTS5 virtual table for document-level search (DEPRECATED)
-- Kept for backward compatibility but not used by TextSearchPort
CREATE VIRTUAL TABLE documents_fts USING fts5(
    document_id UNINDEXED,
    content,
    tokenize='porter unicode61 remove_diacritics 2'
);

-- Legacy FTS5 triggers to aggregate chunks into document-level entries (DEPRECATED)
CREATE TRIGGER text_chunks_fts_insert
AFTER INSERT ON text_chunks
BEGIN
    DELETE FROM documents_fts WHERE document_id = new.document_id;
    INSERT INTO documents_fts(document_id, content)
    SELECT new.document_id, GROUP_CONCAT(content, ' ')
    FROM text_chunks
    WHERE document_id = new.document_id
    GROUP BY document_id;
END;

CREATE TRIGGER text_chunks_fts_delete
AFTER DELETE ON text_chunks
BEGIN
    DELETE FROM documents_fts WHERE document_id = old.document_id;
    INSERT INTO documents_fts(document_id, content)
    SELECT old.document_id, GROUP_CONCAT(content, ' ')
    FROM text_chunks
    WHERE document_id = old.document_id
    GROUP BY document_id;
END;

CREATE TRIGGER text_chunks_fts_update
AFTER UPDATE OF content ON text_chunks
BEGIN
    DELETE FROM documents_fts WHERE document_id = new.document_id;
    INSERT INTO documents_fts(document_id, content)
    SELECT new.document_id, GROUP_CONCAT(content, ' ')
    FROM text_chunks
    WHERE document_id = new.document_id
    GROUP BY document_id;
END;

-- Favorites table
CREATE TABLE IF NOT EXISTS favorites (
    id TEXT PRIMARY KEY,
    document_id TEXT NOT NULL UNIQUE,
    added_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (document_id) REFERENCES documents(id) ON DELETE CASCADE
);

-- Recent documents table
CREATE TABLE IF NOT EXISTS recent_documents (
    id TEXT PRIMARY KEY,
    document_id TEXT NOT NULL UNIQUE,
    last_accessed_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    access_count INTEGER NOT NULL DEFAULT 1,
    FOREIGN KEY (document_id) REFERENCES documents(id) ON DELETE CASCADE
);

-- Conversations table
CREATE TABLE IF NOT EXISTS conversations (
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    model_name TEXT NOT NULL,
    system_prompt TEXT,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    message_count INTEGER NOT NULL DEFAULT 0,
    total_tokens INTEGER NOT NULL DEFAULT 0
);

-- Conversation messages table
CREATE TABLE IF NOT EXISTS conversation_messages (
    id TEXT PRIMARY KEY,
    conversation_id TEXT NOT NULL,
    role TEXT NOT NULL CHECK(role IN ('user', 'assistant', 'system')),
    content TEXT NOT NULL,
    tokens INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    metadata TEXT,
    FOREIGN KEY (conversation_id) REFERENCES conversations(id) ON DELETE CASCADE
);

-- Conversation documents table
CREATE TABLE IF NOT EXISTS conversation_documents (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    conversation_id TEXT NOT NULL,
    document_id TEXT NOT NULL,
    chunk_id TEXT,
    relevance_score REAL,
    added_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (conversation_id) REFERENCES conversations(id) ON DELETE CASCADE,
    FOREIGN KEY (document_id) REFERENCES documents(id) ON DELETE CASCADE,
    UNIQUE(conversation_id, chunk_id)
);

-- Batch jobs table
CREATE TABLE IF NOT EXISTS batch_jobs (
    id TEXT PRIMARY KEY,
    job_type TEXT NOT NULL,
    status TEXT NOT NULL CHECK(status IN ('pending', 'running', 'completed', 'failed', 'cancelled')),
    total_items INTEGER NOT NULL,
    completed_items INTEGER NOT NULL DEFAULT 0,
    failed_items INTEGER NOT NULL DEFAULT 0,
    progress REAL NOT NULL DEFAULT 0.0,
    options TEXT,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    started_at TEXT,
    completed_at TEXT,
    error_message TEXT
) STRICT;

-- Batch job items table
CREATE TABLE IF NOT EXISTS batch_job_items (
    id TEXT PRIMARY KEY,
    job_id TEXT NOT NULL,
    item_url TEXT NOT NULL,
    document_id TEXT,
    status TEXT NOT NULL CHECK(status IN ('pending', 'processing', 'completed', 'failed', 'skipped')),
    error_message TEXT,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    processed_at TEXT,
    FOREIGN KEY (job_id) REFERENCES batch_jobs(id) ON DELETE CASCADE,
    FOREIGN KEY (document_id) REFERENCES documents(id) ON DELETE SET NULL
) STRICT;

-- Web assets table
CREATE TABLE IF NOT EXISTS web_assets (
    id TEXT PRIMARY KEY,
    document_id TEXT NOT NULL,
    url TEXT NOT NULL,
    local_path TEXT NOT NULL,
    content_type TEXT NOT NULL,
    size_bytes INTEGER NOT NULL,
    checksum TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (document_id) REFERENCES documents(id) ON DELETE CASCADE
) STRICT;

-- Schema version tracking table
CREATE TABLE IF NOT EXISTS schema_version (
    version INTEGER PRIMARY KEY,
    applied_at INTEGER NOT NULL DEFAULT (unixepoch()),
    description TEXT
);

-- Insert schema version
INSERT OR REPLACE INTO schema_version (version, description)
VALUES (15, 'Complete schema with Phase 1 metadata, batch jobs, and web assets');

-- ========== INDEXES ==========

-- Critical chunk indexes
CREATE INDEX IF NOT EXISTS idx_chunks_document ON text_chunks(document_id);
CREATE INDEX IF NOT EXISTS idx_chunks_composite ON text_chunks(document_id, chunk_index);

-- Document indexes
CREATE INDEX IF NOT EXISTS idx_documents_file_path ON documents(file_path);
CREATE INDEX IF NOT EXISTS idx_documents_filename ON documents(file_name);
CREATE INDEX IF NOT EXISTS idx_documents_status ON documents(status);
CREATE INDEX IF NOT EXISTS idx_documents_checksum ON documents(checksum);
CREATE INDEX IF NOT EXISTS idx_documents_updated_desc ON documents(updated_at DESC);
CREATE INDEX IF NOT EXISTS idx_documents_indexed ON documents(indexed_at DESC);
CREATE INDEX IF NOT EXISTS idx_documents_status_updated ON documents(status, updated_at DESC);

-- Embedding indexes
CREATE INDEX IF NOT EXISTS idx_embeddings_chunk ON text_embeddings(chunk_id);
CREATE INDEX IF NOT EXISTS idx_image_embeddings_document ON image_embeddings(document_id);
CREATE INDEX IF NOT EXISTS idx_embeddings_chunk_model ON text_embeddings(chunk_id, model_name);

-- Watch folder indexes
CREATE INDEX IF NOT EXISTS idx_watch_folders_path ON watch_folders(path);

-- Chunk content length index
CREATE INDEX IF NOT EXISTS idx_chunks_content_length ON text_chunks(length(content));
CREATE INDEX IF NOT EXISTS idx_chunks_start_char ON text_chunks(document_id, start_char);

-- Phase 1 chunk metadata indexes
CREATE INDEX IF NOT EXISTS idx_text_chunks_has_code ON text_chunks(has_code);
CREATE INDEX IF NOT EXISTS idx_text_chunks_word_count ON text_chunks(word_count);
CREATE INDEX IF NOT EXISTS idx_text_chunks_section ON text_chunks(section);
CREATE INDEX IF NOT EXISTS idx_text_chunks_language_code ON text_chunks(language, has_code);

-- Tag indexes
CREATE INDEX IF NOT EXISTS idx_tags_name ON tags(name);
CREATE INDEX IF NOT EXISTS idx_tags_name_lower ON tags(LOWER(name));
CREATE INDEX IF NOT EXISTS idx_document_tags_document_id ON document_tags(document_id);
CREATE INDEX IF NOT EXISTS idx_document_tags_tag_id ON document_tags(tag_id);
CREATE INDEX IF NOT EXISTS idx_document_tags_composite ON document_tags(document_id, tag_id);

-- Mention indexes
CREATE INDEX IF NOT EXISTS idx_mentions_name ON mentions(name);
CREATE INDEX IF NOT EXISTS idx_mentions_type ON mentions(type);
CREATE INDEX IF NOT EXISTS idx_mentions_type_name ON mentions(type, name);
CREATE INDEX IF NOT EXISTS idx_document_mentions_document_id ON document_mentions(document_id);
CREATE INDEX IF NOT EXISTS idx_document_mentions_mention_id ON document_mentions(mention_id);
CREATE INDEX IF NOT EXISTS idx_document_mentions_composite ON document_mentions(document_id, mention_id);

-- Favorites indexes
CREATE INDEX IF NOT EXISTS idx_favorites_document_id ON favorites(document_id);
CREATE INDEX IF NOT EXISTS idx_favorites_added_at ON favorites(added_at DESC);

-- Recent documents indexes
CREATE INDEX IF NOT EXISTS idx_recent_documents_document_id ON recent_documents(document_id);
CREATE INDEX IF NOT EXISTS idx_recent_documents_last_accessed ON recent_documents(last_accessed_at DESC);
CREATE INDEX IF NOT EXISTS idx_recent_documents_access_count ON recent_documents(access_count DESC);

-- File storage indexes
CREATE INDEX IF NOT EXISTS idx_files_hash ON files(content_hash);
CREATE INDEX IF NOT EXISTS idx_files_mime ON files(mime_type);
CREATE INDEX IF NOT EXISTS idx_files_accessed ON files(accessed_at);
CREATE INDEX IF NOT EXISTS idx_files_extension ON files(file_extension);
CREATE INDEX IF NOT EXISTS idx_file_refs_file ON file_references(file_id);
CREATE INDEX IF NOT EXISTS idx_file_refs_doc ON file_references(document_id);

-- Web document indexes
CREATE INDEX IF NOT EXISTS idx_documents_source_type ON documents(source_type);
CREATE INDEX IF NOT EXISTS idx_documents_canonical_url ON documents(canonical_url) WHERE canonical_url IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_documents_web ON documents(source_type, indexed_at DESC) WHERE source_type = 'web';

-- Phase 2 metadata indexes
CREATE INDEX IF NOT EXISTS idx_documents_language ON documents(language);
CREATE INDEX IF NOT EXISTS idx_documents_category ON documents(category);
CREATE INDEX IF NOT EXISTS idx_documents_quality_score ON documents(quality_score DESC);
CREATE INDEX IF NOT EXISTS idx_documents_access_count ON documents(access_count DESC);
CREATE INDEX IF NOT EXISTS idx_documents_last_accessed ON documents(last_accessed_at DESC);
CREATE INDEX IF NOT EXISTS idx_documents_category_quality ON documents(category, quality_score DESC);

-- Conversation indexes
CREATE INDEX IF NOT EXISTS idx_conversation_messages_conversation_id ON conversation_messages(conversation_id, created_at);
CREATE INDEX IF NOT EXISTS idx_conversation_messages_role ON conversation_messages(role);
CREATE INDEX IF NOT EXISTS idx_conversations_updated_at ON conversations(updated_at DESC);
CREATE INDEX IF NOT EXISTS idx_conversation_documents_conversation_id ON conversation_documents(conversation_id);
CREATE INDEX IF NOT EXISTS idx_conversation_documents_document_id ON conversation_documents(document_id);

-- Batch jobs indexes
CREATE INDEX IF NOT EXISTS idx_batch_jobs_status ON batch_jobs(status);
CREATE INDEX IF NOT EXISTS idx_batch_jobs_created_at ON batch_jobs(created_at DESC);
CREATE INDEX IF NOT EXISTS idx_batch_jobs_type_status ON batch_jobs(job_type, status);

-- Batch job items indexes
CREATE INDEX IF NOT EXISTS idx_batch_job_items_job_id ON batch_job_items(job_id);
CREATE INDEX IF NOT EXISTS idx_batch_job_items_status ON batch_job_items(status);
CREATE INDEX IF NOT EXISTS idx_batch_job_items_job_status ON batch_job_items(job_id, status);
CREATE INDEX IF NOT EXISTS idx_batch_job_items_document_id ON batch_job_items(document_id) WHERE document_id IS NOT NULL;

-- Web assets indexes
CREATE INDEX IF NOT EXISTS idx_web_assets_document_id ON web_assets(document_id);
CREATE INDEX IF NOT EXISTS idx_web_assets_checksum ON web_assets(checksum);
CREATE INDEX IF NOT EXISTS idx_web_assets_url ON web_assets(url);

-- Run analyze to update statistics
ANALYZE;
