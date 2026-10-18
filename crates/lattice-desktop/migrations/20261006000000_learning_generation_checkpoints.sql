-- Worker-owned receipts are separate rows so completing a claim does not
-- rewrite a whole lesson/report, and concurrent completions cannot lose updates.
CREATE TABLE learning_generation_checkpoints (
    job_id TEXT NOT NULL REFERENCES learning_generation_jobs(id) ON DELETE CASCADE,
    lesson_id TEXT NOT NULL REFERENCES learning_lessons(id) ON DELETE CASCADE,
    checkpoint_key TEXT NOT NULL,
    payload_json TEXT NOT NULL CHECK (json_valid(payload_json)),
    updated_at INTEGER NOT NULL,
    PRIMARY KEY (job_id, lesson_id, checkpoint_key)
);
