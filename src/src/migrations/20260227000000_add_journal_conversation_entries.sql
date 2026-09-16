-- Journal entry membership table:
-- lets a journal reference conversations without changing conversation.space_id.

CREATE TABLE IF NOT EXISTS journal_conversation_entries (
    journal_space_id TEXT NOT NULL,
    conversation_id TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (journal_space_id, conversation_id),
    FOREIGN KEY (journal_space_id) REFERENCES conversation_spaces(id) ON DELETE CASCADE,
    FOREIGN KEY (conversation_id) REFERENCES conversations(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_journal_conversation_entries_space_created
    ON journal_conversation_entries (journal_space_id, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_journal_conversation_entries_conversation
    ON journal_conversation_entries (conversation_id, journal_space_id);

-- Backfill existing "moved into journal space" conversations so older data appears
-- in journals after switching to additive journal membership.
INSERT OR IGNORE INTO journal_conversation_entries (journal_space_id, conversation_id, created_at)
SELECT
    c.space_id,
    c.id,
    COALESCE(c.updated_at, CURRENT_TIMESTAMP)
FROM conversations c
INNER JOIN conversation_spaces s ON s.id = c.space_id
WHERE
    COALESCE(json_extract(s.tool_preferences_json, '$.spaceType'), json_extract(s.tool_preferences_json, '$.space_type')) = 'journal';
