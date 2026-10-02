-- Versioned, renderer-safe assessments and a shared evidence timeline.
-- Answer keys are deliberately separated from public item records so normal
-- workspace reads cannot leak them before a form is submitted.
CREATE TABLE learning_outcome_definitions (
    id TEXT PRIMARY KEY,
    program_id TEXT NOT NULL REFERENCES learning_programs(id) ON DELETE CASCADE,
    module_id TEXT,
    lesson_id TEXT,
    title TEXT NOT NULL,
    description TEXT NOT NULL,
    ordinal INTEGER NOT NULL CHECK(ordinal >= 0),
    created_at INTEGER NOT NULL,
    UNIQUE(program_id,id),
    UNIQUE(program_id,module_id,ordinal),
    FOREIGN KEY(program_id,module_id)
        REFERENCES learning_modules(program_id,id) ON DELETE CASCADE,
    FOREIGN KEY(program_id,lesson_id)
        REFERENCES learning_lessons(program_id,id) ON DELETE CASCADE
);

CREATE TABLE learning_assessment_blueprints (
    id TEXT NOT NULL,
    program_id TEXT NOT NULL REFERENCES learning_programs(id) ON DELETE CASCADE,
    revision INTEGER NOT NULL CHECK(revision >= 1),
    predecessor_revision INTEGER,
    purpose TEXT NOT NULL CHECK(purpose IN (
        'practice','checkpoint','module_test','cumulative','transfer'
    )),
    title TEXT NOT NULL,
    instructions TEXT NOT NULL,
    expected_minutes INTEGER NOT NULL CHECK(expected_minutes BETWEEN 1 AND 480),
    allowed_aids_json TEXT NOT NULL,
    passing_score REAL NOT NULL CHECK(passing_score >= 0 AND passing_score <= 1),
    rubric_json TEXT NOT NULL,
    feedback_timing TEXT NOT NULL CHECK(feedback_timing IN ('immediate','after_batch','after_submission')),
    source_version_ids_json TEXT NOT NULL,
    status TEXT NOT NULL CHECK(status IN ('draft','accepted','retired')),
    change_reason TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    PRIMARY KEY(id,revision),
    UNIQUE(program_id,id,revision),
    CHECK(
        (predecessor_revision IS NULL AND revision=1)
        OR predecessor_revision=revision-1
    )
);
CREATE INDEX learning_assessment_blueprints_program
    ON learning_assessment_blueprints(program_id,status,created_at DESC,id,revision);

CREATE TABLE learning_assessment_blueprint_slots (
    blueprint_id TEXT NOT NULL,
    blueprint_revision INTEGER NOT NULL,
    ordinal INTEGER NOT NULL CHECK(ordinal >= 0),
    outcome_id TEXT NOT NULL REFERENCES learning_outcome_definitions(id) ON DELETE RESTRICT,
    format TEXT NOT NULL CHECK(format IN (
        'multiple_choice','short_answer','explanation','ordering','artifact'
    )),
    difficulty INTEGER NOT NULL CHECK(difficulty BETWEEN 1 AND 5),
    difficulty_max INTEGER NOT NULL CHECK(difficulty_max BETWEEN difficulty AND 5),
    points REAL NOT NULL CHECK(points > 0),
    PRIMARY KEY(blueprint_id,blueprint_revision,ordinal),
    FOREIGN KEY(blueprint_id,blueprint_revision)
        REFERENCES learning_assessment_blueprints(id,revision) ON DELETE CASCADE
);

CREATE TABLE learning_assessment_candidates (
    id TEXT PRIMARY KEY,
    program_id TEXT NOT NULL REFERENCES learning_programs(id) ON DELETE CASCADE,
    outcome_id TEXT NOT NULL REFERENCES learning_outcome_definitions(id) ON DELETE RESTRICT,
    format TEXT NOT NULL CHECK(format IN (
        'multiple_choice','short_answer','explanation','ordering','artifact'
    )),
    difficulty INTEGER NOT NULL CHECK(difficulty BETWEEN 1 AND 5),
    prompt TEXT NOT NULL,
    options_json TEXT NOT NULL,
    artifact_kind TEXT,
    rubric_json TEXT NOT NULL,
    source_version_ids_json TEXT NOT NULL,
    authored_by TEXT NOT NULL CHECK(authored_by IN ('person','model','import')),
    author_model TEXT,
    content_sha256 TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    UNIQUE(program_id,content_sha256),
    UNIQUE(program_id,id)
);
CREATE TABLE learning_assessment_candidate_outcomes (
    candidate_id TEXT NOT NULL REFERENCES learning_assessment_candidates(id) ON DELETE CASCADE,
    outcome_id TEXT NOT NULL REFERENCES learning_outcome_definitions(id) ON DELETE RESTRICT,
    PRIMARY KEY(candidate_id,outcome_id)
);

