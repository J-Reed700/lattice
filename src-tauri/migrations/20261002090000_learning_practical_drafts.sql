CREATE TABLE learning_practical_drafts (
    program_id TEXT NOT NULL REFERENCES learning_programs(id) ON DELETE CASCADE,
    activity_id TEXT NOT NULL REFERENCES learning_practical_activities(id) ON DELETE CASCADE,
    activity_revision INTEGER NOT NULL CHECK (activity_revision >= 0),
    draft_revision INTEGER NOT NULL CHECK (draft_revision >= 0),
    files_json TEXT NOT NULL,
    updated_at INTEGER NOT NULL,
    PRIMARY KEY (program_id, activity_id, activity_revision),
    FOREIGN KEY (program_id, activity_id)
        REFERENCES learning_practical_activities(program_id, id) ON DELETE CASCADE
);

CREATE TABLE learning_practical_draft_operations (
    operation_id TEXT PRIMARY KEY,
    program_id TEXT NOT NULL REFERENCES learning_programs(id) ON DELETE CASCADE,
    activity_id TEXT NOT NULL REFERENCES learning_practical_activities(id) ON DELETE CASCADE,
    activity_revision INTEGER NOT NULL,
    payload_hash TEXT NOT NULL,
    result_revision INTEGER NOT NULL,
    created_at INTEGER NOT NULL,
    FOREIGN KEY (program_id, activity_id)
        REFERENCES learning_practical_activities(program_id, id) ON DELETE CASCADE
);

CREATE INDEX learning_practical_draft_operations_scope
    ON learning_practical_draft_operations(program_id, activity_id, activity_revision);
