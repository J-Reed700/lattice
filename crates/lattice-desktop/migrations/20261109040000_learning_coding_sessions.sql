CREATE TABLE IF NOT EXISTS learning_coding_sessions (
    id TEXT PRIMARY KEY REFERENCES learning_simulation_sessions(id) ON DELETE CASCADE,
    program_id TEXT NOT NULL REFERENCES learning_programs(id) ON DELETE CASCADE,
    activity_id TEXT NOT NULL REFERENCES learning_practical_activities(id) ON DELETE CASCADE,
    task TEXT NOT NULL CHECK (task IN ('problem', 'bug_fix', 'project')),
    setting TEXT NOT NULL CHECK (setting IN ('coached', 'mock', 'watched')),
    stage TEXT NOT NULL CHECK (stage IN ('understand', 'plan', 'code', 'verify')),
    budget_seconds INTEGER,
    active_ms INTEGER NOT NULL DEFAULT 0,
    opened_at INTEGER,
    finished_at INTEGER,
    revision INTEGER NOT NULL DEFAULT 1
);

CREATE INDEX IF NOT EXISTS idx_learning_coding_sessions_program
    ON learning_coding_sessions(program_id, activity_id);

CREATE TABLE IF NOT EXISTS learning_coding_events (
    session_id TEXT NOT NULL REFERENCES learning_coding_sessions(id) ON DELETE CASCADE,
    ordinal INTEGER NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('stage', 'run', 'example', 'hint', 'setting', 'opened', 'closed', 'draft_seen')),
    payload_json TEXT NOT NULL DEFAULT '{}',
    at INTEGER NOT NULL,
    PRIMARY KEY (session_id, ordinal)
);

CREATE TABLE IF NOT EXISTS learning_coding_examples (
    session_id TEXT NOT NULL REFERENCES learning_coding_sessions(id) ON DELETE CASCADE,
    ordinal INTEGER NOT NULL,
    args_json TEXT NOT NULL,
    expected_json TEXT NOT NULL,
    verdict TEXT NOT NULL CHECK (verdict IN ('right', 'wrong', 'invalid')),
    catches_json TEXT NOT NULL DEFAULT '[]',
    PRIMARY KEY (session_id, ordinal)
);

ALTER TABLE learning_practical_runs ADD COLUMN coding_session_id TEXT REFERENCES learning_coding_sessions(id) ON DELETE SET NULL;
ALTER TABLE learning_practical_runs ADD COLUMN scope TEXT CHECK (scope IS NULL OR scope IN ('examples', 'all', 'input'));
