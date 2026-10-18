CREATE TABLE learning_tangents (
    conversation_id TEXT PRIMARY KEY REFERENCES conversations(id) ON DELETE CASCADE,
    program_id TEXT NOT NULL REFERENCES learning_programs(id) ON DELETE CASCADE,
    module_id TEXT NOT NULL,
    lesson_id TEXT NOT NULL,
    section_index INTEGER,
    section_title TEXT,
    selected_text TEXT NOT NULL,
    context_message_count INTEGER NOT NULL DEFAULT 1,
    created_at INTEGER NOT NULL
);

CREATE INDEX idx_learning_tangents_program
    ON learning_tangents(program_id, created_at DESC);

CREATE INDEX idx_learning_tangents_lesson
    ON learning_tangents(program_id, lesson_id, created_at DESC);
