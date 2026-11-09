-- What each lesson of a course teaches, named as topics and made once per outline revision before any lesson is written
-- (docs/design/2026-11-08-course-preparation.md, piece 2). Lessons are written against this plan, never against each
-- other's output. One row per lesson, so one lesson's entry can be replaced without touching the others; a revision is
-- planned when every one of its lessons has a row. entry_revision counts the replacements of an entry.

CREATE TABLE learning_course_plan_lessons (
    program_id TEXT NOT NULL REFERENCES learning_programs(id) ON DELETE CASCADE,
    outline_revision INTEGER NOT NULL CHECK(outline_revision >= 0),
    lesson_id TEXT NOT NULL,
    entry_json TEXT NOT NULL CHECK(json_valid(entry_json)),
    entry_revision INTEGER NOT NULL DEFAULT 1 CHECK(entry_revision >= 1),
    created_at INTEGER NOT NULL,
    PRIMARY KEY(program_id, outline_revision, lesson_id)
) STRICT;
