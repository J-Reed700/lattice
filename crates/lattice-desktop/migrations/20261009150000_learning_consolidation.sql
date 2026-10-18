-- One idempotency ledger replaces the thirteen per-workflow operation tables.
-- `scope` names the workflow, `subject_id` the entity a request targets and
-- `result_json` what the first attempt produced. Ledger rows are retry keys,
-- not history, so the old rows are not carried over.
CREATE TABLE learning_operations (
    operation_id TEXT PRIMARY KEY,
    program_id TEXT REFERENCES learning_programs(id) ON DELETE CASCADE,
    scope TEXT NOT NULL,
    kind TEXT NOT NULL,
    subject_id TEXT,
    payload_hash TEXT NOT NULL,
    result_json TEXT NOT NULL,
    created_at INTEGER NOT NULL
);
CREATE INDEX learning_operations_program
    ON learning_operations(program_id, scope, created_at);

-- Refresh checks keep their own operation ID but no longer hang off a ledger
-- table, so they survive the ledger's replacement.
CREATE TEMP TABLE learning_source_refresh_checks_pre_ledger AS
    SELECT * FROM learning_source_refresh_checks;
DROP TABLE learning_source_refresh_checks;
CREATE TABLE learning_source_refresh_checks (
    operation_id TEXT PRIMARY KEY,
    program_id TEXT NOT NULL,
    source_id TEXT NOT NULL,
    status TEXT NOT NULL CHECK(status IN ('unchanged','update_available','failed')),
    checked_at INTEGER NOT NULL,
    active_digest TEXT,
    pending_version_id TEXT,
    message TEXT,
    FOREIGN KEY(program_id,source_id)
        REFERENCES learning_source_library(program_id,id) ON DELETE CASCADE
);
INSERT INTO learning_source_refresh_checks
    SELECT * FROM learning_source_refresh_checks_pre_ledger;
DROP TABLE learning_source_refresh_checks_pre_ledger;
CREATE INDEX learning_source_checks_history
    ON learning_source_refresh_checks(program_id,source_id,checked_at DESC);

DROP TABLE learning_assessment_operations;
DROP TABLE learning_canvas_operations;
DROP TABLE learning_curriculum_operations;
DROP TABLE learning_diagnostic_operations;
DROP TABLE learning_portability_operations;
DROP TABLE learning_practical_activity_operations;
DROP TABLE learning_practical_draft_operations;
DROP TABLE learning_practical_run_operations;
DROP TABLE learning_practice_operations;
DROP TABLE learning_recall_operations;
DROP TABLE learning_runtime_profile_operations;
DROP TABLE learning_simulation_operations;
DROP TABLE learning_source_operations;

-- Study folds into Learning Recall and FSRS 6 becomes the only scheduler. The
-- FSRS memory moves onto the canonical card row, so a deck card and a program
-- recall card are scheduled by the same columns.
ALTER TABLE study_cards ADD COLUMN stability REAL;
ALTER TABLE study_cards ADD COLUMN difficulty REAL;
ALTER TABLE study_cards ADD COLUMN last_reviewed_at INTEGER;

-- Cards already on FSRS keep their memory.
UPDATE study_cards SET
    stability = (SELECT json_extract(p.scheduler_state_json, '$.stability')
                 FROM learning_recall_card_profiles p WHERE p.card_id = study_cards.id),
    difficulty = (SELECT json_extract(p.scheduler_state_json, '$.difficulty')
                  FROM learning_recall_card_profiles p WHERE p.card_id = study_cards.id),
    last_reviewed_at = (SELECT json_extract(p.scheduler_state_json, '$.last_reviewed_at')
                        FROM learning_recall_card_profiles p WHERE p.card_id = study_cards.id)
WHERE scheduler_version = 'fsrs_6_v1';

-- Cards on the retired expanding scheduler start FSRS as new cards: no memory,
-- their current due date, and their review history and counts kept.
UPDATE study_cards SET
    stability = NULL,
    difficulty = NULL,
    last_reviewed_at = (SELECT max(r.reviewed_at) FROM study_reviews r WHERE r.card_id = study_cards.id)
WHERE scheduler_version = 'expanding_v1';

ALTER TABLE study_cards DROP COLUMN scheduler_version;
ALTER TABLE study_reviews DROP COLUMN scheduler_version;
ALTER TABLE learning_recall_card_profiles DROP COLUMN scheduler_version;
ALTER TABLE learning_recall_card_profiles DROP COLUMN scheduler_state_json;
DROP TABLE learning_recall_scheduler_transitions;
DROP TABLE learning_recall_scheduler_migrations;

-- Learning ranks document snapshots through the library's own chunks and
-- vectors, and everything else by keyword, so its private vector table goes.
DROP TABLE learning_source_retrieval_index;
