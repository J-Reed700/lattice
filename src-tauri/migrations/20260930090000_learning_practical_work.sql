-- Generic practical activities plus the engineering runtime/simulation pack.
-- Activity specifications are immutable after the first run; revisions create a
-- new activity row so historical attempts always reopen against their exact brief.
CREATE TABLE learning_runtime_profiles (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    engine TEXT NOT NULL CHECK(engine IN ('docker','podman')),
    image_id TEXT NOT NULL CHECK(
        image_id GLOB 'sha256:*'
        AND length(image_id)=71
        AND substr(image_id,8) NOT GLOB '*[^0-9a-f]*'
    ),
    command_json TEXT NOT NULL,
    limits_json TEXT NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 1 CHECK(enabled IN (0,1)),
    revision INTEGER NOT NULL DEFAULT 0 CHECK(revision >= 0),
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE learning_runtime_profile_operations (
    operation_id TEXT PRIMARY KEY,
    profile_id TEXT NOT NULL REFERENCES learning_runtime_profiles(id) ON DELETE CASCADE,
    payload_hash TEXT NOT NULL,
    result_revision INTEGER NOT NULL CHECK(result_revision >= 0),
    created_at INTEGER NOT NULL
);

CREATE TABLE learning_practical_activities (
    id TEXT PRIMARY KEY,
    program_id TEXT NOT NULL REFERENCES learning_programs(id) ON DELETE CASCADE,
    lesson_id TEXT NOT NULL,
    predecessor_id TEXT,
    kind TEXT NOT NULL CHECK(kind IN (
        'code_lab','debugging','code_review','incident','system_design',
        'project','interview','conversation','writing_revision','custom'
    )),
    title TEXT NOT NULL,
    brief TEXT NOT NULL,
    status TEXT NOT NULL CHECK(status IN ('draft','ready','retired')),
    practice_mode TEXT NOT NULL CHECK(practice_mode IN ('explore','practice','demonstrate')),
    allowed_aids_json TEXT NOT NULL,
    outcome_ids_json TEXT NOT NULL,
    source_version_ids_json TEXT NOT NULL,
    rubric_json TEXT NOT NULL,
    runtime_kind TEXT NOT NULL CHECK(runtime_kind IN ('none','container')),
    runtime_profile_id TEXT REFERENCES learning_runtime_profiles(id) ON DELETE RESTRICT,
    runtime_engine TEXT CHECK(runtime_engine IN ('docker','podman')),
    runtime_image_id TEXT,
    runtime_command_json TEXT,
    runtime_limits_json TEXT,
    generator_model TEXT NOT NULL,
    revision INTEGER NOT NULL DEFAULT 0 CHECK(revision >= 0),
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    UNIQUE(program_id,id),
    FOREIGN KEY(program_id,lesson_id)
        REFERENCES learning_lessons(program_id,id) ON DELETE CASCADE,
    FOREIGN KEY(program_id,predecessor_id)
        REFERENCES learning_practical_activities(program_id,id) ON DELETE SET NULL,
    CHECK(
        (runtime_kind='none' AND runtime_profile_id IS NULL AND runtime_engine IS NULL AND runtime_image_id IS NULL AND runtime_command_json IS NULL AND runtime_limits_json IS NULL)
        OR
        (runtime_kind='container' AND runtime_profile_id IS NOT NULL AND runtime_engine IS NOT NULL AND runtime_image_id IS NOT NULL AND runtime_command_json IS NOT NULL AND runtime_limits_json IS NOT NULL)
    )
);
CREATE INDEX learning_practical_activities_program
    ON learning_practical_activities(program_id,lesson_id,created_at,id);

CREATE TABLE learning_practical_activity_operations (
    operation_id TEXT PRIMARY KEY,
    program_id TEXT NOT NULL REFERENCES learning_programs(id) ON DELETE CASCADE,
    activity_id TEXT NOT NULL REFERENCES learning_practical_activities(id) ON DELETE CASCADE,
    kind TEXT NOT NULL CHECK(kind IN ('generate','revise','retire')),
    payload_hash TEXT NOT NULL,
    result_revision INTEGER NOT NULL CHECK(result_revision >= 0),
    created_at INTEGER NOT NULL
);

CREATE TABLE learning_practical_files (
    activity_id TEXT NOT NULL REFERENCES learning_practical_activities(id) ON DELETE CASCADE,
    ordinal INTEGER NOT NULL CHECK(ordinal >= 0),
    path TEXT NOT NULL,
    role TEXT NOT NULL CHECK(role IN ('starter','check','reference','solution')),
    content TEXT NOT NULL,
    content_sha256 TEXT NOT NULL,
    editable INTEGER NOT NULL CHECK(editable IN (0,1)),
    PRIMARY KEY(activity_id,path),
    UNIQUE(activity_id,ordinal),
    CHECK(role='starter' OR editable=0)
);

-- A run owns the exact learner file snapshot and runtime image ID it used.
-- Hidden check and solution files remain backend-only and never enter renderer DTOs.
CREATE TABLE learning_practical_runs (
    id TEXT PRIMARY KEY,
    program_id TEXT NOT NULL,
    activity_id TEXT NOT NULL,
    activity_revision INTEGER NOT NULL CHECK(activity_revision >= 0),
    activity_snapshot_json TEXT NOT NULL,
    practice_session_id TEXT,
    operation_id TEXT NOT NULL UNIQUE,
    payload_hash TEXT NOT NULL,
    engine TEXT CHECK(engine IN ('docker','podman')),
    image_id TEXT,
    status TEXT NOT NULL CHECK(status IN (
        'pending','running','passed','failed','timed_out','cancelled','interrupted','runtime_unavailable'
    )),
    learner_files_json TEXT NOT NULL,
    stdout TEXT NOT NULL DEFAULT '',
    stderr TEXT NOT NULL DEFAULT '',
    output_truncated INTEGER NOT NULL DEFAULT 0 CHECK(output_truncated IN (0,1)),
    exit_code INTEGER,
    duration_ms INTEGER,
    check_results_json TEXT NOT NULL DEFAULT '[]',
    created_at INTEGER NOT NULL,
    completed_at INTEGER,
    UNIQUE(program_id,id),
    FOREIGN KEY(program_id,activity_id)
        REFERENCES learning_practical_activities(program_id,id) ON DELETE CASCADE,
    FOREIGN KEY(program_id,practice_session_id)
        REFERENCES learning_practice_sessions(program_id,id) ON DELETE SET NULL,
    CHECK(
        (status IN ('pending','running') AND completed_at IS NULL)
        OR
        (status NOT IN ('pending','running') AND completed_at IS NOT NULL)
    )
);
CREATE INDEX learning_practical_runs_activity
    ON learning_practical_runs(program_id,activity_id,created_at DESC,id);

CREATE TABLE learning_practical_run_operations (
    operation_id TEXT PRIMARY KEY,
    program_id TEXT NOT NULL,
    run_id TEXT NOT NULL REFERENCES learning_practical_runs(id) ON DELETE CASCADE,
    kind TEXT NOT NULL CHECK(kind IN ('start','cancel')),
    payload_hash TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    FOREIGN KEY(program_id,run_id)
        REFERENCES learning_practical_runs(program_id,id) ON DELETE CASCADE
);

CREATE TABLE learning_practical_run_checks (
    run_id TEXT NOT NULL REFERENCES learning_practical_runs(id) ON DELETE CASCADE,
    ordinal INTEGER NOT NULL CHECK(ordinal >= 0),
    name TEXT NOT NULL,
    status TEXT NOT NULL CHECK(status IN ('passed','failed','error','not_run')),
    message TEXT NOT NULL,
    duration_ms INTEGER,
    PRIMARY KEY(run_id,ordinal)
);

-- Simulations reuse the practical activity brief/rubric but keep every role turn.
-- A model turn is feedback or scenario state, never evidence by itself.
CREATE TABLE learning_simulation_sessions (
    id TEXT PRIMARY KEY,
    program_id TEXT NOT NULL,
    activity_id TEXT NOT NULL,
    activity_revision INTEGER NOT NULL CHECK(activity_revision >= 0),
    activity_snapshot_json TEXT NOT NULL,
    practice_session_id TEXT,
    operation_id TEXT NOT NULL UNIQUE,
    payload_hash TEXT NOT NULL,
    learner_role TEXT NOT NULL,
    counterpart_role TEXT NOT NULL,
    status TEXT NOT NULL CHECK(status IN ('active','submitted','cancelled')),
    revision INTEGER NOT NULL DEFAULT 0 CHECK(revision >= 0),
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    submitted_at INTEGER,
    UNIQUE(program_id,id),
    FOREIGN KEY(program_id,activity_id)
        REFERENCES learning_practical_activities(program_id,id) ON DELETE CASCADE,
    FOREIGN KEY(program_id,practice_session_id)
        REFERENCES learning_practice_sessions(program_id,id) ON DELETE SET NULL
);
CREATE INDEX learning_simulation_sessions_activity
    ON learning_simulation_sessions(program_id,activity_id,created_at DESC,id);

CREATE TABLE learning_simulation_turns (
    id TEXT PRIMARY KEY,
    program_id TEXT NOT NULL,
    session_id TEXT NOT NULL,
    operation_id TEXT NOT NULL UNIQUE,
    payload_hash TEXT NOT NULL,
    ordinal INTEGER NOT NULL CHECK(ordinal >= 0),
    speaker TEXT NOT NULL CHECK(speaker IN ('learner','counterpart','coach')),
    content TEXT NOT NULL,
    citations_json TEXT NOT NULL DEFAULT '[]',
    model_name TEXT,
    created_at INTEGER NOT NULL,
    UNIQUE(session_id,ordinal),
    FOREIGN KEY(program_id,session_id)
        REFERENCES learning_simulation_sessions(program_id,id) ON DELETE CASCADE
);

CREATE TABLE learning_simulation_operations (
    operation_id TEXT PRIMARY KEY,
    program_id TEXT NOT NULL,
    session_id TEXT NOT NULL,
    kind TEXT NOT NULL CHECK(kind IN ('start','turn','finish','cancel')),
    payload_hash TEXT NOT NULL,
    result_id TEXT,
    created_at INTEGER NOT NULL,
    FOREIGN KEY(program_id,session_id)
        REFERENCES learning_simulation_sessions(program_id,id) ON DELETE CASCADE
);
