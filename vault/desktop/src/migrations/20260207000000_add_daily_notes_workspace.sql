-- Daily notes workspace persistence
-- Stores rich note composition data for the Daily Notes screen.

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

CREATE INDEX IF NOT EXISTS idx_daily_notes_workspace_updated_at
    ON daily_notes_workspace(updated_at DESC);
