-- A teaching finding's summary: what is wrong, in plain words for the user.
--
-- `note` stays what the repairs read, written for the model, and is no longer shown. A row recorded before this
-- column has no summary; it is read back with one made from its note.

ALTER TABLE learning_lesson_teaching_findings ADD COLUMN summary TEXT;
