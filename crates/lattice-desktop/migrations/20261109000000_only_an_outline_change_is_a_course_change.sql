-- `learning_programs.content_revision` is now the outline revision. It moves
-- when a module or lesson of the outline is added, removed or moved, or when a
-- field the lesson writer is given changes: the program's goal and prior
-- knowledge, a module's fields, a lesson's title, objective and minutes.
-- Preparing, completing or editing a lesson, its blocks, questions and claim
-- rows are no change to the outline, so lessons queued together can each be
-- prepared and published in turn.
DROP TRIGGER IF EXISTS learning_program_content_changed;
DROP TRIGGER IF EXISTS learning_module_content_inserted;
DROP TRIGGER IF EXISTS learning_module_content_updated;
DROP TRIGGER IF EXISTS learning_module_content_deleted;
DROP TRIGGER IF EXISTS learning_lesson_content_inserted;
DROP TRIGGER IF EXISTS learning_lesson_content_updated;
DROP TRIGGER IF EXISTS learning_lesson_content_deleted;
DROP TRIGGER IF EXISTS learning_block_content_inserted;
DROP TRIGGER IF EXISTS learning_block_content_updated;
DROP TRIGGER IF EXISTS learning_block_content_deleted;
DROP TRIGGER IF EXISTS learning_question_content_inserted;
DROP TRIGGER IF EXISTS learning_question_content_updated;
DROP TRIGGER IF EXISTS learning_question_content_deleted;
DROP TRIGGER IF EXISTS learning_answer_content_inserted;
DROP TRIGGER IF EXISTS learning_answer_content_updated;
DROP TRIGGER IF EXISTS learning_answer_content_deleted;

CREATE TRIGGER learning_program_content_changed
AFTER UPDATE OF goal, prior_knowledge ON learning_programs
BEGIN
    UPDATE learning_programs SET content_revision = content_revision + 1 WHERE id = NEW.id;
END;

CREATE TRIGGER learning_module_content_inserted AFTER INSERT ON learning_modules
BEGIN
    UPDATE learning_programs SET content_revision = content_revision + 1 WHERE id = NEW.program_id;
END;
CREATE TRIGGER learning_module_content_updated AFTER UPDATE ON learning_modules
BEGIN
    UPDATE learning_programs SET content_revision = content_revision + 1 WHERE id IN (OLD.program_id, NEW.program_id);
END;
CREATE TRIGGER learning_module_content_deleted AFTER DELETE ON learning_modules
BEGIN
    UPDATE learning_programs SET content_revision = content_revision + 1 WHERE id = OLD.program_id;
END;

CREATE TRIGGER learning_lesson_content_inserted AFTER INSERT ON learning_lessons
BEGIN
    UPDATE learning_programs SET content_revision = content_revision + 1 WHERE id = NEW.program_id;
END;
CREATE TRIGGER learning_lesson_content_updated
AFTER UPDATE OF program_id, module_id, ordinal, title, objective, estimated_minutes ON learning_lessons
BEGIN
    UPDATE learning_programs SET content_revision = content_revision + 1 WHERE id IN (OLD.program_id, NEW.program_id);
END;
CREATE TRIGGER learning_lesson_content_deleted AFTER DELETE ON learning_lessons
BEGIN
    UPDATE learning_programs SET content_revision = content_revision + 1 WHERE id = OLD.program_id;
END;
