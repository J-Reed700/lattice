-- Canvas scene state and named snapshots belong to a Learning Studio program.
-- Mutations carry client-stable operation IDs so retries are transactionally safe.
CREATE TABLE learning_canvases (
    id TEXT PRIMARY KEY,
    program_id TEXT NOT NULL REFERENCES learning_programs(id) ON DELETE CASCADE,
    lesson_id TEXT,
    title TEXT NOT NULL,
    description TEXT NOT NULL,
    scene_json TEXT NOT NULL,
    element_count INTEGER NOT NULL CHECK(element_count >= 0),
    revision INTEGER NOT NULL DEFAULT 0 CHECK(revision >= 0),
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    UNIQUE(program_id, id),
    FOREIGN KEY(program_id, lesson_id)
        REFERENCES learning_lessons(program_id, id) ON DELETE CASCADE
);
CREATE INDEX learning_canvases_program ON learning_canvases(program_id, updated_at DESC);

CREATE TABLE learning_canvas_snapshots (
    id TEXT PRIMARY KEY,
    program_id TEXT NOT NULL,
    canvas_id TEXT NOT NULL,
    name TEXT NOT NULL,
    title TEXT NOT NULL,
    description TEXT NOT NULL,
    scene_json TEXT NOT NULL,
    element_count INTEGER NOT NULL CHECK(element_count >= 0),
    canvas_revision INTEGER NOT NULL CHECK(canvas_revision >= 0),
    created_at INTEGER NOT NULL,
    FOREIGN KEY(program_id, canvas_id)
        REFERENCES learning_canvases(program_id, id) ON DELETE CASCADE
);
CREATE INDEX learning_canvas_snapshots_canvas
    ON learning_canvas_snapshots(program_id, canvas_id, created_at, id);

CREATE TABLE learning_canvas_operations (
    operation_id TEXT PRIMARY KEY,
    program_id TEXT NOT NULL,
    canvas_id TEXT NOT NULL,
    kind TEXT NOT NULL CHECK(kind IN ('create', 'save', 'snapshot', 'restore')),
    payload_hash TEXT NOT NULL,
    result_revision INTEGER NOT NULL CHECK(result_revision >= 0),
    result_snapshot_id TEXT,
    created_at INTEGER NOT NULL,
    FOREIGN KEY(program_id, canvas_id)
        REFERENCES learning_canvases(program_id, id) ON DELETE CASCADE
);
CREATE INDEX learning_canvas_operations_canvas
    ON learning_canvas_operations(program_id, canvas_id, created_at);
