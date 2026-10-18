-- Learner progress is independent of the teaching inputs to a long-running job.
-- Content writes advance this counter in the same transaction as the edit.
ALTER TABLE learning_programs ADD COLUMN content_revision INTEGER NOT NULL DEFAULT 0;
ALTER TABLE learning_generation_jobs ADD COLUMN content_revision INTEGER;

-- An existing job has a provable baseline only if its original revision still
-- matches. A previously changed course cannot inherit an assumed approval.
UPDATE learning_generation_jobs
SET content_revision = (SELECT p.content_revision FROM learning_programs p
    WHERE p.id = learning_generation_jobs.program_id)
WHERE EXISTS (SELECT 1 FROM learning_programs p
    WHERE p.id = learning_generation_jobs.program_id
      AND p.revision = learning_generation_jobs.base_revision_number);

CREATE TRIGGER learning_program_content_changed
AFTER UPDATE OF title, goal, prior_knowledge, minutes_per_session ON learning_programs
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
AFTER UPDATE OF program_id, module_id, ordinal, title, objective, estimated_minutes, preparation, curriculum_state ON learning_lessons
BEGIN
    UPDATE learning_programs SET content_revision = content_revision + 1 WHERE id IN (OLD.program_id, NEW.program_id);
END;
CREATE TRIGGER learning_lesson_content_deleted AFTER DELETE ON learning_lessons
BEGIN
    UPDATE learning_programs SET content_revision = content_revision + 1 WHERE id = OLD.program_id;
END;

CREATE TRIGGER learning_block_content_inserted AFTER INSERT ON learning_blocks
BEGIN
    UPDATE learning_programs SET content_revision = content_revision + 1
    WHERE id = (SELECT program_id FROM learning_lessons WHERE id = NEW.lesson_id);
END;
CREATE TRIGGER learning_block_content_updated AFTER UPDATE ON learning_blocks
BEGIN
    UPDATE learning_programs SET content_revision = content_revision + 1
    WHERE id IN (SELECT program_id FROM learning_lessons WHERE id IN (OLD.lesson_id, NEW.lesson_id));
END;
CREATE TRIGGER learning_block_content_deleted AFTER DELETE ON learning_blocks
BEGIN
    UPDATE learning_programs SET content_revision = content_revision + 1
    WHERE id = (SELECT program_id FROM learning_lessons WHERE id = OLD.lesson_id);
END;

CREATE TRIGGER learning_question_content_inserted AFTER INSERT ON learning_questions
BEGIN
    UPDATE learning_programs SET content_revision = content_revision + 1
    WHERE id IN (SELECT program_id FROM learning_lessons WHERE id = NEW.lesson_id);
END;
CREATE TRIGGER learning_question_content_updated AFTER UPDATE ON learning_questions
BEGIN
    UPDATE learning_programs SET content_revision = content_revision + 1
    WHERE id IN (SELECT program_id FROM learning_lessons WHERE id IN (OLD.lesson_id, NEW.lesson_id));
END;
CREATE TRIGGER learning_question_content_deleted AFTER DELETE ON learning_questions
BEGIN
    UPDATE learning_programs SET content_revision = content_revision + 1
    WHERE id = (SELECT program_id FROM learning_lessons WHERE id = OLD.lesson_id);
END;

CREATE TRIGGER learning_answer_content_inserted AFTER INSERT ON learning_answer_keys
BEGIN
    UPDATE learning_programs SET content_revision = content_revision + 1
    WHERE id IN (SELECT l.program_id FROM learning_questions q JOIN learning_lessons l ON l.id = q.lesson_id WHERE q.id = NEW.question_id);
END;
CREATE TRIGGER learning_answer_content_updated AFTER UPDATE ON learning_answer_keys
BEGIN
    UPDATE learning_programs SET content_revision = content_revision + 1
    WHERE id IN (SELECT l.program_id FROM learning_questions q JOIN learning_lessons l ON l.id = q.lesson_id WHERE q.id IN (OLD.question_id, NEW.question_id));
END;
CREATE TRIGGER learning_answer_content_deleted AFTER DELETE ON learning_answer_keys
BEGIN
    UPDATE learning_programs SET content_revision = content_revision + 1
    WHERE id IN (SELECT l.program_id FROM learning_questions q JOIN learning_lessons l ON l.id = q.lesson_id WHERE q.id = OLD.question_id);
END;
