-- Whether a lesson of a course teaches something code can show, as the code fit check judged it
-- (docs/design/2026-11-07-drills-in-the-course.md, section 3). A lesson is judged once; a verdict made on another
-- basis (the check's text and the lesson's module, title and objective) is asked again.

CREATE TABLE learning_drill_code_checks (
    program_id TEXT NOT NULL REFERENCES learning_programs(id) ON DELETE CASCADE,
    lesson_id TEXT NOT NULL,
    basis TEXT NOT NULL,
    teaches INTEGER NOT NULL CHECK(teaches IN (0,1)),
    reason TEXT NOT NULL,
    model TEXT NOT NULL,
    checked_at INTEGER NOT NULL,
    PRIMARY KEY(program_id, lesson_id),
    FOREIGN KEY(program_id, lesson_id) REFERENCES learning_lessons(program_id, id) ON DELETE CASCADE
) STRICT;
