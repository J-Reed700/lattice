-- Quizzes and tests: the assessment engine becomes the only quiz system
-- (docs/design/2026-10-23-quizzes-and-tests.md, section 6.2).
--
-- CHECK lists and NOT NULL constraints change in place with SQLite's
-- documented procedure for constraint-only changes
-- (lang_altertable.html#otheralter), as 20261016100000 does: sqlx-sqlite runs
-- every migration inside a transaction where PRAGMA foreign_keys=OFF is a
-- no-op, so a copy/drop/rename rebuild would cascade-delete child rows on DROP
-- or fail on RESTRICT. Widening a CHECK list, and removing a CHECK or a NOT
-- NULL, never invalidates a stored row, so no row is copied or moved.

PRAGMA writable_schema = ON;

-- Item formats: multiple_choice is renamed single_choice, and five formats join.
UPDATE sqlite_schema
SET sql = replace(
    sql,
    '''multiple_choice'',''short_answer'',''explanation'',''ordering'',''artifact''',
    '''single_choice'',''multi_select'',''true_false'',''short_answer'',''cloze'',''matching'',''ordering'',''explanation'',''artifact'''
)
WHERE type = 'table' AND name IN (
    'learning_assessment_blueprint_slots',
    'learning_assessment_candidates',
    'learning_assessment_form_items'
);

-- Written answers wait for the grading job as 'pending'.
UPDATE sqlite_schema
SET sql = replace(
    sql,
    '''deterministic'',''provisional'',''uncertain'',''needs_review''',
    '''deterministic'',''provisional'',''uncertain'',''needs_review'',''pending'''
)
WHERE type = 'table' AND name IN (
    'learning_assessment_submissions',
    'learning_assessment_item_results'
);

-- A course exam's derived length may pass eight hours.
UPDATE sqlite_schema
SET sql = replace(
    sql,
    'expected_minutes INTEGER NOT NULL CHECK(expected_minutes BETWEEN 1 AND 480)',
    'expected_minutes INTEGER NOT NULL CHECK(expected_minutes >= 1)'
)
WHERE type = 'table' AND name IN (
    'learning_assessment_blueprints',
    'learning_assessment_forms'
);

-- No verdict while any item is pending: passed is NULL until then.
UPDATE sqlite_schema
SET sql = replace(
    sql,
    'passed INTEGER NOT NULL CHECK(passed IN (0,1))',
    'passed INTEGER CHECK(passed IN (0,1))'
)
WHERE type = 'table' AND name = 'learning_assessment_submissions';

-- The single-index columns below are dropped, and SQLite refuses DROP COLUMN
-- while a table-level CHECK names the column, so that CHECK goes first.
UPDATE sqlite_schema
SET sql = replace(
    sql,
    ',' || char(10) || '    CHECK(selected_index IS NULL OR selected_index >= 0)',
    ''
)
WHERE type = 'table' AND name IN (
    'learning_assessment_answer_keys',
    'learning_assessment_response_revisions'
);

-- RESET turns writable_schema off and reparses the edited schema; a syntax
-- error fails the migration here and the transaction rolls the edit back.
PRAGMA writable_schema = RESET;

-- Each renamed row is checked against the new list as it is written.
UPDATE learning_assessment_candidates SET format = 'single_choice' WHERE format = 'multiple_choice';
UPDATE learning_assessment_form_items SET format = 'single_choice' WHERE format = 'multiple_choice';
UPDATE learning_assessment_blueprint_slots SET format = 'single_choice' WHERE format = 'multiple_choice';

-- Which quiz a blueprint is: the lesson, module or course it covers (NULL id
-- for the course). Purposes keep their meaning: checkpoint is a lesson quiz,
-- module_test a module test, cumulative the course exam.
ALTER TABLE learning_assessment_blueprints
    ADD COLUMN scope_kind TEXT CHECK(scope_kind IN ('lesson','module','course'));
ALTER TABLE learning_assessment_blueprints ADD COLUMN scope_id TEXT;
CREATE INDEX learning_assessment_blueprints_scope
    ON learning_assessment_blueprints(program_id, scope_kind, scope_id, status, revision DESC);

-- Blueprints written before scopes existed: module tests take the module of
-- their first slot's outcome, a cumulative blueprint covers the course, and a
-- checkpoint whose first outcome belongs to a lesson is that lesson's quiz.
UPDATE learning_assessment_blueprints
SET scope_kind = 'module',
    scope_id = (
        SELECT o.module_id
        FROM learning_assessment_blueprint_slots s
        JOIN learning_outcome_definitions o ON o.id = s.outcome_id
        WHERE s.blueprint_id = learning_assessment_blueprints.id
          AND s.blueprint_revision = learning_assessment_blueprints.revision
        ORDER BY s.ordinal LIMIT 1
    )
WHERE scope_kind IS NULL AND purpose = 'module_test';
UPDATE learning_assessment_blueprints
SET scope_kind = 'course', scope_id = NULL
WHERE scope_kind IS NULL AND purpose = 'cumulative';
UPDATE learning_assessment_blueprints
SET scope_kind = 'lesson',
    scope_id = (
        SELECT o.lesson_id
        FROM learning_assessment_blueprint_slots s
        JOIN learning_outcome_definitions o ON o.id = s.outcome_id
        WHERE s.blueprint_id = learning_assessment_blueprints.id
          AND s.blueprint_revision = learning_assessment_blueprints.revision
        ORDER BY s.ordinal LIMIT 1
    )
WHERE scope_kind IS NULL AND purpose = 'checkpoint';
-- A blueprint whose scope could not be resolved stays unscoped and is never
-- offered as a quiz; its forms keep their history.
UPDATE learning_assessment_blueprints
SET scope_kind = NULL
WHERE scope_kind IN ('module','lesson') AND scope_id IS NULL;

-- One tagged key per candidate (LearningAssessmentKey, internal, never over
-- IPC), chosen by the candidate's format; the three typed columns go.
ALTER TABLE learning_assessment_answer_keys ADD COLUMN key_json TEXT NOT NULL DEFAULT '{}';
UPDATE learning_assessment_answer_keys
SET key_json = (
    SELECT CASE c.format
        WHEN 'single_choice' THEN json_object('kind','choice','value',learning_assessment_answer_keys.selected_index)
        WHEN 'ordering' THEN json_object('kind','ordering','value',json(learning_assessment_answer_keys.ordered_values_json))
        WHEN 'short_answer' THEN json_object('kind','text','value',json_object(
            'variants',json(learning_assessment_answer_keys.accepted_answers_json),'numeric',NULL))
        ELSE json_object('kind','rubric')
    END
    FROM learning_assessment_candidates c
    WHERE c.id = learning_assessment_answer_keys.candidate_id
);
ALTER TABLE learning_assessment_answer_keys DROP COLUMN selected_index;
ALTER TABLE learning_assessment_answer_keys DROP COLUMN accepted_answers_json;
ALTER TABLE learning_assessment_answer_keys DROP COLUMN ordered_values_json;
-- Per option, why it is wrong (single choice and multi-select); '[]' otherwise.
ALTER TABLE learning_assessment_answer_keys
    ADD COLUMN distractor_rationales_json TEXT NOT NULL DEFAULT '[]';

-- The bank: blinded key review status, where an item came from, and the
-- lesson it was written for (NULL for module and course items).
ALTER TABLE learning_assessment_candidates
    ADD COLUMN review_status TEXT NOT NULL DEFAULT 'agreed'
    CHECK(review_status IN ('agreed','disputed','dropped','reported'));
ALTER TABLE learning_assessment_candidates
    ADD COLUMN origin TEXT NOT NULL DEFAULT 'bank'
    CHECK(origin IN ('lesson','bank','migrated'));
ALTER TABLE learning_assessment_candidates ADD COLUMN lesson_id TEXT;
-- The passage the item was written from (verbatim in its lesson block or
-- source), shown as the source line and given to the grader as reference.
ALTER TABLE learning_assessment_candidates ADD COLUMN source_quote TEXT NOT NULL DEFAULT '';
-- Matching's left-hand entries; options_json holds the right-hand side.
ALTER TABLE learning_assessment_candidates ADD COLUMN left_items_json TEXT NOT NULL DEFAULT '[]';
CREATE INDEX learning_assessment_candidates_lesson
    ON learning_assessment_candidates(program_id, lesson_id, review_status);

-- Shuffled option order per form item: shown position -> authored index.
ALTER TABLE learning_assessment_form_items
    ADD COLUMN option_order_json TEXT NOT NULL DEFAULT '[]';
ALTER TABLE learning_assessment_form_items
    ADD COLUMN left_items_json TEXT NOT NULL DEFAULT '[]';

-- Forms draw from the course's whole bank by requirement (outcome, format,
-- difficulty), so blueprints no longer own a candidate list.
DROP TABLE learning_assessment_blueprint_candidates;

