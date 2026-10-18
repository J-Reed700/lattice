-- Widen conversation_memory_items.kind to admit 'established_fact': a fact the
-- conversation established from an explicit source (a cited page, a named
-- document, a URL), kept so later turns can rely on it without fetching the
-- source again.
--
-- SQLite cannot alter a CHECK constraint, so the table is rebuilt. The
-- referenced table is never renamed: with foreign-key enforcement on, RENAME
-- rewrites child REFERENCES clauses to the temporary name, which is exactly
-- the state this rebuild must not leave behind. The replacement is built
-- beside the original and swapped in. Two steps need care:
--
-- * FK checks are deferred to commit, so a superseded row never lands before
--   its replacement and the drop of the old parent is validated against the
--   new one.
-- * The memory-invalidation triggers name the table in their bodies, and
--   RENAME re-parses every trigger — they are dropped first and recreated
--   verbatim afterwards so no statement ever runs with the name missing.
--
-- Ids are preserved verbatim, so evidence spans, supersession links and event
-- logs are untouched.
PRAGMA defer_foreign_keys = ON;

DROP TRIGGER trg_conversation_memory_invalidate_au;
DROP TRIGGER trg_conversation_memory_invalidate_ad;

CREATE TABLE conversation_memory_items_new (
    id TEXT PRIMARY KEY,
    conversation_id TEXT NOT NULL,
    kind TEXT NOT NULL CHECK(kind IN (
        'constraint', 'goal', 'decision', 'user_fact',
        'preference', 'open_question', 'established_fact',
        'unresolved_change')),
    state TEXT NOT NULL DEFAULT 'active'
        CHECK(state IN ('active', 'superseded', 'resolved')),
    -- Generated text for search and display. Renderers mark it as a label so
    -- it cannot be mistaken for something the user wrote.
    label TEXT NOT NULL,
    created_at_sequence INTEGER NOT NULL,
    changed_at_sequence INTEGER NOT NULL,
    superseded_by TEXT,
    revision INTEGER NOT NULL DEFAULT 0,
    review TEXT NOT NULL DEFAULT 'supported'
        CHECK(review IN ('supported', 'ambiguous')),
    -- JSON array of memory ids only, never memory text. Bounded and validated
    -- in Rust inside the same transaction that writes it.
    related_item_ids TEXT NOT NULL DEFAULT '[]',
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (conversation_id) REFERENCES conversations(id) ON DELETE CASCADE,
    -- A replacement that is deleted leaves the pointer null rather than taking
    -- the item it replaced with it. Written against the canonical name: once
    -- the swap below completes, this resolves to this very table.
    FOREIGN KEY (superseded_by) REFERENCES conversation_memory_items(id) ON DELETE SET NULL
);

INSERT INTO conversation_memory_items_new (
    id, conversation_id, kind, state, label,
    created_at_sequence, changed_at_sequence, superseded_by,
    revision, review, related_item_ids, created_at, updated_at)
SELECT
    id, conversation_id, kind, state, label,
    created_at_sequence, changed_at_sequence, superseded_by,
    revision, review, related_item_ids, created_at, updated_at
FROM conversation_memory_items;

DROP TABLE conversation_memory_items;

ALTER TABLE conversation_memory_items_new RENAME TO conversation_memory_items;

CREATE INDEX IF NOT EXISTS idx_conversation_memory_items_active
    ON conversation_memory_items(conversation_id, state, kind);

-- Recreated verbatim from the init schema (lines 660-704 of
-- 20260916000000_init_schema.sql); keep them in step if those ever change.
CREATE TRIGGER IF NOT EXISTS trg_conversation_memory_invalidate_au
AFTER UPDATE OF content, role ON conversation_messages
WHEN EXISTS (SELECT 1 FROM conversations WHERE id = NEW.conversation_id)
BEGIN
    DELETE FROM conversation_memory_items WHERE conversation_id = NEW.conversation_id;
    DELETE FROM conversation_memory_events WHERE conversation_id = NEW.conversation_id;
    DELETE FROM conversation_summaries WHERE conversation_id = NEW.conversation_id;
    INSERT INTO conversation_memory_state (
        conversation_id, schema_version, memory_revision,
        source_transcript_revision, processed_through_sequence,
        validity, last_error_code, updated_at)
    VALUES (NEW.conversation_id, 1, 1, 0, 0, 'rebuild_required',
            'source_rewritten', CURRENT_TIMESTAMP)
    ON CONFLICT(conversation_id) DO UPDATE SET
        memory_revision = memory_revision + 1,
        processed_through_sequence = 0,
        source_transcript_revision = 0,
        validity = 'rebuild_required',
        last_error_code = 'source_rewritten',
        updated_at = CURRENT_TIMESTAMP;
END;

CREATE TRIGGER IF NOT EXISTS trg_conversation_memory_invalidate_ad
AFTER DELETE ON conversation_messages
WHEN EXISTS (SELECT 1 FROM conversations WHERE id = OLD.conversation_id)
BEGIN
    DELETE FROM conversation_memory_items WHERE conversation_id = OLD.conversation_id;
    DELETE FROM conversation_memory_events WHERE conversation_id = OLD.conversation_id;
    DELETE FROM conversation_summaries WHERE conversation_id = OLD.conversation_id;
    INSERT INTO conversation_memory_state (
        conversation_id, schema_version, memory_revision,
        source_transcript_revision, processed_through_sequence,
        validity, last_error_code, updated_at)
    VALUES (OLD.conversation_id, 1, 1, 0, 0, 'rebuild_required',
            'source_deleted', CURRENT_TIMESTAMP)
    ON CONFLICT(conversation_id) DO UPDATE SET
        memory_revision = memory_revision + 1,
        processed_through_sequence = 0,
        source_transcript_revision = 0,
        validity = 'rebuild_required',
        last_error_code = 'source_deleted',
        updated_at = CURRENT_TIMESTAMP;
END;
