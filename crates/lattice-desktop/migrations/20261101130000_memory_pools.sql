-- A chat shares memory with the chats in its pool: its space, project folder or
-- course (docs/design/2026-10-28-tutor.md §3.4). The sharing the user chose is
-- now named by kind and key, so a new kind of chat needs no migration.
--
-- scope_kind NULL means the user has not decided, and the item follows the rule
-- for its kind. Existing rows keep what they said, 'conversation' included: an
-- upgrade shares nothing the user kept to one chat.
--
-- SQLite cannot alter a check, so the table is rebuilt. The triggers naming it
-- are dropped first, because renaming the new table rechecks every trigger.
DROP TRIGGER trg_memory_scope_superseded;
DROP TRIGGER trg_memory_suppression_evidence;
DROP TRIGGER trg_conversation_memory_invalidate_bd;
DROP INDEX idx_memory_attributes_space;
DROP INDEX idx_memory_attributes_scope;

CREATE TABLE conversation_memory_attributes_new (
    item_id TEXT PRIMARY KEY REFERENCES conversation_memory_items(id) ON DELETE CASCADE,
    scope_kind TEXT CHECK(scope_kind IN ('conversation','space','folder','course','personal')),
    scope_key TEXT,
    valid_from TEXT,
    valid_until TEXT,
    verified_at TEXT,
    forgotten INTEGER NOT NULL DEFAULT 0 CHECK(forgotten IN (0,1)),
    CHECK ((COALESCE(scope_kind,'') IN ('space','folder','course')) = (scope_key IS NOT NULL)),
    CHECK (valid_from IS NULL OR valid_until IS NULL OR valid_from < valid_until)
);
INSERT INTO conversation_memory_attributes_new
    (item_id,scope_kind,scope_key,valid_from,valid_until,verified_at,forgotten)
SELECT item_id,scope,space_id,valid_from,valid_until,verified_at,forgotten
FROM conversation_memory_attributes;
DROP TABLE conversation_memory_attributes;
ALTER TABLE conversation_memory_attributes_new RENAME TO conversation_memory_attributes;

CREATE INDEX idx_memory_attributes_scope
    ON conversation_memory_attributes(scope_kind, scope_key, forgotten);

-- A deleted space or course takes the sharing with it. The row stays: it holds
-- the item's dates and says the user wrote or changed the item.
CREATE TRIGGER trg_memory_scope_space_deleted AFTER DELETE ON conversation_spaces
BEGIN
    UPDATE conversation_memory_attributes SET scope_kind=NULL,scope_key=NULL
    WHERE scope_kind='space' AND scope_key=OLD.id;
END;

CREATE TRIGGER trg_memory_scope_course_deleted AFTER DELETE ON learning_programs
BEGIN
    UPDATE conversation_memory_attributes SET scope_kind=NULL,scope_key=NULL
    WHERE scope_kind='course' AND scope_key=OLD.id;
END;

CREATE TRIGGER trg_memory_scope_superseded AFTER UPDATE OF superseded_by ON conversation_memory_items
WHEN NEW.superseded_by IS NOT NULL
BEGIN
    INSERT OR IGNORE INTO conversation_memory_attributes(item_id,scope_kind,scope_key,forgotten)
    SELECT NEW.superseded_by,scope_kind,scope_key,forgotten FROM conversation_memory_attributes WHERE item_id=NEW.id;
END;

CREATE TRIGGER trg_memory_suppression_evidence AFTER INSERT ON conversation_memory_evidence
WHEN NEW.purpose='assertion' AND EXISTS (
    SELECT 1 FROM conversation_memory_suppressions s WHERE s.message_id=NEW.message_id
      AND s.content_digest=NEW.content_digest AND s.start_byte<NEW.end_byte AND s.end_byte>NEW.start_byte
)
BEGIN
    INSERT INTO conversation_memory_attributes(item_id,scope_kind,forgotten) VALUES (NEW.item_id,'conversation',1)
    ON CONFLICT(item_id) DO UPDATE SET forgotten=1,scope_kind='conversation',scope_key=NULL;
    UPDATE conversation_memory_items SET state='resolved' WHERE id=NEW.item_id;
END;

CREATE TRIGGER trg_conversation_memory_invalidate_bd
BEFORE DELETE ON conversation_messages
WHEN EXISTS (SELECT 1 FROM conversations WHERE id = OLD.conversation_id)
 AND EXISTS (
    SELECT 1 FROM conversation_memory_evidence e
    WHERE e.message_id = OLD.id
      AND NOT EXISTS (
          SELECT 1 FROM conversation_memory_attributes a WHERE a.item_id = e.item_id)
 )
BEGIN
    DELETE FROM conversation_memory_items
    WHERE conversation_id = OLD.conversation_id
      AND NOT EXISTS (
          SELECT 1 FROM conversation_memory_attributes a
          WHERE a.item_id = conversation_memory_items.id);
    -- The user's own actions stay on record; they describe items that remain.
    DELETE FROM conversation_memory_events
    WHERE conversation_id = OLD.conversation_id AND operation NOT LIKE 'user\_%' ESCAPE '\';
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