-- A form is graded or practice, and remembers the quiz it was taken for.
ALTER TABLE learning_assessment_forms
    ADD COLUMN mode TEXT NOT NULL DEFAULT 'graded' CHECK(mode IN ('graded','practice'));
ALTER TABLE learning_assessment_forms
    ADD COLUMN scope_kind TEXT CHECK(scope_kind IN ('lesson','module','course'));
ALTER TABLE learning_assessment_forms ADD COLUMN scope_id TEXT;
UPDATE learning_assessment_forms
SET scope_kind = (
        SELECT b.scope_kind FROM learning_assessment_blueprints b
        WHERE b.id = learning_assessment_forms.blueprint_id
          AND b.revision = learning_assessment_forms.blueprint_revision
    ),
    scope_id = (
        SELECT b.scope_id FROM learning_assessment_blueprints b
        WHERE b.id = learning_assessment_forms.blueprint_id
          AND b.revision = learning_assessment_forms.blueprint_revision
    );
CREATE INDEX learning_assessment_forms_scope
    ON learning_assessment_forms(program_id, scope_kind, scope_id, created_at DESC);

-- Responses for every format: chosen indices (single choice, true/false,
-- multi-select), one text per cloze blank, and a right-hand index (or null)
-- per matching pair. A single choice is a one-element selected_indices.
ALTER TABLE learning_assessment_response_revisions
    ADD COLUMN selected_indices_json TEXT NOT NULL DEFAULT '[]';
