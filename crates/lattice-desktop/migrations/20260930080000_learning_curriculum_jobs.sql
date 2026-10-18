-- Accepted curriculum is a chain of immutable snapshots. The current program
-- tables remain the materialized read model; publishing a revision replaces
-- that read model transactionally only after the snapshot is complete.
CREATE TABLE learning_curriculum_revisions (
    id TEXT PRIMARY KEY,
    program_id TEXT NOT NULL REFERENCES learning_programs(id) ON DELETE CASCADE,
    revision INTEGER NOT NULL CHECK(revision >= 1),
    predecessor_id TEXT REFERENCES learning_curriculum_revisions(id) ON DELETE RESTRICT,
    status TEXT NOT NULL CHECK(status IN ('draft','accepted','superseded')),
    reason TEXT NOT NULL,
    snapshot_json TEXT NOT NULL,
    snapshot_sha256 TEXT NOT NULL,
    required_lesson_count INTEGER NOT NULL CHECK(required_lesson_count >= 0),
    resume_lesson_id TEXT,
    created_at INTEGER NOT NULL,
    accepted_at INTEGER,
    UNIQUE(program_id,revision),
    UNIQUE(program_id,snapshot_sha256),
    CHECK(
        (status='draft' AND accepted_at IS NULL)
        OR (status IN ('accepted','superseded') AND accepted_at IS NOT NULL)
    )
);
CREATE UNIQUE INDEX learning_curriculum_one_draft
    ON learning_curriculum_revisions(program_id) WHERE status='draft';
CREATE UNIQUE INDEX learning_curriculum_one_accepted
    ON learning_curriculum_revisions(program_id) WHERE status='accepted';

CREATE TABLE learning_curriculum_changes (
    revision_id TEXT NOT NULL REFERENCES learning_curriculum_revisions(id) ON DELETE CASCADE,
    ordinal INTEGER NOT NULL CHECK(ordinal >= 0),
    operation TEXT NOT NULL CHECK(operation IN (
        'add','edit','move','skip','replace','challenge','restore'
    )),
    lesson_id TEXT,
    before_json TEXT,
    after_json TEXT,
    explanation TEXT NOT NULL,
    PRIMARY KEY(revision_id,ordinal)
);

CREATE TABLE learning_curriculum_operations (
    operation_id TEXT PRIMARY KEY,
    program_id TEXT NOT NULL REFERENCES learning_programs(id) ON DELETE CASCADE,
    kind TEXT NOT NULL CHECK(kind IN (
        'create_revision','accept_revision','discard_revision','cancel_job','retry_job','save_diagnostic','publish_job','start_job'
    )),
    payload_hash TEXT NOT NULL,
    result_id TEXT,
    created_at INTEGER NOT NULL
);

CREATE TABLE learning_generation_jobs (
    id TEXT PRIMARY KEY,
    program_id TEXT NOT NULL REFERENCES learning_programs(id) ON DELETE CASCADE,
    operation_id TEXT NOT NULL UNIQUE,
    payload_hash TEXT NOT NULL,
    kind TEXT NOT NULL CHECK(kind IN (
        'program','diagnostic','curriculum_revision','lesson_preparation','assessment','practical'
    )),
    status TEXT NOT NULL CHECK(status IN (
        'pending','running','completed','failed','cancelled','interrupted'
    )),
    requested_json TEXT NOT NULL,
    base_revision_number INTEGER NOT NULL DEFAULT 0 CHECK(base_revision_number >= 0),
    progress_current INTEGER NOT NULL DEFAULT 0 CHECK(progress_current >= 0),
    progress_total INTEGER NOT NULL CHECK(progress_total >= 1),
    progress_message TEXT NOT NULL,
    staged_result_json TEXT,
    published_result_id TEXT,
    error_code TEXT,
    error_message TEXT,
    retry_of_job_id TEXT REFERENCES learning_generation_jobs(id) ON DELETE SET NULL,
    created_at INTEGER NOT NULL,
    started_at INTEGER,
    finished_at INTEGER,
    heartbeat_at INTEGER,
    CHECK(progress_current <= progress_total),
    CHECK(
        (status IN ('pending','running') AND finished_at IS NULL)
        OR (status NOT IN ('pending','running') AND finished_at IS NOT NULL)
    ),
    CHECK(status!='completed' OR published_result_id IS NOT NULL)
);
CREATE INDEX learning_generation_jobs_program
    ON learning_generation_jobs(program_id,created_at DESC,id);

CREATE TABLE learning_generation_job_events (
    job_id TEXT NOT NULL REFERENCES learning_generation_jobs(id) ON DELETE CASCADE,
    ordinal INTEGER NOT NULL CHECK(ordinal >= 0),
    status TEXT NOT NULL CHECK(status IN (
        'pending','running','completed','failed','cancelled','interrupted'
    )),
    progress_current INTEGER NOT NULL CHECK(progress_current >= 0),
    message TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    PRIMARY KEY(job_id,ordinal)
);

CREATE TABLE learning_diagnostic_attempts (
    id TEXT PRIMARY KEY,
    program_id TEXT NOT NULL REFERENCES learning_programs(id) ON DELETE CASCADE,
    operation_id TEXT NOT NULL UNIQUE,
    status TEXT NOT NULL CHECK(status IN ('active','submitted','skipped')),
    prompt_json TEXT NOT NULL,
    response_json TEXT NOT NULL,
    findings_json TEXT NOT NULL,
    source_coverage_gaps_json TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    submitted_at INTEGER,
    CHECK(
        (status='active' AND submitted_at IS NULL)
        OR (status!='active' AND submitted_at IS NOT NULL)
    )
);

CREATE TABLE learning_diagnostic_operations (
    operation_id TEXT PRIMARY KEY,
    program_id TEXT NOT NULL REFERENCES learning_programs(id) ON DELETE CASCADE,
    diagnostic_id TEXT NOT NULL REFERENCES learning_diagnostic_attempts(id) ON DELETE CASCADE,
    kind TEXT NOT NULL CHECK(kind IN ('start','submit','skip')),
    payload_hash TEXT NOT NULL,
    created_at INTEGER NOT NULL
);

ALTER TABLE learning_lessons ADD COLUMN curriculum_state TEXT NOT NULL DEFAULT 'outline'
    CHECK(curriculum_state IN ('outline','ready','completed','skipped','replaced','challenged'));
ALTER TABLE learning_lessons ADD COLUMN replacement_lesson_id TEXT;
