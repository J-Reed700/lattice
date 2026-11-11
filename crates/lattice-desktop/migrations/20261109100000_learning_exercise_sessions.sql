-- Exercise sessions, their scorecards, the course's practice profile and fit, and the
-- new activity kind they run under.
--
-- The kind lives in a CHECK on learning_practical_activities, which SQLite keeps in the
-- stored CREATE TABLE text. sqlx-sqlite runs every migration inside a transaction
-- (it ignores `-- no-transaction`), where a DROP of a parent table cascades deletes
-- to its children, so the table is never rebuilt. The stored text is edited in place,
-- by the procedure 20261016100000_learning_delete_cascades.sql uses and explains.

PRAGMA writable_schema = ON;

UPDATE sqlite_schema
SET sql = replace(
    sql,
    '''project'',''interview'',''conversation'',''writing_revision'',''custom''',
    '''project'',''interview'',''conversation'',''writing_revision'',''custom'',''exercise'''
)
WHERE type = 'table' AND name = 'learning_practical_activities';

UPDATE sqlite_schema
SET sql = replace(
    sql,
    'material = ''passage'' OR material GLOB ''code:?*''',
    'material = ''passage'' OR material GLOB ''code:?*'' OR material GLOB ''problem:?*'''
)
WHERE type = 'table' AND name = 'learning_drill_pattern_schedules';

PRAGMA writable_schema = RESET;

-- Fail (and roll back) unless the new kind is in force. Creating and dropping a
-- main-schema table also bumps the schema cookie, so pooled connections reload.
CREATE TABLE _lattice_exercise_kind_guard (ok INTEGER NOT NULL CHECK (ok = 1));
INSERT INTO _lattice_exercise_kind_guard (ok)
SELECT instr(sql, '''exercise''') > 0
FROM sqlite_schema
WHERE type = 'table' AND name = 'learning_practical_activities';
DROP TABLE _lattice_exercise_kind_guard;

CREATE TABLE _lattice_problem_material_guard (ok INTEGER NOT NULL CHECK (ok = 1));
INSERT INTO _lattice_problem_material_guard (ok)
SELECT instr(sql, 'GLOB ''problem:?*''') > 0
FROM sqlite_schema
WHERE type = 'table' AND name = 'learning_drill_pattern_schedules';
DROP TABLE _lattice_problem_material_guard;

CREATE TABLE learning_exercise_sessions (
    id TEXT PRIMARY KEY REFERENCES learning_simulation_sessions(id) ON DELETE CASCADE,
    program_id TEXT NOT NULL REFERENCES learning_programs(id) ON DELETE CASCADE,
    activity_id TEXT NOT NULL REFERENCES learning_practical_activities(id) ON DELETE CASCADE,
    kind TEXT NOT NULL,
    form TEXT NOT NULL CHECK(form IN ('build','repair','complete','arrange')),
    setting TEXT NOT NULL CHECK(setting IN ('coached','mock','watched')),
    stage TEXT NOT NULL CHECK(stage IN ('understand','examples','plan','make','check','review','reproduce','locate','fix','verify','build')),
    budget_seconds INTEGER CHECK(budget_seconds IS NULL OR budget_seconds > 0),
    active_ms INTEGER NOT NULL DEFAULT 0 CHECK(active_ms >= 0),
    opened_at INTEGER,
    draft_json TEXT NOT NULL,
    pending_cue_json TEXT,
    submitted_at INTEGER,
    solution_seen_at INTEGER,
    finished_at INTEGER,
    revision INTEGER NOT NULL DEFAULT 0 CHECK(revision >= 0),
    created_at INTEGER NOT NULL
);
CREATE INDEX learning_exercise_sessions_program ON learning_exercise_sessions(program_id, created_at DESC);

CREATE TABLE learning_exercise_session_events (
    session_id TEXT NOT NULL REFERENCES learning_exercise_sessions(id) ON DELETE CASCADE,
    ordinal INTEGER NOT NULL CHECK(ordinal >= 0),
    kind TEXT NOT NULL CHECK(kind IN ('stage','check','example','hint','setting','opened','closed','draft_seen','solution_seen')),
    payload_json TEXT NOT NULL,
    at INTEGER NOT NULL,
    PRIMARY KEY(session_id, ordinal)
);

CREATE TABLE learning_coding_examples (
    session_id TEXT NOT NULL REFERENCES learning_exercise_sessions(id) ON DELETE CASCADE,
    ordinal INTEGER NOT NULL CHECK(ordinal >= 0),
    args_json TEXT NOT NULL,
    expected TEXT NOT NULL,
    verdict TEXT NOT NULL CHECK(verdict IN ('right','wrong','invalid')),
    reference_actual TEXT,
    why_invalid TEXT,
    catches_json TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    PRIMARY KEY(session_id, ordinal)
);

ALTER TABLE learning_practical_runs ADD COLUMN exercise_session_id TEXT REFERENCES learning_exercise_sessions(id) ON DELETE SET NULL;
ALTER TABLE learning_practical_runs ADD COLUMN scope TEXT CHECK(scope IS NULL OR scope IN ('shown','all','input'));
CREATE INDEX learning_practical_runs_exercise_session ON learning_practical_runs(exercise_session_id, created_at) WHERE exercise_session_id IS NOT NULL;
ALTER TABLE learning_practical_run_checks ADD COLUMN detail_json TEXT;

CREATE TABLE learning_scorecards (
    session_id TEXT NOT NULL REFERENCES learning_simulation_sessions(id) ON DELETE CASCADE,
    criterion_id TEXT NOT NULL,
    ordinal INTEGER NOT NULL CHECK(ordinal >= 0),
    title TEXT NOT NULL,
    status TEXT NOT NULL CHECK(status IN ('observed','insufficient_evidence')),
    points INTEGER CHECK(points IS NULL OR points >= 0),
    max_points INTEGER NOT NULL CHECK(max_points > 0),
    evidence_json TEXT NOT NULL,
    feedback TEXT NOT NULL,
    next_step TEXT NOT NULL,
    PRIMARY KEY(session_id, criterion_id),
    CHECK((status = 'observed') = (points IS NOT NULL)),
    CHECK(points IS NULL OR points <= max_points)
);

CREATE TABLE learning_exercise_debriefs (
    session_id TEXT PRIMARY KEY REFERENCES learning_exercise_sessions(id) ON DELETE CASCADE,
    approach TEXT NOT NULL,
    complexity TEXT NOT NULL,
    ratings_json TEXT NOT NULL,
    model TEXT NOT NULL,
    written_at INTEGER NOT NULL
);

CREATE TABLE learning_practice_profiles (
    program_id TEXT PRIMARY KEY REFERENCES learning_programs(id) ON DELETE CASCADE,
    details TEXT NOT NULL DEFAULT '',
    language TEXT,
    interview_at INTEGER,
    company TEXT,
    role TEXT,
    revision INTEGER NOT NULL DEFAULT 0 CHECK(revision >= 0),
    updated_at INTEGER NOT NULL
);

CREATE TABLE learning_practice_fits (
    program_id TEXT NOT NULL REFERENCES learning_programs(id) ON DELETE CASCADE,
    kind TEXT NOT NULL,
    basis TEXT NOT NULL,
    fits INTEGER NOT NULL CHECK(fits IN (0,1)),
    reason TEXT NOT NULL,
    model TEXT NOT NULL,
    decided_at INTEGER NOT NULL,
    PRIMARY KEY (program_id, kind)
);
