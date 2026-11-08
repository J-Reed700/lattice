-- When the user last changed the section a teaching finding is about.
--
-- A finding whose note was written before that moment is shown as possibly out of date. A row recorded before this
-- column has no such moment.

ALTER TABLE learning_lesson_teaching_findings ADD COLUMN part_changed_at INTEGER;
