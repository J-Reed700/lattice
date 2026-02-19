-- Migration: Add conversation support for LLM interactions
-- Version: 008
-- Description: Creates tables for managing conversation history and context

-- Conversations table: stores conversation metadata
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

-- Conversation messages table: stores individual messages in a conversation
CREATE TABLE IF NOT EXISTS conversation_messages (
    id TEXT PRIMARY KEY,
    conversation_id TEXT NOT NULL,
    role TEXT NOT NULL CHECK(role IN ('user', 'assistant', 'system')),
    content TEXT NOT NULL,
    tokens INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    metadata TEXT, -- JSON field for additional data (search results, etc.)
    FOREIGN KEY (conversation_id) REFERENCES conversations(id) ON DELETE CASCADE
);

-- Conversation documents: tracks which documents are in the conversation context
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

-- Indexes for performance
CREATE INDEX IF NOT EXISTS idx_conversation_messages_conversation_id
    ON conversation_messages(conversation_id, created_at);

CREATE INDEX IF NOT EXISTS idx_conversation_messages_role
    ON conversation_messages(role);

CREATE INDEX IF NOT EXISTS idx_conversations_updated_at
    ON conversations(updated_at DESC);

CREATE INDEX IF NOT EXISTS idx_conversation_documents_conversation_id
    ON conversation_documents(conversation_id);

CREATE INDEX IF NOT EXISTS idx_conversation_documents_document_id
    ON conversation_documents(document_id);

-- Update schema version
INSERT OR REPLACE INTO schema_version (version, description)
VALUES (8, 'Add conversation support for LLM interactions');