CREATE TABLE learning_assessment_answer_keys (
    candidate_id TEXT PRIMARY KEY REFERENCES learning_assessment_candidates(id) ON DELETE CASCADE,
    selected_index INTEGER,
    accepted_answers_json TEXT NOT NULL,
    ordered_values_json TEXT NOT NULL,
    explanation TEXT NOT NULL,
    CHECK(selected_index IS NULL OR selected_index >= 0)
);

-- Candidate authoring is scoped to an immutable blueprint revision. A later
-- blueprint may request the same outcomes, but it cannot select another
-- revision's prompts or frozen source set from the program-wide bank.
CREATE TABLE learning_assessment_blueprint_candidates (
    program_id TEXT NOT NULL,
    blueprint_id TEXT NOT NULL,
    blueprint_revision INTEGER NOT NULL,
    candidate_id TEXT NOT NULL,
    PRIMARY KEY(blueprint_id,blueprint_revision,candidate_id),
    FOREIGN KEY(program_id,blueprint_id,blueprint_revision)
        REFERENCES learning_assessment_blueprints(program_id,id,revision) ON DELETE CASCADE,
    FOREIGN KEY(program_id,candidate_id)
        REFERENCES learning_assessment_candidates(program_id,id) ON DELETE CASCADE
);

CREATE TABLE learning_assessment_forms (
    id TEXT PRIMARY KEY,
    program_id TEXT NOT NULL REFERENCES learning_programs(id) ON DELETE CASCADE,
    blueprint_id TEXT NOT NULL,
    blueprint_revision INTEGER NOT NULL,
    retake_of_form_id TEXT REFERENCES learning_assessment_forms(id) ON DELETE SET NULL,
    status TEXT NOT NULL CHECK(status IN ('active','submitted','interrupted')),
    revision INTEGER NOT NULL DEFAULT 0 CHECK(revision >= 0),
    title TEXT NOT NULL,
    instructions TEXT NOT NULL,
    expected_minutes INTEGER NOT NULL CHECK(expected_minutes BETWEEN 1 AND 480),
    allowed_aids_json TEXT NOT NULL,
    purpose TEXT NOT NULL CHECK(purpose IN (
        'practice','checkpoint','module_test','cumulative','transfer'
    )),
    passing_score REAL NOT NULL CHECK(passing_score >= 0 AND passing_score <= 1),
    rubric_json TEXT NOT NULL,
    feedback_timing TEXT NOT NULL CHECK(feedback_timing IN ('immediate','after_batch','after_submission')),
    source_version_ids_json TEXT NOT NULL,
    model_name TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    submitted_at INTEGER,
    FOREIGN KEY(blueprint_id,blueprint_revision)
        REFERENCES learning_assessment_blueprints(id,revision) ON DELETE RESTRICT,
    CHECK(
        (status='submitted' AND submitted_at IS NOT NULL)
        OR (status!='submitted' AND submitted_at IS NULL)
    )
);
CREATE INDEX learning_assessment_forms_program
    ON learning_assessment_forms(program_id,created_at DESC,id);

CREATE TABLE learning_assessment_form_items (
    form_id TEXT NOT NULL REFERENCES learning_assessment_forms(id) ON DELETE CASCADE,
    ordinal INTEGER NOT NULL CHECK(ordinal >= 0),
    candidate_id TEXT NOT NULL REFERENCES learning_assessment_candidates(id) ON DELETE RESTRICT,
    outcome_id TEXT NOT NULL REFERENCES learning_outcome_definitions(id) ON DELETE RESTRICT,
    format TEXT NOT NULL CHECK(format IN (
        'multiple_choice','short_answer','explanation','ordering','artifact'
    )),
    difficulty INTEGER NOT NULL CHECK(difficulty BETWEEN 1 AND 5),
    prompt TEXT NOT NULL,
    options_json TEXT NOT NULL,
    artifact_kind TEXT,
    rubric_json TEXT NOT NULL,
    points REAL NOT NULL CHECK(points > 0),
    previously_exposed INTEGER NOT NULL CHECK(previously_exposed IN (0,1)),
    PRIMARY KEY(form_id,ordinal),
    UNIQUE(form_id,candidate_id)
);
CREATE TABLE learning_assessment_form_item_outcomes (
    form_id TEXT NOT NULL,
    item_ordinal INTEGER NOT NULL,
    outcome_id TEXT NOT NULL REFERENCES learning_outcome_definitions(id) ON DELETE RESTRICT,
    PRIMARY KEY(form_id,item_ordinal,outcome_id),
    FOREIGN KEY(form_id,item_ordinal)
        REFERENCES learning_assessment_form_items(form_id,ordinal) ON DELETE CASCADE
);

