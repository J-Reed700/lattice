-- How a teaching finding's key identifies it.
--
-- 'place': the kind and the part (`teaching:section-3`), every row recorded before this column. While its part's text
-- is unchanged, such a row stands for every finding of its kind at its part.
-- 'finding': the finding's own id (`plan/used/section-3/5f0c9a1be2d44a07`), one row per finding.

ALTER TABLE learning_lesson_teaching_findings ADD COLUMN identity TEXT NOT NULL DEFAULT 'place' CHECK(identity IN ('place','finding'));
