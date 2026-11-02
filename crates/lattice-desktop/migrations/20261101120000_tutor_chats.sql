-- A tutor chat is a conversation bound to a course and, when opened from a
-- lesson, to that lesson. Deleting a course leaves its chats as ordinary chats.
-- The Tutor tab lists a course's chats, so the lookup is indexed.
ALTER TABLE conversations ADD COLUMN program_id TEXT
    REFERENCES learning_programs(id) ON DELETE SET NULL;
ALTER TABLE conversations ADD COLUMN lesson_id TEXT
    REFERENCES learning_lessons(id) ON DELETE SET NULL;

CREATE INDEX IF NOT EXISTS idx_conversations_program
    ON conversations(program_id, updated_at DESC)
    WHERE program_id IS NOT NULL;
