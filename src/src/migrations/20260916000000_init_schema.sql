-- Lattice canonical schema.
--
-- This single migration replaces the 28-file historical chain that ran from
-- 20250101000000 through 20260916020000. Lattice is pre-release with no legacy
-- database support, so the chain's data-repair steps (backfills, timestamp
-- normalizations, "purge legacy" deletes, table rename dances) have no fresh
-- database to repair and are deliberately absent. What remains is the schema
-- those migrations converged on, plus the seed rows they installed.
--
-- PRAGMAs (foreign_keys, journal_mode, synchronous, ...) are applied by the
-- connection layer, not here, to avoid transaction issues.

-- =====================================================================
-- Documents and chunks
-- =====================================================================

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
    word_count INTEGER DEFAULT 0,
    source_context TEXT CHECK(source_context IS NULL OR json_valid(source_context))
);

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
    page_number INTEGER CHECK(page_number IS NULL OR page_number > 0),
    FOREIGN KEY (document_id) REFERENCES documents(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS text_embeddings (
    id TEXT PRIMARY KEY,
    chunk_id TEXT NOT NULL,
    embedding BLOB NOT NULL,
    model_name TEXT NOT NULL,
    dimension INTEGER NOT NULL,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (chunk_id) REFERENCES text_chunks(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS image_embeddings (
    id TEXT PRIMARY KEY,
    document_id TEXT NOT NULL,
    embedding BLOB NOT NULL,
    model_name TEXT NOT NULL,
    dimension INTEGER NOT NULL,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (document_id) REFERENCES documents(id) ON DELETE CASCADE
);

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

-- =====================================================================
-- Tags and mentions
-- =====================================================================

CREATE TABLE IF NOT EXISTS tags (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE COLLATE NOCASE,
    color TEXT NOT NULL DEFAULT '#6366f1',
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS document_tags (
    document_id TEXT NOT NULL,
    tag_id TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (document_id, tag_id),
    FOREIGN KEY (document_id) REFERENCES documents(id) ON DELETE CASCADE,
    FOREIGN KEY (tag_id) REFERENCES tags(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS mentions (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    type TEXT NOT NULL CHECK(type IN ('person', 'concept', 'wikilink')),
    metadata TEXT,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

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

-- =====================================================================
-- File storage
-- =====================================================================

CREATE TABLE IF NOT EXISTS watch_folders (
    id TEXT PRIMARY KEY,
    path TEXT NOT NULL UNIQUE,
    recursive BOOLEAN NOT NULL DEFAULT TRUE,
    enabled BOOLEAN NOT NULL DEFAULT TRUE,
    last_scan TEXT,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

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

-- =====================================================================
-- Full-text search over chunks (the index every search query reads)
-- =====================================================================

CREATE VIRTUAL TABLE IF NOT EXISTS chunks_fts USING fts5(
    chunk_id UNINDEXED,
    content,
    tokenize='porter unicode61 remove_diacritics 2'
);

-- Indexes the contextualized text when the chunker produced one, else the raw
-- chunk body.
CREATE TRIGGER chunks_fts_insert AFTER INSERT ON text_chunks BEGIN
  INSERT INTO chunks_fts(chunk_id, content) VALUES(new.id, COALESCE(new.contextualized_content, new.content));
END;

CREATE TRIGGER chunks_fts_update AFTER UPDATE ON text_chunks BEGIN
  DELETE FROM chunks_fts WHERE chunk_id = old.id;
  INSERT INTO chunks_fts(chunk_id, content) VALUES(new.id, COALESCE(new.contextualized_content, new.content));
END;

CREATE TRIGGER chunks_fts_delete
AFTER DELETE ON text_chunks
BEGIN
    DELETE FROM chunks_fts WHERE chunk_id = old.id;
END;

-- Legacy document-level FTS5 table. No search path reads it; it is retained
-- because maintenance callers (rebuild_fts5_index, fts5_migration) still target
-- it by name. Deliberately has NO triggers: re-tokenizing a whole growing
-- document after every chunk insert held SQLite's single writer for minutes.
CREATE VIRTUAL TABLE documents_fts USING fts5(
    document_id UNINDEXED,
    content,
    tokenize='porter unicode61 remove_diacritics 2'
);

-- =====================================================================
-- Favorites and recents
-- =====================================================================

CREATE TABLE IF NOT EXISTS favorites (
    id TEXT PRIMARY KEY,
    document_id TEXT NOT NULL UNIQUE,
    added_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (document_id) REFERENCES documents(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS recent_documents (
    id TEXT PRIMARY KEY,
    document_id TEXT NOT NULL UNIQUE,
    last_accessed_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    access_count INTEGER NOT NULL DEFAULT 1,
    FOREIGN KEY (document_id) REFERENCES documents(id) ON DELETE CASCADE
);

-- =====================================================================
-- Conversation spaces (standard spaces only; journals are separate)
-- =====================================================================

CREATE TABLE IF NOT EXISTS conversation_spaces (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    description TEXT,
    icon TEXT,
    accent_color TEXT,
    space_prompt TEXT,
    default_model_name TEXT,
    tool_preferences_json TEXT,
    is_archived INTEGER NOT NULL DEFAULT 0 CHECK (is_archived IN (0, 1)),
    sort_order INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- Seed: the default space every conversation falls back to.
-- Timestamps use the RFC3339 form that ordered text columns are stored in.
INSERT OR IGNORE INTO conversation_spaces (
    id,
    name,
    description,
    sort_order,
    created_at,
    updated_at
) VALUES (
    'space_general',
    'General',
    'Default space for all conversations',
    0,
    strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
    strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
);

-- =====================================================================
-- Conversations
-- =====================================================================

CREATE TABLE IF NOT EXISTS conversations (
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    model_name TEXT NOT NULL,
    system_prompt TEXT,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    message_count INTEGER NOT NULL DEFAULT 0,
    total_tokens INTEGER NOT NULL DEFAULT 0,
    space_id TEXT NOT NULL DEFAULT 'space_general',
    is_saved INTEGER NOT NULL DEFAULT 0 CHECK (is_saved IN (0, 1)),
    is_bookmarked INTEGER NOT NULL DEFAULT 0 CHECK (is_bookmarked IN (0, 1)),
    is_pinned INTEGER NOT NULL DEFAULT 0 CHECK (is_pinned IN (0, 1)),
    is_archived INTEGER NOT NULL DEFAULT 0 CHECK (is_archived IN (0, 1)),
    saved_at TEXT,
    bookmarked_at TEXT,
    pinned_at TEXT,
    archived_at TEXT
);

CREATE TABLE IF NOT EXISTS conversation_messages (
    id TEXT PRIMARY KEY,
    conversation_id TEXT NOT NULL,
    role TEXT NOT NULL CHECK(role IN ('user', 'assistant', 'system')),
    content TEXT NOT NULL,
    tokens INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    metadata TEXT,
    status TEXT NOT NULL DEFAULT 'completed'
        CHECK(status IN ('pending', 'completed', 'failed', 'streaming', 'error')),
    FOREIGN KEY (conversation_id) REFERENCES conversations(id) ON DELETE CASCADE
);

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

CREATE TABLE IF NOT EXISTS conversation_summaries (
    id TEXT PRIMARY KEY,
    conversation_id TEXT NOT NULL UNIQUE,
    summary_text TEXT NOT NULL,
    up_to_message_id TEXT NOT NULL,
    original_message_count INTEGER NOT NULL CHECK(original_message_count > 0),
    original_tokens INTEGER NOT NULL CHECK(original_tokens > 0),
    summary_tokens INTEGER NOT NULL CHECK(summary_tokens > 0),
    compression_ratio REAL NOT NULL CHECK(compression_ratio > 0.0 AND compression_ratio <= 1.0),
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (conversation_id) REFERENCES conversations(id) ON DELETE CASCADE,
    FOREIGN KEY (up_to_message_id) REFERENCES conversation_messages(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS conversation_memory_vectors (
    id TEXT PRIMARY KEY,
    conversation_id TEXT NOT NULL,
    message_id TEXT NOT NULL UNIQUE,
    role TEXT NOT NULL CHECK(role IN ('user', 'assistant', 'system')),
    content TEXT NOT NULL,
    embedding BLOB NOT NULL,
    dimension INTEGER NOT NULL CHECK(dimension > 0),
    embedding_model TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (conversation_id) REFERENCES conversations(id) ON DELETE CASCADE,
    FOREIGN KEY (message_id) REFERENCES conversation_messages(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS conversation_message_bookmarks (
    id TEXT PRIMARY KEY,
    conversation_id TEXT NOT NULL,
    message_id TEXT NOT NULL,
    title TEXT,
    note TEXT,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (conversation_id) REFERENCES conversations(id) ON DELETE CASCADE,
    FOREIGN KEY (message_id) REFERENCES conversation_messages(id) ON DELETE CASCADE,
    UNIQUE (conversation_id, message_id)
);

CREATE TABLE IF NOT EXISTS conversation_web_sources (
    id TEXT PRIMARY KEY,
    conversation_id TEXT NOT NULL,
    url TEXT NOT NULL,
    normalized_url TEXT NOT NULL,
    title TEXT,
    excerpt TEXT,
    relevance_score REAL,
    added_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (conversation_id) REFERENCES conversations(id) ON DELETE CASCADE,
    UNIQUE (conversation_id, normalized_url)
);

-- A conversation must always belong to an existing standard space.
CREATE TRIGGER IF NOT EXISTS trg_conversations_require_valid_space_insert
BEFORE INSERT ON conversations
WHEN NOT EXISTS (
    SELECT 1
    FROM conversation_spaces s
    WHERE s.id = NEW.space_id
)
BEGIN
    SELECT RAISE(ABORT, 'conversation must belong to an existing space');
END;

CREATE TRIGGER IF NOT EXISTS trg_conversations_require_valid_space_update
BEFORE UPDATE OF space_id ON conversations
WHEN NOT EXISTS (
    SELECT 1
    FROM conversation_spaces s
    WHERE s.id = NEW.space_id
)
BEGIN
    SELECT RAISE(ABORT, 'conversation must belong to an existing space');
END;

-- =====================================================================
-- Conversation Spotlight search index
-- =====================================================================

CREATE VIRTUAL TABLE IF NOT EXISTS conversation_search_fts USING fts5(
    conversation_id UNINDEXED,
    message_id UNINDEXED,
    source UNINDEXED,
    content,
    tokenize = 'unicode61 remove_diacritics 2'
);

CREATE TRIGGER IF NOT EXISTS trg_conversation_search_fts_conversation_ai
AFTER INSERT ON conversations
BEGIN
    INSERT INTO conversation_search_fts (conversation_id, message_id, source, content)
    VALUES (new.id, NULL, 'title', COALESCE(new.title, ''));
END;

CREATE TRIGGER IF NOT EXISTS trg_conversation_search_fts_conversation_au
AFTER UPDATE OF title ON conversations
BEGIN
    DELETE FROM conversation_search_fts
    WHERE conversation_id = old.id AND source = 'title';
    INSERT INTO conversation_search_fts (conversation_id, message_id, source, content)
    VALUES (new.id, NULL, 'title', COALESCE(new.title, ''));
END;

CREATE TRIGGER IF NOT EXISTS trg_conversation_search_fts_conversation_ad
AFTER DELETE ON conversations
BEGIN
    DELETE FROM conversation_search_fts
    WHERE conversation_id = old.id;
END;

CREATE TRIGGER IF NOT EXISTS trg_conversation_search_fts_message_ai
AFTER INSERT ON conversation_messages
BEGIN
    INSERT INTO conversation_search_fts (conversation_id, message_id, source, content)
    VALUES (new.conversation_id, new.id, 'message', COALESCE(new.content, ''));
END;

CREATE TRIGGER IF NOT EXISTS trg_conversation_search_fts_message_au
AFTER UPDATE OF content ON conversation_messages
BEGIN
    DELETE FROM conversation_search_fts
    WHERE conversation_id = old.conversation_id AND message_id = old.id AND source = 'message';
    INSERT INTO conversation_search_fts (conversation_id, message_id, source, content)
    VALUES (new.conversation_id, new.id, 'message', COALESCE(new.content, ''));
END;

CREATE TRIGGER IF NOT EXISTS trg_conversation_search_fts_message_ad
AFTER DELETE ON conversation_messages
BEGIN
    DELETE FROM conversation_search_fts
    WHERE conversation_id = old.conversation_id AND message_id = old.id;
END;

CREATE TRIGGER IF NOT EXISTS trg_conversation_search_fts_bookmark_ai
AFTER INSERT ON conversation_message_bookmarks
BEGIN
    INSERT INTO conversation_search_fts (conversation_id, message_id, source, content)
    VALUES (
        new.conversation_id,
        new.message_id,
        'bookmark',
        TRIM(COALESCE(new.title, '') || ' ' || COALESCE(new.note, ''))
    );
END;

CREATE TRIGGER IF NOT EXISTS trg_conversation_search_fts_bookmark_au
AFTER UPDATE OF title, note ON conversation_message_bookmarks
BEGIN
    DELETE FROM conversation_search_fts
    WHERE conversation_id = old.conversation_id AND message_id = old.message_id AND source = 'bookmark';
    INSERT INTO conversation_search_fts (conversation_id, message_id, source, content)
    VALUES (
        new.conversation_id,
        new.message_id,
        'bookmark',
        TRIM(COALESCE(new.title, '') || ' ' || COALESCE(new.note, ''))
    );
END;

CREATE TRIGGER IF NOT EXISTS trg_conversation_search_fts_bookmark_ad
AFTER DELETE ON conversation_message_bookmarks
BEGIN
    DELETE FROM conversation_search_fts
    WHERE conversation_id = old.conversation_id AND message_id = old.message_id AND source = 'bookmark';
END;

-- =====================================================================
-- Space membership and collaborators
-- =====================================================================

CREATE TABLE IF NOT EXISTS document_space_memberships (
    document_id TEXT NOT NULL,
    space_id TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (document_id, space_id),
    FOREIGN KEY (document_id) REFERENCES documents(id) ON DELETE CASCADE,
    FOREIGN KEY (space_id) REFERENCES conversation_spaces(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS collaborator_profiles (
    id TEXT PRIMARY KEY,
    display_name TEXT NOT NULL,
    email TEXT,
    avatar_url TEXT,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS conversation_space_members (
    space_id TEXT NOT NULL,
    member_id TEXT NOT NULL,
    role TEXT NOT NULL CHECK (role IN ('owner', 'editor', 'viewer')),
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (space_id, member_id),
    FOREIGN KEY (space_id) REFERENCES conversation_spaces(id) ON DELETE CASCADE,
    FOREIGN KEY (member_id) REFERENCES collaborator_profiles(id) ON DELETE CASCADE
);

-- Seed: local-first baseline actor used until full auth/identity arrives.
INSERT OR IGNORE INTO collaborator_profiles (
    id,
    display_name,
    email,
    avatar_url,
    created_at,
    updated_at
) VALUES (
    'member_local_owner',
    'Local Owner',
    NULL,
    NULL,
    CURRENT_TIMESTAMP,
    CURRENT_TIMESTAMP
);

-- Seed: every space has an owner.
INSERT OR IGNORE INTO conversation_space_members (space_id, member_id, role)
SELECT id, 'member_local_owner', 'owner'
FROM conversation_spaces;

-- =====================================================================
-- Journals (a separate overlay; journals never own conversations)
-- =====================================================================

CREATE TABLE IF NOT EXISTS journals (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    description TEXT,
    icon TEXT,
    accent_color TEXT,
    space_prompt TEXT,
    default_model_name TEXT,
    tool_preferences_json TEXT,
    is_archived INTEGER NOT NULL DEFAULT 0 CHECK (is_archived IN (0, 1)),
    sort_order INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS journal_conversation_entries (
    journal_space_id TEXT NOT NULL,
    conversation_id TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (journal_space_id, conversation_id),
    FOREIGN KEY (journal_space_id) REFERENCES journals(id) ON DELETE CASCADE,
    FOREIGN KEY (conversation_id) REFERENCES conversations(id) ON DELETE CASCADE
);

-- =====================================================================
-- Daily notes workspace
-- =====================================================================

CREATE TABLE IF NOT EXISTS daily_notes_workspace (
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    content TEXT NOT NULL DEFAULT '',
    linked_document_ids TEXT NOT NULL DEFAULT '[]',
    linked_conversation_ids TEXT NOT NULL DEFAULT '[]',
    highlights_json TEXT NOT NULL DEFAULT '[]',
    sticky_notes_json TEXT NOT NULL DEFAULT '[]',
    conversation_snapshots_json TEXT NOT NULL DEFAULT '[]',
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- =====================================================================
-- Batch jobs and web assets
-- =====================================================================

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

-- =====================================================================
-- Model registry and downloads
-- =====================================================================

-- `storage_kind` is the discriminator that makes invalid location shapes
-- unrepresentable: a weight file, a model directory, or no on-disk artifact at
-- all (Ollama, where `storage_path` stays NULL).
CREATE TABLE IF NOT EXISTS models (
    id TEXT PRIMARY KEY,
    model_name TEXT NOT NULL,
    model_id TEXT NOT NULL UNIQUE,
    description TEXT,
    base_path TEXT,
    total_size_bytes INTEGER NOT NULL DEFAULT 0,
    model_type TEXT NOT NULL CHECK(model_type IN ('chat', 'embedding', 'qa', 'multi_modal', 'custom', 'ocr', 'unknown')),
    architecture TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending' CHECK(status IN ('pending', 'downloading', 'completed', 'failed', 'verified')),
    downloaded_at TEXT,
    last_used_at TEXT,
    use_count INTEGER NOT NULL DEFAULT 0,
    is_active_for_chat INTEGER NOT NULL DEFAULT 0 CHECK (is_active_for_chat IN (0, 1)),
    is_active_for_embedding INTEGER NOT NULL DEFAULT 0 CHECK (is_active_for_embedding IN (0, 1)),
    metadata TEXT,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    backend TEXT NOT NULL DEFAULT 'local'
        CHECK (backend IN ('local', 'ollama')),
    is_active_for_utility INTEGER NOT NULL DEFAULT 0
        CHECK (is_active_for_utility IN (0, 1)),
    storage_kind TEXT NOT NULL DEFAULT 'local_file'
        CHECK (storage_kind IN ('local_file', 'local_dir', 'remote_ollama')),
    storage_path TEXT
);

CREATE TABLE IF NOT EXISTS model_files (
    id TEXT PRIMARY KEY,
    model_id TEXT NOT NULL,
    file_name TEXT NOT NULL,
    file_path TEXT NOT NULL,
    relative_path TEXT,
    size_bytes INTEGER NOT NULL,
    downloaded_bytes INTEGER DEFAULT 0,
    checksum_sha256 TEXT,
    checksum TEXT,
    download_url TEXT,
    status TEXT NOT NULL CHECK(status IN ('pending', 'downloading', 'completed', 'failed', 'verified')),
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    downloaded_at TEXT,
    FOREIGN KEY (model_id) REFERENCES models(model_id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS download_sessions (
    id TEXT PRIMARY KEY NOT NULL,
    url TEXT NOT NULL,
    destination TEXT NOT NULL,
    state TEXT NOT NULL CHECK(state IN ('pending', 'downloading', 'paused', 'completed', 'failed', 'cancelled')),
    bytes_downloaded INTEGER NOT NULL DEFAULT 0,
    total_bytes INTEGER,
    bytes_per_second REAL NOT NULL DEFAULT 0.0,
    checksum_algorithm TEXT CHECK(checksum_algorithm IN ('sha256', 'md5')),
    checksum_value TEXT,
    error_message TEXT,
    retry_count INTEGER NOT NULL DEFAULT 0,
    max_retries INTEGER NOT NULL DEFAULT 3,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    started_at TEXT,
    completed_at TEXT,
    model_name TEXT,
    model_id TEXT,
    model_file_name TEXT
);

-- One active model per role slot.
CREATE TRIGGER IF NOT EXISTS ensure_single_active_chat_model
    BEFORE UPDATE OF is_active_for_chat ON models
    FOR EACH ROW
    WHEN NEW.is_active_for_chat = 1
BEGIN
    UPDATE models SET is_active_for_chat = 0 WHERE id != NEW.id;
END;

CREATE TRIGGER IF NOT EXISTS ensure_single_active_embedding_model
    BEFORE UPDATE OF is_active_for_embedding ON models
    FOR EACH ROW
    WHEN NEW.is_active_for_embedding = 1
BEGIN
    UPDATE models SET is_active_for_embedding = 0 WHERE id != NEW.id;
END;

CREATE TRIGGER IF NOT EXISTS ensure_single_active_utility_model
    BEFORE UPDATE OF is_active_for_utility ON models
    FOR EACH ROW
    WHEN NEW.is_active_for_utility = 1
BEGIN
    UPDATE models SET is_active_for_utility = 0 WHERE id != NEW.id;
END;

CREATE TRIGGER IF NOT EXISTS cleanup_download_sessions_after_model_delete
AFTER DELETE ON models
FOR EACH ROW
WHEN OLD.model_id IS NOT NULL
BEGIN
    DELETE FROM download_sessions
    WHERE model_id = OLD.model_id;
END;

-- Seed: the synthetic Ollama-server row. `model_id` is stable and immutable so
-- every run finds the same row; the AI Models tab toggles its role flags, while
-- the actual model tags live in LLM settings.
INSERT OR IGNORE INTO models (
    id,
    model_name,
    model_id,
    base_path,
    total_size_bytes,
    status,
    model_type,
    architecture,
    downloaded_at,
    last_used_at,
    use_count,
    is_active_for_chat,
    is_active_for_embedding,
    is_active_for_utility,
    backend,
    storage_kind,
    storage_path,
    metadata
)
VALUES (
    '__ollama_server__',
    'Ollama Server',
    '__ollama_server__',
    '',
    0,
    'completed',
    'chat',
    'ollama',
    CURRENT_TIMESTAMP,
    NULL,
    0,
    0,
    0,
    0,
    'ollama',
    'remote_ollama',
    NULL,
    '{"synthetic":true,"source":"ollama_server_row"}'
);

-- =====================================================================
-- Corpus-shape clustering overlay (derived; safe to regenerate)
-- =====================================================================

CREATE TABLE IF NOT EXISTS cluster_runs (
    id TEXT PRIMARY KEY,
    ran_at TEXT NOT NULL,
    doc_count INTEGER NOT NULL,
    cluster_count INTEGER NOT NULL,
    noise_count INTEGER NOT NULL,
    params_hash TEXT NOT NULL,
    duration_ms INTEGER NOT NULL,
    llm_calls INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS clusters (
    id TEXT PRIMARY KEY,
    run_id TEXT NOT NULL REFERENCES cluster_runs(id) ON DELETE CASCADE,
    label TEXT NOT NULL,
    description TEXT,
    member_count INTEGER NOT NULL,
    centroid BLOB NOT NULL,
    fingerprint TEXT NOT NULL,
    label_source TEXT NOT NULL,
    inherited_from_cluster_id TEXT,
    created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS cluster_members (
    cluster_id TEXT NOT NULL REFERENCES clusters(id) ON DELETE CASCADE,
    document_id TEXT NOT NULL,
    membership_probability REAL NOT NULL,
    is_representative INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (cluster_id, document_id)
);

-- =====================================================================
-- Chat starters, references, study
-- =====================================================================

-- Cache only: one row per corpus fingerprint.
CREATE TABLE IF NOT EXISTS chat_starter_cache (
    fingerprint TEXT PRIMARY KEY,
    starters_json TEXT NOT NULL,
    created_at TEXT NOT NULL
);

-- A saved excerpt from a document, with enough locator information to reopen
-- the source at the right place. `document_id` is intentionally NOT a foreign
-- key: a reference outlives the removal of a document from the index.
CREATE TABLE IF NOT EXISTS passage_references (
    id TEXT PRIMARY KEY,
    document_id TEXT NOT NULL,
    chunk_id TEXT,
    file_path TEXT NOT NULL,
    file_name TEXT NOT NULL,
    locator TEXT,
    text TEXT NOT NULL,
    title TEXT,
    note TEXT,
    created_at TEXT NOT NULL
);

CREATE TABLE study_decks (
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    focus TEXT NOT NULL,
    study_goal TEXT NOT NULL DEFAULT '',
    model_name TEXT NOT NULL,
    created_at INTEGER NOT NULL
) STRICT;

CREATE TABLE study_cards (
    id TEXT PRIMARY KEY,
    deck_id TEXT NOT NULL REFERENCES study_decks(id) ON DELETE CASCADE,
    question TEXT NOT NULL,
    answer TEXT NOT NULL,
    options_json TEXT NOT NULL,
    correct_index INTEGER NOT NULL CHECK(correct_index BETWEEN 0 AND 4),
    explanation TEXT NOT NULL,
    source_json TEXT NOT NULL,
    topic TEXT NOT NULL,
    due_at INTEGER NOT NULL,
    interval_days INTEGER NOT NULL DEFAULT 0,
    review_count INTEGER NOT NULL DEFAULT 0,
    lapses INTEGER NOT NULL DEFAULT 0
) STRICT;

CREATE TABLE study_reviews (
    id TEXT PRIMARY KEY,
    card_id TEXT NOT NULL REFERENCES study_cards(id) ON DELETE CASCADE,
    reviewed_at INTEGER NOT NULL,
    rating TEXT NOT NULL CHECK(rating IN ('again','hard','good','easy')),
    mode TEXT NOT NULL CHECK(mode IN ('flashcard','quiz')),
    correct INTEGER,
    selected_option INTEGER
) STRICT;

-- =====================================================================
-- Learned sparse retrieval and summary overlays
-- =====================================================================

-- Per-chunk term weights from a model's sparse head. Term ids are positions in
-- one model's tokenizer vocabulary, so every read filters on `model_identity`.
CREATE TABLE IF NOT EXISTS chunk_sparse_terms (
    chunk_id TEXT NOT NULL REFERENCES text_chunks(id) ON DELETE CASCADE,
    model_identity TEXT NOT NULL,
    term_id INTEGER NOT NULL,
    weight REAL NOT NULL,
    PRIMARY KEY (chunk_id, model_identity, term_id)
);

-- RAPTOR-lite collection-level summary overlay: one short summary per document
-- plus one per top-level section, embedded into their own vector index.
CREATE TABLE IF NOT EXISTS document_summaries (
    id TEXT PRIMARY KEY,
    document_id TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    section TEXT,
    level TEXT NOT NULL CHECK(level IN ('document', 'section')),
    summary_text TEXT NOT NULL,
    model_identity TEXT NOT NULL,
    created_at TEXT NOT NULL
);

-- =====================================================================
-- Schema version tracking
-- =====================================================================

CREATE TABLE IF NOT EXISTS schema_version (
    version INTEGER PRIMARY KEY,
    applied_at INTEGER NOT NULL DEFAULT (unixepoch()),
    description TEXT
);

INSERT OR REPLACE INTO schema_version (version, description)
VALUES (16, 'Unified schema with conversations, model downloads, and memory vectors');

-- =====================================================================
-- Indexes
-- =====================================================================

-- Documents
CREATE INDEX IF NOT EXISTS idx_documents_file_path ON documents(file_path);
CREATE INDEX IF NOT EXISTS idx_documents_filename ON documents(file_name);
CREATE INDEX IF NOT EXISTS idx_documents_status ON documents(status);
CREATE INDEX IF NOT EXISTS idx_documents_checksum ON documents(checksum);
CREATE INDEX IF NOT EXISTS idx_documents_updated_desc ON documents(updated_at DESC);
CREATE INDEX IF NOT EXISTS idx_documents_indexed ON documents(indexed_at DESC);
CREATE INDEX IF NOT EXISTS idx_documents_status_updated ON documents(status, updated_at DESC);
CREATE INDEX IF NOT EXISTS idx_documents_source_type ON documents(source_type);
CREATE INDEX IF NOT EXISTS idx_documents_canonical_url ON documents(canonical_url) WHERE canonical_url IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_documents_web ON documents(source_type, indexed_at DESC) WHERE source_type = 'web';
CREATE INDEX IF NOT EXISTS idx_documents_language ON documents(language);
CREATE INDEX IF NOT EXISTS idx_documents_category ON documents(category);
CREATE INDEX IF NOT EXISTS idx_documents_quality_score ON documents(quality_score DESC);
CREATE INDEX IF NOT EXISTS idx_documents_access_count ON documents(access_count DESC);
CREATE INDEX IF NOT EXISTS idx_documents_last_accessed ON documents(last_accessed_at DESC);
CREATE INDEX IF NOT EXISTS idx_documents_category_quality ON documents(category, quality_score DESC);
CREATE INDEX IF NOT EXISTS idx_document_source_group ON documents(json_extract(source_context, '$.group.id'));

-- Chunks
CREATE INDEX IF NOT EXISTS idx_chunks_document ON text_chunks(document_id);
CREATE INDEX IF NOT EXISTS idx_chunks_composite ON text_chunks(document_id, chunk_index);
CREATE INDEX IF NOT EXISTS idx_chunks_content_length ON text_chunks(length(content));
CREATE INDEX IF NOT EXISTS idx_chunks_start_char ON text_chunks(document_id, start_char);
CREATE INDEX IF NOT EXISTS idx_text_chunks_has_code ON text_chunks(has_code);
CREATE INDEX IF NOT EXISTS idx_text_chunks_word_count ON text_chunks(word_count);
CREATE INDEX IF NOT EXISTS idx_text_chunks_section ON text_chunks(section);
CREATE INDEX IF NOT EXISTS idx_text_chunks_language_code ON text_chunks(language, has_code);
CREATE INDEX IF NOT EXISTS idx_chunk_section_order ON text_chunks(document_id, section, chunk_index);

-- Embeddings
CREATE INDEX IF NOT EXISTS idx_embeddings_chunk ON text_embeddings(chunk_id);
CREATE INDEX IF NOT EXISTS idx_embeddings_chunk_model ON text_embeddings(chunk_id, model_name);
CREATE INDEX IF NOT EXISTS idx_image_embeddings_document ON image_embeddings(document_id);

-- Tags and mentions
CREATE INDEX IF NOT EXISTS idx_tags_name ON tags(name);
CREATE INDEX IF NOT EXISTS idx_tags_name_lower ON tags(LOWER(name));
CREATE INDEX IF NOT EXISTS idx_document_tags_document_id ON document_tags(document_id);
CREATE INDEX IF NOT EXISTS idx_document_tags_tag_id ON document_tags(tag_id);
CREATE INDEX IF NOT EXISTS idx_document_tags_composite ON document_tags(document_id, tag_id);
CREATE INDEX IF NOT EXISTS idx_mentions_name ON mentions(name);
CREATE INDEX IF NOT EXISTS idx_mentions_type ON mentions(type);
CREATE INDEX IF NOT EXISTS idx_mentions_type_name ON mentions(type, name);
CREATE INDEX IF NOT EXISTS idx_document_mentions_document_id ON document_mentions(document_id);
CREATE INDEX IF NOT EXISTS idx_document_mentions_mention_id ON document_mentions(mention_id);
CREATE INDEX IF NOT EXISTS idx_document_mentions_composite ON document_mentions(document_id, mention_id);

-- File storage
CREATE INDEX IF NOT EXISTS idx_watch_folders_path ON watch_folders(path);
CREATE INDEX IF NOT EXISTS idx_files_hash ON files(content_hash);
CREATE INDEX IF NOT EXISTS idx_files_mime ON files(mime_type);
CREATE INDEX IF NOT EXISTS idx_files_accessed ON files(accessed_at);
CREATE INDEX IF NOT EXISTS idx_files_extension ON files(file_extension);
CREATE INDEX IF NOT EXISTS idx_file_refs_file ON file_references(file_id);
CREATE INDEX IF NOT EXISTS idx_file_refs_doc ON file_references(document_id);

-- Favorites and recents
CREATE INDEX IF NOT EXISTS idx_favorites_document_id ON favorites(document_id);
CREATE INDEX IF NOT EXISTS idx_favorites_added_at ON favorites(added_at DESC);
CREATE INDEX IF NOT EXISTS idx_recent_documents_document_id ON recent_documents(document_id);
CREATE INDEX IF NOT EXISTS idx_recent_documents_last_accessed ON recent_documents(last_accessed_at DESC);
CREATE INDEX IF NOT EXISTS idx_recent_documents_access_count ON recent_documents(access_count DESC);

-- Conversations and spaces
CREATE INDEX IF NOT EXISTS idx_conversation_messages_conversation_id ON conversation_messages(conversation_id, created_at);
CREATE INDEX IF NOT EXISTS idx_conversation_messages_role ON conversation_messages(role);
CREATE INDEX IF NOT EXISTS idx_conversation_messages_status ON conversation_messages(conversation_id, status);
CREATE INDEX IF NOT EXISTS idx_conversations_updated_at ON conversations(updated_at DESC);
CREATE INDEX IF NOT EXISTS idx_conversation_documents_conversation_id ON conversation_documents(conversation_id);
CREATE INDEX IF NOT EXISTS idx_conversation_documents_document_id ON conversation_documents(document_id);
CREATE INDEX IF NOT EXISTS idx_conversation_summaries_conversation_id ON conversation_summaries(conversation_id);
CREATE INDEX IF NOT EXISTS idx_conversation_summaries_created_at ON conversation_summaries(created_at DESC);
CREATE INDEX IF NOT EXISTS idx_conversation_memory_vectors_conversation_id ON conversation_memory_vectors(conversation_id);
CREATE INDEX IF NOT EXISTS idx_conversation_memory_vectors_created_at ON conversation_memory_vectors(created_at DESC);
CREATE INDEX IF NOT EXISTS idx_conversation_spaces_sort ON conversation_spaces(sort_order, updated_at DESC);
CREATE INDEX IF NOT EXISTS idx_conversation_spaces_archived ON conversation_spaces(is_archived, updated_at DESC);
CREATE INDEX IF NOT EXISTS idx_conversations_space_updated ON conversations(space_id, updated_at DESC);
CREATE INDEX IF NOT EXISTS idx_conversations_saved ON conversations(space_id, is_saved, updated_at DESC);
CREATE INDEX IF NOT EXISTS idx_conversations_bookmarked ON conversations(space_id, is_bookmarked, updated_at DESC);
CREATE INDEX IF NOT EXISTS idx_conversations_pinned ON conversations(space_id, is_pinned, pinned_at DESC);
CREATE INDEX IF NOT EXISTS idx_conversations_archived ON conversations(space_id, is_archived, updated_at DESC);
CREATE INDEX IF NOT EXISTS idx_message_bookmarks_conversation ON conversation_message_bookmarks(conversation_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_message_bookmarks_message ON conversation_message_bookmarks(message_id);
CREATE INDEX IF NOT EXISTS idx_conversation_web_sources_conversation_added
    ON conversation_web_sources(conversation_id, added_at DESC);
CREATE INDEX IF NOT EXISTS idx_conversation_web_sources_normalized_url
    ON conversation_web_sources(normalized_url);

-- Space membership and collaborators
CREATE INDEX IF NOT EXISTS idx_document_space_memberships_space_doc
    ON document_space_memberships (space_id, document_id);
CREATE INDEX IF NOT EXISTS idx_document_space_memberships_document_space
    ON document_space_memberships (document_id, space_id);
CREATE INDEX IF NOT EXISTS idx_collaborator_profiles_display_name
    ON collaborator_profiles(display_name COLLATE NOCASE);
CREATE INDEX IF NOT EXISTS idx_conversation_space_members_space_role
    ON conversation_space_members(space_id, role);
CREATE INDEX IF NOT EXISTS idx_conversation_space_members_member_space
    ON conversation_space_members(member_id, space_id);

-- Journals
CREATE INDEX IF NOT EXISTS idx_journals_sort
    ON journals(sort_order, updated_at DESC);
CREATE INDEX IF NOT EXISTS idx_journals_archived
    ON journals(is_archived, updated_at DESC);
CREATE INDEX IF NOT EXISTS idx_journal_conversation_entries_space_created
    ON journal_conversation_entries (journal_space_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_journal_conversation_entries_conversation
    ON journal_conversation_entries (conversation_id, journal_space_id);

-- Daily notes
CREATE INDEX IF NOT EXISTS idx_daily_notes_workspace_updated_at
    ON daily_notes_workspace(updated_at DESC);

-- Batch jobs and web assets
CREATE INDEX IF NOT EXISTS idx_batch_jobs_status ON batch_jobs(status);
CREATE INDEX IF NOT EXISTS idx_batch_jobs_created_at ON batch_jobs(created_at DESC);
CREATE INDEX IF NOT EXISTS idx_batch_jobs_type_status ON batch_jobs(job_type, status);
CREATE INDEX IF NOT EXISTS idx_batch_job_items_job_id ON batch_job_items(job_id);
CREATE INDEX IF NOT EXISTS idx_batch_job_items_status ON batch_job_items(status);
CREATE INDEX IF NOT EXISTS idx_batch_job_items_job_status ON batch_job_items(job_id, status);
CREATE INDEX IF NOT EXISTS idx_batch_job_items_document_id ON batch_job_items(document_id) WHERE document_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_web_assets_document_id ON web_assets(document_id);
CREATE INDEX IF NOT EXISTS idx_web_assets_checksum ON web_assets(checksum);
CREATE INDEX IF NOT EXISTS idx_web_assets_url ON web_assets(url);

-- Models and downloads
CREATE INDEX IF NOT EXISTS idx_models_model_id ON models(model_id);
CREATE INDEX IF NOT EXISTS idx_models_active_chat ON models(is_active_for_chat) WHERE is_active_for_chat = 1;
CREATE INDEX IF NOT EXISTS idx_models_active_embedding ON models(is_active_for_embedding) WHERE is_active_for_embedding = 1;
CREATE INDEX IF NOT EXISTS idx_models_active_utility
    ON models(is_active_for_utility) WHERE is_active_for_utility = 1;
CREATE INDEX IF NOT EXISTS idx_models_backend ON models(backend);
CREATE INDEX IF NOT EXISTS idx_model_files_model_id ON model_files(model_id);
CREATE INDEX IF NOT EXISTS idx_model_files_status ON model_files(status);
CREATE UNIQUE INDEX IF NOT EXISTS idx_model_files_unique ON model_files(model_id, file_name);
CREATE INDEX IF NOT EXISTS idx_download_sessions_state ON download_sessions(state);
CREATE INDEX IF NOT EXISTS idx_download_sessions_created_at ON download_sessions(created_at);
CREATE INDEX IF NOT EXISTS idx_download_sessions_url ON download_sessions(url);
CREATE INDEX IF NOT EXISTS idx_download_sessions_model_id ON download_sessions(model_id);

-- Clustering overlay
CREATE INDEX IF NOT EXISTS idx_cluster_runs_ran_at ON cluster_runs(ran_at DESC);
CREATE INDEX IF NOT EXISTS idx_clusters_run_id ON clusters(run_id);
CREATE INDEX IF NOT EXISTS idx_clusters_fingerprint ON clusters(fingerprint);
CREATE INDEX IF NOT EXISTS idx_cluster_members_doc ON cluster_members(document_id);

-- Chat starters, references, study
CREATE INDEX IF NOT EXISTS idx_chat_starter_cache_created_at
    ON chat_starter_cache(created_at DESC);
CREATE INDEX IF NOT EXISTS idx_passage_references_created_at
    ON passage_references(created_at DESC);
CREATE INDEX IF NOT EXISTS idx_passage_references_document_id
    ON passage_references(document_id);
CREATE INDEX IF NOT EXISTS idx_passage_references_chunk_id
    ON passage_references(chunk_id);
CREATE INDEX study_cards_deck_due ON study_cards(deck_id, due_at);
CREATE INDEX study_reviews_card ON study_reviews(card_id);

-- Sparse terms and summaries
-- The query path walks in from the term side: a query activates a few dozen
-- terms and needs every chunk that shares one.
CREATE INDEX IF NOT EXISTS idx_chunk_sparse_terms_lookup
    ON chunk_sparse_terms(model_identity, term_id);
CREATE INDEX IF NOT EXISTS idx_document_summaries_document
    ON document_summaries(document_id, level);
CREATE INDEX IF NOT EXISTS idx_document_summaries_identity
    ON document_summaries(model_identity);
-- A document has at most one summary per (identity, level, section), so a
-- regeneration replaces rather than accumulates.
CREATE UNIQUE INDEX IF NOT EXISTS idx_document_summaries_unique
    ON document_summaries(document_id, model_identity, level, COALESCE(section, ''));

ANALYZE;