ALTER TABLE learning_assessment_response_revisions
    ADD COLUMN blank_responses_json TEXT NOT NULL DEFAULT '[]';
ALTER TABLE learning_assessment_response_revisions
    ADD COLUMN matches_json TEXT NOT NULL DEFAULT '[]';
UPDATE learning_assessment_response_revisions
SET selected_indices_json = json_array(selected_index)
WHERE selected_index IS NOT NULL;
ALTER TABLE learning_assessment_response_revisions DROP COLUMN selected_index;

-- Overrides and the two-phase submit.
ALTER TABLE learning_assessment_submissions ADD COLUMN score_without_overrides REAL
    CHECK(score_without_overrides IS NULL OR (score_without_overrides >= 0 AND score_without_overrides <= 1));
ALTER TABLE learning_assessment_submissions
    ADD COLUMN pending_items INTEGER NOT NULL DEFAULT 0 CHECK(pending_items >= 0);

-- Partial credit, the per-item why, overrides, and (section 4.3a) the
-- validated grade rationale: summary, per-criterion verdict, what was missing,
-- the learner's own words, what full credit includes, the source quote, and
-- the grader's identity. '{}' for keyed items, whose why is the key reveal
-- plus detail. The rationale replaces the criterion results and artifact
-- quotes, so there is one shape, not two.
ALTER TABLE learning_assessment_item_results ADD COLUMN points_earned REAL NOT NULL DEFAULT 0
    CHECK(points_earned >= 0);
ALTER TABLE learning_assessment_item_results ADD COLUMN points_possible REAL NOT NULL DEFAULT 1
    CHECK(points_possible > 0);
ALTER TABLE learning_assessment_item_results ADD COLUMN detail TEXT NOT NULL DEFAULT '';
ALTER TABLE learning_assessment_item_results
    ADD COLUMN learner_override INTEGER NOT NULL DEFAULT 0 CHECK(learner_override IN (0,1));
ALTER TABLE learning_assessment_item_results
    ADD COLUMN rationale_json TEXT NOT NULL DEFAULT '{}';
UPDATE learning_assessment_item_results
SET points_possible = (
        SELECT fi.points FROM learning_assessment_form_items fi
        WHERE fi.form_id = learning_assessment_item_results.form_id
          AND fi.ordinal = learning_assessment_item_results.item_ordinal
    ),
    points_earned = score * (
        SELECT fi.points FROM learning_assessment_form_items fi
        WHERE fi.form_id = learning_assessment_item_results.form_id
          AND fi.ordinal = learning_assessment_item_results.item_ordinal
    );
ALTER TABLE learning_assessment_item_results DROP COLUMN criterion_results_json;
ALTER TABLE learning_assessment_item_results DROP COLUMN artifact_quotes_json;

