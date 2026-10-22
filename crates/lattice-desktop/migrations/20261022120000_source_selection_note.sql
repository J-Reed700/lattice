-- Why the research planner selected a source: the reason it gave with the
-- selection ("which gap each selection can resolve, including its scope and
-- authority"). The source-standing-v2 judgement reads it as how the course
-- obtained the page, a selection rather than a verdict on standing
-- (docs/design/2026-10-21-claim-official-sources.md §4.4). Existing rows keep
-- NULL: the judgement sees who added them without a note. Nothing converts.
ALTER TABLE learning_source_library ADD COLUMN selection_note TEXT NULL;
