-- Review drills in the course (docs/design/2026-11-07-drills-in-the-course.md, sections 5 and 7): every drill is
-- built from one lesson of a course.
--
-- A drill keeps the lesson it was built from, and drills from before have none. The shelf index replaces the bank's
-- older indexes by language, by course, by program and by domain. Each pattern of a material and each lesson of a
-- course has a schedule, written once a review has rated it, and a lesson records whether it can be drilled in code
-- or in passages, with the basis the decision rested on.

ALTER TABLE learning_drills ADD COLUMN lesson_id TEXT REFERENCES learning_lessons(id) ON DELETE SET NULL;

DROP INDEX learning_drills_bank;
DROP INDEX learning_drills_course_bank;
DROP INDEX learning_drills_program;
DROP INDEX learning_drills_domains;

CREATE INDEX learning_drills_shelf ON learning_drills(program_id, material, language, status, created_at);
CREATE INDEX learning_drills_lesson ON learning_drills(lesson_id) WHERE lesson_id IS NOT NULL;

CREATE TABLE learning_drill_pattern_schedules (
    material TEXT NOT NULL CHECK(material = 'passage' OR material GLOB 'code:?*'),
    pattern_id TEXT NOT NULL,
    stability REAL NOT NULL,
    difficulty REAL NOT NULL,
    last_reviewed_at INTEGER NOT NULL,
    due_at INTEGER NOT NULL,
    interval_days INTEGER NOT NULL CHECK(interval_days >= 0),
    review_count INTEGER NOT NULL CHECK(review_count >= 1),
    lapses INTEGER NOT NULL CHECK(lapses >= 0),
    PRIMARY KEY(material, pattern_id)
) STRICT;

CREATE TABLE learning_drill_lesson_schedules (
    program_id TEXT NOT NULL REFERENCES learning_programs(id) ON DELETE CASCADE,
    lesson_id TEXT NOT NULL,
    stability REAL NOT NULL,
    difficulty REAL NOT NULL,
    last_reviewed_at INTEGER NOT NULL,
    due_at INTEGER NOT NULL,
    interval_days INTEGER NOT NULL CHECK(interval_days >= 0),
    review_count INTEGER NOT NULL CHECK(review_count >= 1),
    lapses INTEGER NOT NULL CHECK(lapses >= 0),
    PRIMARY KEY(program_id, lesson_id),
    FOREIGN KEY(program_id, lesson_id) REFERENCES learning_lessons(program_id, id) ON DELETE CASCADE
) STRICT;

CREATE INDEX learning_drill_lesson_schedules_lesson ON learning_drill_lesson_schedules(lesson_id);

CREATE TABLE learning_drill_lesson_fits (
    program_id TEXT NOT NULL REFERENCES learning_programs(id) ON DELETE CASCADE,
    lesson_id TEXT NOT NULL,
    material TEXT NOT NULL CHECK(material IN ('code','passage')),
    fits INTEGER NOT NULL CHECK(fits IN (0,1)),
    basis TEXT NOT NULL,
    decided_at INTEGER NOT NULL,
    PRIMARY KEY(program_id, lesson_id, material),
    FOREIGN KEY(program_id, lesson_id) REFERENCES learning_lessons(program_id, id) ON DELETE CASCADE
) STRICT;

CREATE INDEX learning_drill_lesson_fits_lesson ON learning_drill_lesson_fits(lesson_id);
