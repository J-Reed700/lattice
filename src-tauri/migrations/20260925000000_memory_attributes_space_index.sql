-- Deleting a space cascades to its memory attributes by space_id. The only
-- index, idx_memory_attributes_scope, leads with scope, so that cascade (and
-- any lookup by space) scanned the whole table. Only space-scoped rows carry a
-- space_id, so the index is partial and stays small.
CREATE INDEX idx_memory_attributes_space
    ON conversation_memory_attributes(space_id)
    WHERE space_id IS NOT NULL;
