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

CREATE INDEX IF NOT EXISTS idx_conversation_web_sources_conversation_added
    ON conversation_web_sources(conversation_id, added_at DESC);

CREATE INDEX IF NOT EXISTS idx_conversation_web_sources_normalized_url
    ON conversation_web_sources(normalized_url);
