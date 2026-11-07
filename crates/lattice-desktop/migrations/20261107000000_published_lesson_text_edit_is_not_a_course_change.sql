-- A learner's edit of a sentence in a published lesson changes only that
-- section's body. It is not a change to the course content a lesson being
-- prepared was based on, so it leaves `content_revision` alone. Every other
-- update to a block, and every update to a block of a lesson not yet ready,
-- counts as before.
DROP TRIGGER learning_block_content_updated;
CREATE TRIGGER learning_block_content_updated AFTER UPDATE ON learning_blocks
WHEN NOT (
    OLD.lesson_id IS NEW.lesson_id
    AND OLD.ordinal IS NEW.ordinal
    AND OLD.kind IS NEW.kind
    AND OLD.title IS NEW.title
    AND OLD.source_ids_json IS NEW.source_ids_json
    AND OLD.rubric_json IS NEW.rubric_json
    AND EXISTS (SELECT 1 FROM learning_lessons WHERE id = NEW.lesson_id AND preparation = 'ready')
)
BEGIN
    UPDATE learning_programs SET content_revision = content_revision + 1
    WHERE id IN (SELECT program_id FROM learning_lessons WHERE id IN (OLD.lesson_id, NEW.lesson_id));
END;
