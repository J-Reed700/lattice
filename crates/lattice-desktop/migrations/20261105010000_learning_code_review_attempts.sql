-- Code review drills (docs/design/2026-11-04-code-review-drills.md, sections 1 and 4): a learner's attempt at a drill.
--
-- An attempt opens when a drill is taken from the bank, and the learner submits what they found and, when they wrote
-- one, their fix. A job grades it against the drill's key. A grader that gives no assessment leaves the attempt
-- `couldnt_grade` with its error, which a regrade tries again; it is never a score.

CREATE TABLE learning_drill_attempts (
    id TEXT PRIMARY KEY,
    drill_id TEXT NOT NULL REFERENCES learning_drills(id) ON DELETE CASCADE,
    mode TEXT NOT NULL CHECK(mode IN ('practice','interview','rapid')),
    started_at INTEGER NOT NULL,
    submitted_at INTEGER,
    -- As the client measured it.
    time_spent_ms INTEGER CHECK(time_spent_ms IS NULL OR time_spent_ms >= 0),
    -- The learner's own findings, and the drill's files as they left them when they wrote a fix.
    findings_json TEXT CHECK(findings_json IS NULL OR json_valid(findings_json)),
    fix_files_json TEXT CHECK(fix_files_json IS NULL OR json_valid(fix_files_json)),
    status TEXT NOT NULL CHECK(status IN ('open','submitted','graded','couldnt_grade')),
    -- The grade: the judge's assessment, the score's parts, the rating of each key issue's pattern as
    -- [{pattern, rating}] for the schedule, and what came of the fix for each key issue.
    assessment_json TEXT CHECK(assessment_json IS NULL OR json_valid(assessment_json)),
    score_json TEXT CHECK(score_json IS NULL OR json_valid(score_json)),
    ratings_json TEXT CHECK(ratings_json IS NULL OR json_valid(ratings_json)),
    fix_json TEXT CHECK(fix_json IS NULL OR json_valid(fix_json)),
    -- The model that graded, or tried to, as the server names it. Null when no model could be reached.
    grader_model TEXT,
    graded_at INTEGER,
    -- Why the grader gave no assessment.
    grading_error TEXT,
    -- How many times a grade was asked for again.
    regrades INTEGER NOT NULL DEFAULT 0 CHECK(regrades >= 0),
    CHECK((status = 'open') = (submitted_at IS NULL)),
    CHECK((status = 'open') = (findings_json IS NULL)),
    CHECK((status = 'open') = (time_spent_ms IS NULL)),
    CHECK(status <> 'open' OR fix_files_json IS NULL),
    CHECK((status = 'graded') = (assessment_json IS NOT NULL)),
    CHECK((status = 'graded') = (score_json IS NOT NULL)),
    CHECK((status = 'graded') = (ratings_json IS NOT NULL)),
    CHECK((status = 'graded') = (graded_at IS NOT NULL)),
    CHECK(status <> 'graded' OR grader_model IS NOT NULL),
    CHECK(status IN ('graded','couldnt_grade') OR grader_model IS NULL),
    CHECK(status <> 'graded' OR (fix_files_json IS NOT NULL) = (fix_json IS NOT NULL)),
    CHECK(status = 'graded' OR fix_json IS NULL),
    CHECK((status = 'couldnt_grade') = (grading_error IS NOT NULL))
) STRICT;

-- Deleting a drill finds its attempts.
CREATE INDEX learning_drill_attempts_drill ON learning_drill_attempts(drill_id);
