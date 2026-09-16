-- Corpus-derived Chat starters (BRIEF rank 11, contract §4.7).
-- Cache only: one row per corpus fingerprint. Dropping the table costs one
-- utility-LLM call, never any user data.

CREATE TABLE IF NOT EXISTS chat_starter_cache (
    fingerprint TEXT PRIMARY KEY,
    starters_json TEXT NOT NULL,
    created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_chat_starter_cache_created_at
    ON chat_starter_cache(created_at DESC);
