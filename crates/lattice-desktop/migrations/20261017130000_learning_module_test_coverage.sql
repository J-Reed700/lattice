-- How much of its module a module test covered: the lessons whose questions it
-- asked, out of the module's lessons when it was taken. A module test may be
-- taken before every lesson is prepared. Practice and quiz attempts leave both NULL.
ALTER TABLE learning_attempts ADD COLUMN covered_lessons INTEGER;
ALTER TABLE learning_attempts ADD COLUMN module_lessons INTEGER;
