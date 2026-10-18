-- A tangent uses the ordinary transcript and generation pipeline, but belongs
-- to a conversation until explicitly promoted. Deleting a parent deletes its
-- tangents; deleting an anchor message leaves the captured passage intact.
ALTER TABLE conversations ADD COLUMN tangent_parent_id TEXT
    REFERENCES conversations(id) ON DELETE CASCADE;
ALTER TABLE conversations ADD COLUMN tangent_selection TEXT;
ALTER TABLE conversations ADD COLUMN tangent_context_message_count INTEGER NOT NULL DEFAULT 0;

CREATE INDEX idx_conversations_tangent_parent
    ON conversations(tangent_parent_id, updated_at DESC);
