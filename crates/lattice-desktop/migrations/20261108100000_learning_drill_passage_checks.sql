-- Whether a passage of a course's source teaches a lesson of the course, as the passage check judged it
-- (docs/design/2026-11-07-drills-in-the-course.md, section 4). A passage is judged once for a lesson; a verdict made on
-- another basis (the check's text and the lesson's module, title and objective) is asked again.

CREATE TABLE learning_drill_passage_checks (
    program_id TEXT NOT NULL REFERENCES learning_programs(id) ON DELETE CASCADE,
    lesson_id TEXT NOT NULL,
    version_id TEXT NOT NULL,
    start_byte INTEGER NOT NULL CHECK(start_byte >= 0),
    end_byte INTEGER NOT NULL CHECK(end_byte > start_byte),
    basis TEXT NOT NULL,
    teaches INTEGER NOT NULL CHECK(teaches IN (0,1)),
    reason TEXT NOT NULL,
    model TEXT NOT NULL,
    checked_at INTEGER NOT NULL,
    PRIMARY KEY(program_id, lesson_id, version_id, start_byte, end_byte),
    FOREIGN KEY(program_id, lesson_id) REFERENCES learning_lessons(program_id, id) ON DELETE CASCADE,
    FOREIGN KEY(program_id, version_id) REFERENCES learning_source_versions(program_id, id) ON DELETE CASCADE
) STRICT;

CREATE INDEX learning_drill_passage_checks_version ON learning_drill_passage_checks(program_id, version_id);
