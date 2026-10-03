-- Explorer threads are conversations bound to a folder on disk. The column
-- holds the canonical absolute path of that folder; NULL for every other
-- conversation. Explorer lists its threads per folder, so the lookup is indexed.
ALTER TABLE conversations ADD COLUMN explorer_root TEXT;

CREATE INDEX IF NOT EXISTS idx_conversations_explorer_root
    ON conversations(explorer_root, updated_at DESC)
    WHERE explorer_root IS NOT NULL;
