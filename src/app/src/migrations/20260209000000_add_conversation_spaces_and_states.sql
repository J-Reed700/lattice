-- Phase 1 foundation for conversation organization:
-- - Spaces (collection-like environments)
-- - Conversation saved/bookmarked/pinned/archived states
-- - Message-level bookmarks

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

INSERT OR IGNORE INTO conversation_spaces (
    id,
    name,
    description,
    sort_order
) VALUES (
    'space_general',
    'General',
    'Default space for all conversations',
    0
);

ALTER TABLE conversations ADD COLUMN space_id TEXT NOT NULL DEFAULT 'space_general';
ALTER TABLE conversations ADD COLUMN is_saved INTEGER NOT NULL DEFAULT 0 CHECK (is_saved IN (0, 1));
ALTER TABLE conversations ADD COLUMN is_bookmarked INTEGER NOT NULL DEFAULT 0 CHECK (is_bookmarked IN (0, 1));
ALTER TABLE conversations ADD COLUMN is_pinned INTEGER NOT NULL DEFAULT 0 CHECK (is_pinned IN (0, 1));
ALTER TABLE conversations ADD COLUMN is_archived INTEGER NOT NULL DEFAULT 0 CHECK (is_archived IN (0, 1));
ALTER TABLE conversations ADD COLUMN saved_at TEXT;
ALTER TABLE conversations ADD COLUMN bookmarked_at TEXT;
ALTER TABLE conversations ADD COLUMN pinned_at TEXT;
ALTER TABLE conversations ADD COLUMN archived_at TEXT;

UPDATE conversations
SET space_id = 'space_general'
WHERE space_id IS NULL OR TRIM(space_id) = '';

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

CREATE INDEX IF NOT EXISTS idx_conversation_spaces_sort ON conversation_spaces(sort_order, updated_at DESC);
CREATE INDEX IF NOT EXISTS idx_conversation_spaces_archived ON conversation_spaces(is_archived, updated_at DESC);
CREATE INDEX IF NOT EXISTS idx_conversations_space_updated ON conversations(space_id, updated_at DESC);
CREATE INDEX IF NOT EXISTS idx_conversations_saved ON conversations(space_id, is_saved, updated_at DESC);
CREATE INDEX IF NOT EXISTS idx_conversations_bookmarked ON conversations(space_id, is_bookmarked, updated_at DESC);
CREATE INDEX IF NOT EXISTS idx_conversations_pinned ON conversations(space_id, is_pinned, pinned_at DESC);
CREATE INDEX IF NOT EXISTS idx_conversations_archived ON conversations(space_id, is_archived, updated_at DESC);
CREATE INDEX IF NOT EXISTS idx_message_bookmarks_conversation ON conversation_message_bookmarks(conversation_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_message_bookmarks_message ON conversation_message_bookmarks(message_id);
