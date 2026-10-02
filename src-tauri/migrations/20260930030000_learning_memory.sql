-- Learning Studio links to canonical journal pages and Study decks. The
-- program owns only these links and draft/origin records; deleting a program
-- leaves journals, notes, decks, cards, and review history intact.
CREATE TABLE learning_memory (
    program_id TEXT PRIMARY KEY REFERENCES learning_programs(id) ON DELETE CASCADE,
    journal_id TEXT UNIQUE REFERENCES journals(id) ON DELETE SET NULL,
    deck_id TEXT UNIQUE REFERENCES study_decks(id) ON DELETE SET NULL,
    created_at INTEGER NOT NULL
);

CREATE TABLE learning_lesson_note_links (
    program_id TEXT NOT NULL REFERENCES learning_memory(program_id) ON DELETE CASCADE,
    lesson_id TEXT NOT NULL,
    note_id TEXT NOT NULL UNIQUE REFERENCES daily_notes_workspace(id) ON DELETE CASCADE,
    created_at INTEGER NOT NULL,
    PRIMARY KEY(program_id, lesson_id),
    FOREIGN KEY(program_id, lesson_id)
        REFERENCES learning_lessons(program_id, id) ON DELETE CASCADE
);

CREATE TABLE learning_card_drafts (
    id TEXT PRIMARY KEY,
    program_id TEXT NOT NULL REFERENCES learning_memory(program_id) ON DELETE CASCADE,
    lesson_id TEXT NOT NULL,
    question TEXT NOT NULL,
    answer TEXT NOT NULL,
    explanation TEXT NOT NULL,
    source_ids_json TEXT NOT NULL,
    origin TEXT NOT NULL CHECK(origin IN ('generated', 'manual')),
    status TEXT NOT NULL DEFAULT 'pending' CHECK(status IN ('pending', 'accepted', 'discarded')),
    accepted_card_id TEXT REFERENCES study_cards(id) ON DELETE SET NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    FOREIGN KEY(program_id, lesson_id)
        REFERENCES learning_lessons(program_id, id) ON DELETE CASCADE
);
CREATE INDEX learning_card_drafts_program_status
    ON learning_card_drafts(program_id, status, created_at);

CREATE TABLE learning_card_origins (
    card_id TEXT PRIMARY KEY REFERENCES study_cards(id) ON DELETE CASCADE,
    program_id TEXT NOT NULL REFERENCES learning_memory(program_id) ON DELETE CASCADE,
    lesson_id TEXT NOT NULL,
    origin TEXT NOT NULL CHECK(origin IN ('generated', 'manual')),
    source_ids_json TEXT NOT NULL,
    accepted_at INTEGER NOT NULL,
    FOREIGN KEY(program_id, lesson_id)
        REFERENCES learning_lessons(program_id, id) ON DELETE CASCADE
);

ALTER TABLE study_cards ADD COLUMN format TEXT NOT NULL DEFAULT 'multiple_choice'
    CHECK(format IN ('multiple_choice', 'question_answer'));
ALTER TABLE study_cards ADD COLUMN scheduler_version TEXT NOT NULL DEFAULT 'expanding_v1'
    CHECK(scheduler_version IN ('expanding_v1'));
ALTER TABLE study_reviews ADD COLUMN scheduler_version TEXT NOT NULL DEFAULT 'expanding_v1'
    CHECK(scheduler_version IN ('expanding_v1'));
