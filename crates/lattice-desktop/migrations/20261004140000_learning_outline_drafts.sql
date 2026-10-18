-- Preserve raw authored citations and review findings independently of ready lessons.
CREATE TABLE learning_outline_drafts (
    program_id TEXT PRIMARY KEY REFERENCES learning_programs(id) ON DELETE CASCADE,
    state_json TEXT NOT NULL CHECK(json_valid(state_json))
);
CREATE TABLE learning_outline_checkpoints (
    program_id TEXT NOT NULL REFERENCES learning_programs(id) ON DELETE CASCADE,
    revision INTEGER NOT NULL,
    state_json TEXT NOT NULL CHECK(json_valid(state_json)),
    created_at INTEGER NOT NULL,
    PRIMARY KEY(program_id, revision)
);