-- "I think I was right": one contest per item. The first grade's results and
-- rationale are copied in before the blind regrade, which writes its own
-- results, rationale and its answer to the learner's note.
CREATE TABLE learning_assessment_grade_contests (
    id TEXT PRIMARY KEY,
    program_id TEXT NOT NULL REFERENCES learning_programs(id) ON DELETE CASCADE,
    form_id TEXT NOT NULL,
    item_ordinal INTEGER NOT NULL,
    note TEXT NOT NULL,
    status TEXT NOT NULL CHECK(status IN ('open','revised','upheld','overridden','failed')),
    first_score REAL NOT NULL CHECK(first_score >= 0 AND first_score <= 1),
    first_results_json TEXT NOT NULL,
    regrade_score REAL CHECK(regrade_score IS NULL OR (regrade_score >= 0 AND regrade_score <= 1)),
    regrade_results_json TEXT,
    regrade_model TEXT,
    regrade_reason TEXT,
    response_to_note TEXT,
    created_at INTEGER NOT NULL,
    decided_at INTEGER,
    UNIQUE(form_id, item_ordinal),
    FOREIGN KEY(form_id, item_ordinal)
        REFERENCES learning_assessment_form_items(form_id, ordinal) ON DELETE CASCADE
);
CREATE INDEX learning_assessment_grade_contests_program
    ON learning_assessment_grade_contests(program_id, created_at);

-- "Report this question" on a keyed item: excluded from future forms.
CREATE TABLE learning_assessment_item_reports (
    id TEXT PRIMARY KEY,
    program_id TEXT NOT NULL REFERENCES learning_programs(id) ON DELETE CASCADE,
    candidate_id TEXT NOT NULL REFERENCES learning_assessment_candidates(id) ON DELETE CASCADE,
    form_id TEXT REFERENCES learning_assessment_forms(id) ON DELETE SET NULL,
    note TEXT NOT NULL,
    created_at INTEGER NOT NULL
);
CREATE INDEX learning_assessment_item_reports_program
    ON learning_assessment_item_reports(program_id, candidate_id);

-- Per-course pass marks and the written-answers switch, created lazily.
CREATE TABLE learning_quiz_settings (
    program_id TEXT PRIMARY KEY REFERENCES learning_programs(id) ON DELETE CASCADE,
    quiz_pass REAL NOT NULL DEFAULT 0.7 CHECK(quiz_pass > 0 AND quiz_pass <= 1),
    test_pass REAL NOT NULL DEFAULT 0.75 CHECK(test_pass > 0 AND test_pass <= 1),
    exam_pass REAL NOT NULL DEFAULT 0.75 CHECK(exam_pass > 0 AND exam_pass <= 1),
    written_answers INTEGER NOT NULL DEFAULT 1 CHECK(written_answers IN (0,1)),
    updated_at INTEGER NOT NULL
);

-- Lesson-level outcomes for every prepared lesson. They name their lesson and
-- no module (the lesson row says where it sits), so a lesson moved between
-- modules keeps its outcome, and module outcome lists stay the module's own.
INSERT INTO learning_outcome_definitions(
    id, program_id, module_id, lesson_id, title, description, ordinal, created_at
)
SELECT
    lower(hex(randomblob(4))) || '-' || lower(hex(randomblob(2))) || '-4'
        || substr(lower(hex(randomblob(2))), 2) || '-a'
        || substr(lower(hex(randomblob(2))), 2) || '-' || lower(hex(randomblob(6))),
    l.program_id, NULL, l.id, l.title, l.objective, 100 + l.ordinal,
    CAST(strftime('%s','now') AS INTEGER) * 1000
FROM learning_lessons l
WHERE l.preparation = 'ready'
  AND NOT EXISTS (SELECT 1 FROM learning_outcome_definitions o WHERE o.lesson_id = l.id);

-- Prepared lessons' reviewed questions join their lesson's bank as single
-- choice items (difficulty by former kind: practice 2, quiz 3, test 4).
-- content_sha256 is a uniqueness token for these rows, not a hash.
INSERT INTO learning_assessment_candidates(
    id, program_id, outcome_id, format, difficulty, prompt, options_json, artifact_kind,
    rubric_json, source_version_ids_json, authored_by, author_model, content_sha256,
    created_at, review_status, origin, lesson_id
)
SELECT
    q.id, l.program_id, o.id, 'single_choice',
    CASE q.kind WHEN 'practice' THEN 2 WHEN 'quiz' THEN 3 ELSE 4 END,
    q.prompt, q.options_json, NULL, '[]', q.source_ids_json, 'model', NULL,
    'migrated:' || q.id, CAST(strftime('%s','now') AS INTEGER) * 1000,
    'agreed', 'migrated', l.id