CREATE TABLE learning_assessment_response_revisions (
    form_id TEXT NOT NULL REFERENCES learning_assessment_forms(id) ON DELETE CASCADE,
    item_ordinal INTEGER NOT NULL,
    revision INTEGER NOT NULL CHECK(revision >= 1),
    operation_id TEXT NOT NULL UNIQUE,
    payload_hash TEXT NOT NULL,
    selected_index INTEGER,
    text_response TEXT,
    ordered_values_json TEXT NOT NULL,
    artifact_json TEXT,
    assistance_json TEXT NOT NULL DEFAULT '[]',
    created_at INTEGER NOT NULL,
    PRIMARY KEY(form_id,item_ordinal,revision),
    FOREIGN KEY(form_id,item_ordinal)
        REFERENCES learning_assessment_form_items(form_id,ordinal) ON DELETE CASCADE,
    CHECK(selected_index IS NULL OR selected_index >= 0)
);

CREATE TABLE learning_assessment_submissions (
    form_id TEXT PRIMARY KEY REFERENCES learning_assessment_forms(id) ON DELETE CASCADE,
    program_id TEXT NOT NULL REFERENCES learning_programs(id) ON DELETE CASCADE,
    operation_id TEXT NOT NULL UNIQUE,
    payload_hash TEXT NOT NULL,
    score REAL NOT NULL CHECK(score >= 0 AND score <= 1),
    passed INTEGER NOT NULL CHECK(passed IN (0,1)),
    grade_status TEXT NOT NULL CHECK(grade_status IN (
        'deterministic','provisional','uncertain','needs_review'
    )),
    grader_model TEXT,
    grader_disagreement_json TEXT NOT NULL,
    feedback TEXT NOT NULL,
    submitted_at INTEGER NOT NULL
);

CREATE TABLE learning_assessment_item_results (
    form_id TEXT NOT NULL REFERENCES learning_assessment_submissions(form_id) ON DELETE CASCADE,
    item_ordinal INTEGER NOT NULL,
    score REAL NOT NULL CHECK(score >= 0 AND score <= 1),
    correct INTEGER CHECK(correct IN (0,1)),
    grade_status TEXT NOT NULL CHECK(grade_status IN (
        'deterministic','provisional','uncertain','needs_review'
    )),
    feedback TEXT NOT NULL,
    criterion_results_json TEXT NOT NULL,
    artifact_quotes_json TEXT NOT NULL,
    PRIMARY KEY(form_id,item_ordinal),
    FOREIGN KEY(form_id,item_ordinal)
        REFERENCES learning_assessment_form_items(form_id,ordinal) ON DELETE RESTRICT
);

CREATE TABLE learning_evidence_events (
    id TEXT PRIMARY KEY,
    program_id TEXT NOT NULL REFERENCES learning_programs(id) ON DELETE CASCADE,
    outcome_id TEXT REFERENCES learning_outcome_definitions(id) ON DELETE SET NULL,
    source_kind TEXT NOT NULL CHECK(source_kind IN (
        'practice','assessment','recall','practical','simulation','manual'
    )),
    source_id TEXT NOT NULL,
    dimension TEXT NOT NULL CHECK(dimension IN (
        'recall','explanation','application','transfer'
    )),
    result TEXT NOT NULL CHECK(result IN (
        'observed','not_observed','uncertain','not_assessed'
    )),
    score REAL CHECK(score >= 0 AND score <= 1),
    observation TEXT NOT NULL,
    evidence_quote TEXT,
    assistance_json TEXT NOT NULL,
    observed_at INTEGER NOT NULL
);
CREATE INDEX learning_evidence_timeline
    ON learning_evidence_events(program_id,outcome_id,observed_at DESC,id);

CREATE TABLE learning_follow_up_recommendations (
    id TEXT PRIMARY KEY,
    program_id TEXT NOT NULL REFERENCES learning_programs(id) ON DELETE CASCADE,
    outcome_id TEXT REFERENCES learning_outcome_definitions(id) ON DELETE SET NULL,
    reason_code TEXT NOT NULL CHECK(reason_code IN (
        'missed_outcome','assisted_success','low_transfer','stale_evidence','uncertain_grade'
    )),
    explanation TEXT NOT NULL,
    action_kind TEXT NOT NULL CHECK(action_kind IN (
        'lesson','practice','assessment','recall','practical'
    )),
    action_ref TEXT,
    status TEXT NOT NULL CHECK(status IN ('pending','accepted','dismissed','completed')),
    evidence_event_ids_json TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    decided_at INTEGER
);
CREATE INDEX learning_follow_up_program
    ON learning_follow_up_recommendations(program_id,status,created_at DESC,id);

CREATE TABLE learning_assessment_operations (
    operation_id TEXT PRIMARY KEY,
    program_id TEXT NOT NULL REFERENCES learning_programs(id) ON DELETE CASCADE,
    form_id TEXT,
    kind TEXT NOT NULL CHECK(kind IN (
        'create_blueprint','create_form','save_response','interrupt','submit','decide_follow_up'
    )),
    payload_hash TEXT NOT NULL,
    response_id TEXT,
    created_at INTEGER NOT NULL,
    FOREIGN KEY(form_id) REFERENCES learning_assessment_forms(id) ON DELETE CASCADE
);
