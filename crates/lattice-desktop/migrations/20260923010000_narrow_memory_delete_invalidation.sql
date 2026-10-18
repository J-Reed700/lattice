-- Deleting a message no longer wipes the whole memory ledger.
--
-- The old AFTER DELETE trigger cleared every item, event and summary of the
-- conversation on any message delete. Regenerate takes back the last turn by
-- deleting it, so every regenerate discarded memory the user had authored or
-- edited and forced a full rebuild.
--
-- Now:
-- * A delete that no extracted item cites leaves the ledger standing. The
--   watermark is clamped to the highest remaining sequence, and a summary that
--   covered the deleted message is dropped, since it may repeat its text.
-- * A delete that an extracted item cites still invalidates extraction: the
--   supersession chain those items took part in can no longer be trusted, so
--   the extracted ledger is cleared and rebuilt from the originals. Items the
--   user authored or edited survive it.
--
-- "Authored or edited by the user" is an item with a
-- `conversation_memory_attributes` row: only the user's remember, correct,
-- scope, dates, verify and forget actions (and the triggers that carry those
-- decisions forward) write one.
--
-- BEFORE DELETE because the evidence rows that say which items cite the
-- message are removed by the foreign-key cascade of the delete itself.
DROP TRIGGER trg_conversation_memory_invalidate_ad;

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

-- Runs after the invalidation above, which leaves nothing 'ready' to clamp.
CREATE TRIGGER trg_conversation_memory_clamp_ad
AFTER DELETE ON conversation_messages
WHEN EXISTS (SELECT 1 FROM conversations WHERE id = OLD.conversation_id)
BEGIN
    DELETE FROM conversation_summaries
    WHERE conversation_id = OLD.conversation_id
      AND OLD.sequence > 0
      AND OLD.sequence <= COALESCE(
          (SELECT m.sequence FROM conversation_messages m
           WHERE m.id = conversation_summaries.up_to_message_id), 0);
    UPDATE conversation_memory_state
    SET processed_through_sequence = MIN(
            processed_through_sequence,
            COALESCE(
                (SELECT MAX(sequence) FROM conversation_messages
                 WHERE conversation_id = OLD.conversation_id),
                0)),
        updated_at = CURRENT_TIMESTAMP
    WHERE conversation_id = OLD.conversation_id
      AND validity = 'ready'
      AND processed_through_sequence > COALESCE(
          (SELECT MAX(sequence) FROM conversation_messages
           WHERE conversation_id = OLD.conversation_id),
          0);
END;