FROM learning_questions q
JOIN learning_lessons l ON l.id = q.lesson_id
JOIN learning_outcome_definitions o ON o.lesson_id = l.id
JOIN learning_answer_keys k ON k.question_id = q.id;
INSERT INTO learning_assessment_candidate_outcomes(candidate_id, outcome_id)
SELECT c.id, c.outcome_id FROM learning_assessment_candidates c WHERE c.origin = 'migrated';
INSERT INTO learning_assessment_answer_keys(candidate_id, explanation, key_json)
SELECT k.question_id, k.explanation, json_object('kind','choice','value',k.correct_index)
FROM learning_answer_keys k
JOIN learning_assessment_candidates c ON c.id = k.question_id AND c.origin = 'migrated';

-- Quick checks end. Attempt history is not carried (owner decision 5). The
-- answer keys go before the questions so their content triggers never fire
-- against a missing table; each table's triggers and indexes go with it.
DROP TABLE learning_attempts;
DROP TABLE learning_answer_keys;
DROP TABLE learning_questions;

-- Fail (and roll back) unless every edit is in force and nothing new can
-- block a course delete. The CREATE/DROP of a main-schema table also bumps
-- the schema cookie so pooled connections reload the edited schema.
CREATE TABLE _lattice_quizzes_guard (ok INTEGER NOT NULL CHECK (ok = 1));
INSERT INTO _lattice_quizzes_guard (ok)
SELECT
    (SELECT count(*) FROM sqlite_schema
     WHERE type = 'table'
       AND name IN ('learning_assessment_blueprint_slots','learning_assessment_candidates','learning_assessment_form_items')
       AND sql LIKE '%''single_choice'',''multi_select'',''true_false'',''short_answer'',''cloze'',''matching'',''ordering'',''explanation'',''artifact''%'
       AND sql NOT LIKE '%multiple_choice%') = 3
  AND (SELECT count(*) FROM sqlite_schema
       WHERE type = 'table'
         AND name IN ('learning_assessment_submissions','learning_assessment_item_results')
         AND sql LIKE '%''needs_review'',''pending''%') = 2
  AND (SELECT count(*) FROM sqlite_schema
       WHERE type = 'table'
         AND name IN ('learning_assessment_blueprints','learning_assessment_forms')
         AND sql LIKE '%CHECK(expected_minutes >= 1)%') = 2
  AND (SELECT "notnull" FROM pragma_table_info('learning_assessment_submissions') WHERE name = 'passed') = 0
  AND (SELECT count(*) FROM pragma_table_info('learning_assessment_answer_keys')
       WHERE name IN ('selected_index','accepted_answers_json','ordered_values_json')) = 0
  AND (SELECT count(*) FROM pragma_table_info('learning_assessment_response_revisions') WHERE name = 'selected_index') = 0
  AND (SELECT count(*) FROM pragma_table_info('learning_assessment_item_results')
       WHERE name IN ('criterion_results_json','artifact_quotes_json')) = 0
  AND (SELECT count(*) FROM sqlite_schema
       WHERE type = 'table' AND name IN ('learning_questions','learning_answer_keys','learning_attempts',
                                         'learning_assessment_blueprint_candidates')) = 0
  AND (SELECT count(DISTINCT id) FROM pragma_foreign_key_list('learning_assessment_grade_contests') WHERE on_delete = 'CASCADE') = 2
  AND (SELECT count(*) FROM pragma_foreign_key_list('learning_assessment_grade_contests') WHERE on_delete NOT IN ('CASCADE','SET NULL')) = 0
  AND (SELECT count(*) FROM pragma_foreign_key_list('learning_assessment_item_reports') WHERE on_delete NOT IN ('CASCADE','SET NULL')) = 0
  AND (SELECT count(*) FROM pragma_foreign_key_list('learning_quiz_settings') WHERE on_delete <> 'CASCADE') = 0
  AND (SELECT count(*) FROM learning_assessment_answer_keys WHERE key_json = '{}') = 0;
DROP TABLE _lattice_quizzes_guard;
