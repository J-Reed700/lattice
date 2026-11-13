-- The entries a lesson's course plan held before the learner changed it by hand or asked for a change
-- (docs/design/2026-11-08-course-preparation.md, piece 2). A stack per lesson: a change adds the entry it replaced,
-- tagged with the entry_revision that entry had, and an undo takes the newest one back out.

CREATE TABLE learning_course_plan_entry_history (
    program_id TEXT NOT NULL REFERENCES learning_programs(id) ON DELETE CASCADE,
    outline_revision INTEGER NOT NULL CHECK(outline_revision >= 0),
    lesson_id TEXT NOT NULL,
    entry_revision INTEGER NOT NULL CHECK(entry_revision >= 1),
    entry_json TEXT NOT NULL CHECK(json_valid(entry_json)),
    replaced_at INTEGER NOT NULL,
    PRIMARY KEY(program_id, outline_revision, lesson_id, entry_revision)
) STRICT;
