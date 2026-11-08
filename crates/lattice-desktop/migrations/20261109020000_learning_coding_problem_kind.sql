-- Add coding_problem to the kind CHECK constraint on learning_practical_activities.
-- SQLite cannot alter CHECK constraints in place, and dropping the table cascades
-- deletes to child tables because sqlx runs migrations in transactions where
-- foreign keys cannot be turned off. Child tables are preserved in TEMP tables
-- while the activity table is rebuilt.

CREATE TEMP TABLE learning_practical_files_temp AS SELECT * FROM learning_practical_files;
CREATE TEMP TABLE learning_practical_drafts_temp AS SELECT * FROM learning_practical_drafts;
CREATE TEMP TABLE learning_practical_runs_temp AS SELECT * FROM learning_practical_runs;
CREATE TEMP TABLE learning_practical_run_checks_temp AS SELECT * FROM learning_practical_run_checks;
CREATE TEMP TABLE learning_simulation_sessions_temp AS SELECT * FROM learning_simulation_sessions;
CREATE TEMP TABLE learning_simulation_turns_temp AS SELECT * FROM learning_simulation_turns;

CREATE TABLE learning_practical_activities_new (
    id TEXT PRIMARY KEY,
    program_id TEXT NOT NULL REFERENCES learning_programs(id) ON DELETE CASCADE,
    lesson_id TEXT NOT NULL,
    predecessor_id TEXT,
    kind TEXT NOT NULL CHECK(kind IN (
        'code_lab','debugging','code_review','incident','system_design',
        'project','interview','conversation','writing_revision','custom','coding_problem'
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
    builtin_runtime TEXT
        CHECK(builtin_runtime IS NULL OR (builtin_runtime IN ('javascript','python') AND runtime_kind='none')),
    builtin_runtime_version TEXT
        CHECK((builtin_runtime IS NULL AND builtin_runtime_version IS NULL)
           OR (builtin_runtime IS NOT NULL AND builtin_runtime_version IS NOT NULL AND length(builtin_runtime_version) BETWEEN 1 AND 128)),
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

INSERT INTO learning_practical_activities_new SELECT * FROM learning_practical_activities;
DROP TABLE learning_practical_activities;
ALTER TABLE learning_practical_activities_new RENAME TO learning_practical_activities;
CREATE INDEX learning_practical_activities_program
    ON learning_practical_activities(program_id,lesson_id,created_at,id);

INSERT INTO learning_practical_files SELECT * FROM learning_practical_files_temp;
DROP TABLE learning_practical_files_temp;

INSERT INTO learning_practical_drafts SELECT * FROM learning_practical_drafts_temp;
DROP TABLE learning_practical_drafts_temp;

INSERT INTO learning_practical_runs SELECT * FROM learning_practical_runs_temp;
DROP TABLE learning_practical_runs_temp;

INSERT INTO learning_practical_run_checks SELECT * FROM learning_practical_run_checks_temp;
DROP TABLE learning_practical_run_checks_temp;

INSERT INTO learning_simulation_sessions SELECT * FROM learning_simulation_sessions_temp;
DROP TABLE learning_simulation_sessions_temp;

INSERT INTO learning_simulation_turns SELECT * FROM learning_simulation_turns_temp;
DROP TABLE learning_simulation_turns_temp;
