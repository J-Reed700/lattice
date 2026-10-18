CREATE TABLE learning_practice_sessions (
    id TEXT PRIMARY KEY,
    program_id TEXT NOT NULL REFERENCES learning_programs(id) ON DELETE CASCADE,
    lesson_id TEXT NOT NULL,
    lesson_title TEXT NOT NULL,
    lesson_objective TEXT NOT NULL,
    task_prompt TEXT NOT NULL,
    status TEXT NOT NULL CHECK(status IN ('active','submitted')),
    mode TEXT NOT NULL CHECK(mode IN ('explore','practice','demonstrate')),
    revision INTEGER NOT NULL DEFAULT 0,
    source_version_ids_json TEXT NOT NULL,
    rubric_json TEXT NOT NULL,
    revealed_solution TEXT,
    revealed_solution_citations_json TEXT NOT NULL DEFAULT '[]',
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    submitted_at INTEGER,
    FOREIGN KEY(program_id, lesson_id) REFERENCES learning_lessons(program_id,id) ON DELETE CASCADE,
    UNIQUE(program_id,id)
);
CREATE INDEX learning_practice_sessions_program ON learning_practice_sessions(program_id,created_at DESC);

CREATE TABLE learning_practice_artifact_revisions (
    program_id TEXT NOT NULL,
    session_id TEXT NOT NULL,
    revision INTEGER NOT NULL,
    text TEXT NOT NULL,
    sha256 TEXT NOT NULL,
    operation_id TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    PRIMARY KEY(session_id,revision),
    UNIQUE(operation_id),
    FOREIGN KEY(program_id,session_id) REFERENCES learning_practice_sessions(program_id,id) ON DELETE CASCADE
);

CREATE TABLE learning_practice_operations (
    operation_id TEXT PRIMARY KEY,
    program_id TEXT NOT NULL,
    session_id TEXT NOT NULL,
    kind TEXT NOT NULL CHECK(kind IN ('start','save_artifact','change_mode','open_source','tutor','reveal_solution','submit','accept_proposal','reject_proposal')),
    payload_hash TEXT NOT NULL,
    response_id TEXT,
    created_at INTEGER NOT NULL,
    FOREIGN KEY(program_id,session_id) REFERENCES learning_practice_sessions(program_id,id) ON DELETE CASCADE
);
CREATE INDEX learning_practice_operations_session ON learning_practice_operations(program_id,session_id,created_at);

CREATE TABLE learning_practice_assistance (
    id TEXT PRIMARY KEY,
    program_id TEXT NOT NULL,
    session_id TEXT NOT NULL,
    operation_id TEXT NOT NULL UNIQUE,
    kind TEXT NOT NULL CHECK(kind IN ('source_opened','tutor_response','hint','solution_revealed','mode_changed')),
    mode TEXT NOT NULL CHECK(mode IN ('explore','practice','demonstrate')),
    artifact_revision INTEGER NOT NULL,
    details_json TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    FOREIGN KEY(program_id,session_id) REFERENCES learning_practice_sessions(program_id,id) ON DELETE CASCADE
);
CREATE INDEX learning_practice_assistance_session ON learning_practice_assistance(session_id,created_at,id);

CREATE TABLE learning_practice_tutor_turns (
    id TEXT PRIMARY KEY,
    program_id TEXT NOT NULL,
    session_id TEXT NOT NULL,
    operation_id TEXT NOT NULL UNIQUE,
    prompt TEXT NOT NULL,
    response TEXT NOT NULL,
    request_kind TEXT NOT NULL CHECK(request_kind IN ('hint','question','critique')),
    hint_level TEXT,
    citations_json TEXT NOT NULL,
    proposal_ids_json TEXT NOT NULL,
    model_name TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    FOREIGN KEY(program_id,session_id) REFERENCES learning_practice_sessions(program_id,id) ON DELETE CASCADE
);
CREATE INDEX learning_practice_turns_session ON learning_practice_tutor_turns(session_id,created_at,id);

CREATE TABLE learning_practice_proposals (
    id TEXT PRIMARY KEY,
    program_id TEXT NOT NULL,
    session_id TEXT NOT NULL,
    tutor_turn_id TEXT,
    kind TEXT NOT NULL CHECK(kind IN ('misconception','follow_up')),
    text TEXT NOT NULL,
    evidence_quote TEXT,
    status TEXT NOT NULL CHECK(status IN ('pending','accepted','rejected')),
    created_at INTEGER NOT NULL,
    decided_at INTEGER,
    FOREIGN KEY(program_id,session_id) REFERENCES learning_practice_sessions(program_id,id) ON DELETE CASCADE,
    FOREIGN KEY(tutor_turn_id) REFERENCES learning_practice_tutor_turns(id) ON DELETE SET NULL
);
CREATE INDEX learning_practice_proposals_session ON learning_practice_proposals(session_id,created_at,id);

CREATE TABLE learning_practice_submissions (
    session_id TEXT PRIMARY KEY,
    program_id TEXT NOT NULL,
    operation_id TEXT NOT NULL UNIQUE,
    payload_hash TEXT NOT NULL,
    grade_status TEXT NOT NULL CHECK(grade_status IN ('provisional','uncertain')),
    artifact_revision INTEGER NOT NULL,
    artifact_text TEXT NOT NULL,
    rubric_json TEXT NOT NULL,
    criteria_json TEXT NOT NULL,
    evidence_json TEXT NOT NULL,
    assistance_json TEXT NOT NULL,
    mode TEXT NOT NULL CHECK(mode IN ('explore','practice','demonstrate')),
    grader_model TEXT NOT NULL,
    submitted_at INTEGER NOT NULL,
    FOREIGN KEY(program_id,session_id) REFERENCES learning_practice_sessions(program_id,id) ON DELETE CASCADE
);

CREATE TABLE learning_practice_evidence_events (
    id TEXT PRIMARY KEY,
    program_id TEXT NOT NULL,
    session_id TEXT NOT NULL,
    dimension TEXT NOT NULL CHECK(dimension IN ('recall','explanation','application','transfer')),
    observed INTEGER NOT NULL CHECK(observed IN (0,1)),
    observation TEXT NOT NULL,
    evidence_quote TEXT,
    assistance_kinds_json TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    FOREIGN KEY(program_id,session_id) REFERENCES learning_practice_sessions(program_id,id) ON DELETE CASCADE
);
CREATE INDEX learning_practice_evidence_session ON learning_practice_evidence_events(session_id,dimension);
