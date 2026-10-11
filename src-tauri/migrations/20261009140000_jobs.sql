-- Durable background work for every feature. Only shared::runtime::jobs writes
-- these tables; a feature keeps its own facts about a job in a side table
-- keyed by job_id. Lesson jobs saved under the old learning tables are not
-- carried over: their courses keep their published lessons and outlines.
DROP TABLE learning_generation_checkpoints;
DROP TABLE learning_generation_job_events;
DROP TABLE learning_generation_jobs;

CREATE TABLE jobs (
    id TEXT PRIMARY KEY,
    kind TEXT NOT NULL CHECK(length(kind) > 0),
    subject_id TEXT,
    operation_id TEXT NOT NULL UNIQUE,
    payload_hash TEXT NOT NULL,
    status TEXT NOT NULL CHECK(status IN (
        'pending','running','completed','failed','cancelled','interrupted'
    )),
    requested_json TEXT NOT NULL CHECK(json_valid(requested_json)),
    progress_current INTEGER NOT NULL DEFAULT 0 CHECK(progress_current >= 0),
    progress_total INTEGER NOT NULL CHECK(progress_total >= 1),
    progress_message TEXT NOT NULL,
    activity_json TEXT CHECK(activity_json IS NULL OR json_valid(activity_json)),
    staged_result_json TEXT CHECK(staged_result_json IS NULL OR json_valid(staged_result_json)),
    result_ref TEXT,
    error_code TEXT,
    error_message TEXT,
    retry_of_job_id TEXT REFERENCES jobs(id) ON DELETE SET NULL,
    retry_count INTEGER NOT NULL DEFAULT 0 CHECK(retry_count >= 0),
    retry_not_before INTEGER,
    created_at INTEGER NOT NULL,
    started_at INTEGER,
    finished_at INTEGER,
    heartbeat_at INTEGER,
    CHECK(progress_current <= progress_total),
    CHECK(
        (status IN ('pending','running') AND finished_at IS NULL)
        OR (status NOT IN ('pending','running') AND finished_at IS NOT NULL)
    ),
    CHECK(status!='completed' OR result_ref IS NOT NULL)
);
CREATE INDEX jobs_subject ON jobs(kind,subject_id,created_at DESC,id);
CREATE INDEX jobs_status ON jobs(status,kind);
CREATE INDEX jobs_retry_of ON jobs(retry_of_job_id);

CREATE TABLE job_events (
    job_id TEXT NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
    ordinal INTEGER NOT NULL CHECK(ordinal >= 0),
    status TEXT NOT NULL CHECK(status IN (
        'pending','running','completed','failed','cancelled','interrupted'
    )),
    progress_current INTEGER NOT NULL CHECK(progress_current >= 0),
    message TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    PRIMARY KEY(job_id,ordinal)
);

-- Worker-owned receipts are separate rows so completing one step does not
-- rewrite the others, and concurrent completions cannot lose updates.
CREATE TABLE job_checkpoints (
    job_id TEXT NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
    key TEXT NOT NULL,
    data_json TEXT NOT NULL CHECK(json_valid(data_json)),
    updated_at INTEGER NOT NULL,
    PRIMARY KEY(job_id,key)
);

-- Learning's facts about a lesson job: the course it prepares and the
-- revisions it was requested against.
CREATE TABLE learning_jobs (
    job_id TEXT PRIMARY KEY REFERENCES jobs(id) ON DELETE CASCADE,
    program_id TEXT NOT NULL REFERENCES learning_programs(id) ON DELETE CASCADE,
    base_revision_number INTEGER NOT NULL DEFAULT 0 CHECK(base_revision_number >= 0),
    content_revision INTEGER
);
CREATE INDEX learning_jobs_program ON learning_jobs(program_id);

-- Deleting a course removes its jobs, with their events and checkpoints.
CREATE TRIGGER learning_program_jobs_deleted BEFORE DELETE ON learning_programs
BEGIN
    DELETE FROM jobs WHERE id IN (SELECT job_id FROM learning_jobs WHERE program_id = OLD.id);
END;
