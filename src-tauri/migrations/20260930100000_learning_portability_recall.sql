-- Portable, checksummed program packs are always previewed before application.
CREATE TABLE learning_pack_exports (
    id TEXT PRIMARY KEY,
    program_id TEXT NOT NULL REFERENCES learning_programs(id) ON DELETE CASCADE,
    operation_id TEXT NOT NULL UNIQUE,
    payload_hash TEXT NOT NULL,
    format_version INTEGER NOT NULL CHECK(format_version >= 1),
    root_sha256 TEXT NOT NULL,
    destination_path TEXT NOT NULL,
    privacy_manifest_json TEXT NOT NULL,
    entry_manifest_json TEXT NOT NULL,
    manifest_json TEXT NOT NULL,
    created_at INTEGER NOT NULL
);

CREATE TABLE learning_pack_import_previews (
    id TEXT PRIMARY KEY,
    operation_id TEXT NOT NULL UNIQUE,
    payload_hash TEXT NOT NULL,
    source_path TEXT NOT NULL,
    root_sha256 TEXT NOT NULL,
    program_id TEXT NOT NULL,
    program_title TEXT NOT NULL,
    conflict_policy TEXT NOT NULL CHECK(conflict_policy IN (
        'create_copy','merge_safe','replace_after_backup'
    )),
    conflicts_json TEXT NOT NULL,
    changes_json TEXT NOT NULL,
    warnings_json TEXT NOT NULL,
    manifest_json TEXT NOT NULL,
    status TEXT NOT NULL CHECK(status IN ('pending','applied','cancelled','stale')),
    created_at INTEGER NOT NULL,
    decided_at INTEGER
);

CREATE TABLE learning_pack_imports (
    id TEXT PRIMARY KEY,
    preview_id TEXT NOT NULL UNIQUE REFERENCES learning_pack_import_previews(id) ON DELETE RESTRICT,
    operation_id TEXT NOT NULL UNIQUE,
    payload_hash TEXT NOT NULL,
    imported_program_id TEXT NOT NULL REFERENCES learning_programs(id) ON DELETE CASCADE,
    backup_id TEXT,
    applied_changes_json TEXT NOT NULL,
    imported_at INTEGER NOT NULL
);

-- A source may disappear from active use while immutable versions remain for
-- old citations, attempts, and exports.
ALTER TABLE learning_source_library ADD COLUMN deleted_at INTEGER;
ALTER TABLE learning_source_library ADD COLUMN deletion_reason TEXT;

-- Rebuild canonical Study cards and their children together so both scheduler
-- versions remain representable without losing review history or Studio links.
CREATE TEMP TABLE study_reviews_pre_recall AS SELECT * FROM study_reviews;
DROP TABLE study_reviews;
CREATE TEMP TABLE learning_card_drafts_pre_recall AS SELECT * FROM learning_card_drafts;
DROP TABLE learning_card_drafts;
CREATE TEMP TABLE learning_card_origins_pre_recall AS SELECT * FROM learning_card_origins;
DROP TABLE learning_card_origins;

CREATE TABLE study_cards_recall (
    id TEXT PRIMARY KEY,
    deck_id TEXT NOT NULL REFERENCES study_decks(id) ON DELETE CASCADE,
    question TEXT NOT NULL,
    answer TEXT NOT NULL,
    options_json TEXT NOT NULL,
    correct_index INTEGER NOT NULL CHECK(correct_index BETWEEN 0 AND 7),
    explanation TEXT NOT NULL,
    source_json TEXT NOT NULL,
    topic TEXT NOT NULL,
    due_at INTEGER NOT NULL,
    interval_days INTEGER NOT NULL DEFAULT 0,
    review_count INTEGER NOT NULL DEFAULT 0,
    lapses INTEGER NOT NULL DEFAULT 0,
    format TEXT NOT NULL DEFAULT 'multiple_choice'
        CHECK(format IN ('multiple_choice','question_answer')),
    scheduler_version TEXT NOT NULL DEFAULT 'expanding_v1'
        CHECK(scheduler_version IN ('expanding_v1','fsrs_6_v1'))
) STRICT;
INSERT INTO study_cards_recall(
    id,deck_id,question,answer,options_json,correct_index,explanation,source_json,
    topic,due_at,interval_days,review_count,lapses,format,scheduler_version
)
SELECT id,deck_id,question,answer,options_json,correct_index,explanation,source_json,
       topic,due_at,interval_days,review_count,lapses,format,scheduler_version
FROM study_cards;
DROP TABLE study_cards;
ALTER TABLE study_cards_recall RENAME TO study_cards;
CREATE INDEX study_cards_deck_due ON study_cards(deck_id, due_at);

-- Keep the canonical review log intact while allowing Learning Recall to
-- identify the scheduler used for each new review. Existing rows stay on the
-- Expanding v1 scheduler.
CREATE TABLE study_reviews (
    id TEXT PRIMARY KEY,
    card_id TEXT NOT NULL REFERENCES study_cards(id) ON DELETE CASCADE,
    reviewed_at INTEGER NOT NULL,
    rating TEXT NOT NULL CHECK(rating IN ('again','hard','good','easy')),
    mode TEXT NOT NULL CHECK(mode IN ('flashcard','quiz')),
    correct INTEGER,
    selected_option INTEGER,
    scheduler_version TEXT NOT NULL DEFAULT 'expanding_v1'
        CHECK(scheduler_version IN ('expanding_v1','fsrs_6_v1'))
) STRICT;
INSERT INTO study_reviews(id,card_id,reviewed_at,rating,mode,correct,selected_option,scheduler_version)
    SELECT id,card_id,reviewed_at,rating,mode,correct,selected_option,'expanding_v1'
    FROM study_reviews_pre_recall;
DROP TABLE study_reviews_pre_recall;
CREATE INDEX study_reviews_card ON study_reviews(card_id);

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
INSERT INTO learning_card_drafts SELECT * FROM learning_card_drafts_pre_recall;
DROP TABLE learning_card_drafts_pre_recall;
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
INSERT INTO learning_card_origins SELECT * FROM learning_card_origins_pre_recall;
DROP TABLE learning_card_origins_pre_recall;

CREATE TABLE learning_source_selectors (
    id TEXT PRIMARY KEY,
    program_id TEXT NOT NULL,
    source_id TEXT NOT NULL,
    source_version_id TEXT NOT NULL,
    exact_text TEXT NOT NULL,
    prefix_text TEXT NOT NULL,
    suffix_text TEXT NOT NULL,
    start_byte INTEGER CHECK(start_byte IS NULL OR start_byte >= 0),
    end_byte INTEGER CHECK(end_byte IS NULL OR end_byte >= 0),
    candidate_count INTEGER NOT NULL CHECK(candidate_count >= 0),
    status TEXT NOT NULL CHECK(status IN ('exact','contextual','ambiguous','not_found')),
    created_at INTEGER NOT NULL,
    FOREIGN KEY(program_id,source_id)
        REFERENCES learning_source_library(program_id,id) ON DELETE CASCADE,
    FOREIGN KEY(program_id,source_version_id)
        REFERENCES learning_source_versions(program_id,id) ON DELETE CASCADE,
    CHECK(
        (status IN ('exact','contextual') AND start_byte IS NOT NULL AND end_byte IS NOT NULL AND end_byte > start_byte)
        OR (status IN ('ambiguous','not_found') AND start_byte IS NULL AND end_byte IS NULL)
    )
);

CREATE TABLE learning_source_retrieval_index (
    program_id TEXT NOT NULL,
    source_version_id TEXT NOT NULL,
    chunk_ordinal INTEGER NOT NULL CHECK(chunk_ordinal >= 0),
    chunk_text TEXT NOT NULL,
    start_byte INTEGER NOT NULL CHECK(start_byte >= 0),
    end_byte INTEGER NOT NULL CHECK(end_byte >= start_byte),
    embedding_model TEXT,
    embedding_json TEXT,
    created_at INTEGER NOT NULL,
    PRIMARY KEY(program_id,source_version_id,chunk_ordinal),
    FOREIGN KEY(program_id,source_version_id)
        REFERENCES learning_source_versions(program_id,id) ON DELETE CASCADE
);

-- History and duplicate suggestions are additive, keeping the canonical Study
-- tables readable by older data paths. Expanded formats use the flexible card
-- kind here until the canonical card table can be rebuilt on a major migration.
CREATE TABLE learning_recall_card_profiles (
    card_id TEXT PRIMARY KEY REFERENCES study_cards(id) ON DELETE CASCADE,
    program_id TEXT NOT NULL REFERENCES learning_programs(id) ON DELETE CASCADE,
    format TEXT NOT NULL CHECK(format IN (
        'multiple_choice','question_answer','cloze','reverse','code_prediction','reconstruction'
    )),
    prompt_json TEXT NOT NULL,
    answer_json TEXT NOT NULL,
    source_version_ids_json TEXT NOT NULL DEFAULT '[]',
    scheduler_version TEXT NOT NULL CHECK(scheduler_version IN ('expanding_v1','fsrs_6_v1')),
    scheduler_state_json TEXT NOT NULL,
    content_revision INTEGER NOT NULL DEFAULT 1 CHECK(content_revision >= 1),
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE learning_recall_card_versions (
    card_id TEXT NOT NULL REFERENCES learning_recall_card_profiles(card_id) ON DELETE CASCADE,
    revision INTEGER NOT NULL CHECK(revision >= 1),
    format TEXT NOT NULL CHECK(format IN (
        'multiple_choice','question_answer','cloze','reverse','code_prediction','reconstruction'
    )),
    prompt_json TEXT NOT NULL,
    answer_json TEXT NOT NULL,
    source_version_ids_json TEXT NOT NULL,
    change_reason TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    PRIMARY KEY(card_id,revision)
);

CREATE TABLE learning_recall_duplicate_suggestions (
    id TEXT PRIMARY KEY,
    program_id TEXT NOT NULL REFERENCES learning_programs(id) ON DELETE CASCADE,
    card_id TEXT NOT NULL REFERENCES study_cards(id) ON DELETE CASCADE,
    possible_duplicate_card_id TEXT NOT NULL REFERENCES study_cards(id) ON DELETE CASCADE,
    reason TEXT NOT NULL,
    similarity REAL CHECK(similarity IS NULL OR (similarity >= 0 AND similarity <= 1)),
    status TEXT NOT NULL CHECK(status IN ('pending','confirmed','dismissed')),
    created_at INTEGER NOT NULL,
    decided_at INTEGER,
    CHECK(card_id < possible_duplicate_card_id)
);

CREATE TABLE learning_recall_scheduler_transitions (
    id TEXT PRIMARY KEY,
    card_id TEXT NOT NULL REFERENCES study_cards(id) ON DELETE CASCADE,
    review_id TEXT REFERENCES study_reviews(id) ON DELETE SET NULL,
    scheduler_version TEXT NOT NULL CHECK(scheduler_version IN ('expanding_v1','fsrs_6_v1')),
    prior_state_json TEXT NOT NULL,
    rating TEXT NOT NULL CHECK(rating IN ('again','hard','good','easy')),
    next_state_json TEXT NOT NULL,
    due_at INTEGER NOT NULL,
    created_at INTEGER NOT NULL,
    UNIQUE(card_id,review_id,scheduler_version)
);

CREATE TABLE learning_recall_scheduler_migrations (
    id TEXT PRIMARY KEY,
    operation_id TEXT NOT NULL UNIQUE,
    program_id TEXT NOT NULL REFERENCES learning_programs(id) ON DELETE CASCADE,
    card_id TEXT NOT NULL REFERENCES study_cards(id) ON DELETE CASCADE,
    from_version TEXT NOT NULL CHECK(from_version IN ('expanding_v1','fsrs_6_v1')),
    to_version TEXT NOT NULL CHECK(to_version IN ('expanding_v1','fsrs_6_v1')),
    prior_state_json TEXT NOT NULL,
    next_state_json TEXT NOT NULL,
    replayed_review_count INTEGER NOT NULL CHECK(replayed_review_count >= 0),
    created_at INTEGER NOT NULL
);

CREATE TABLE learning_recall_operations (
    operation_id TEXT PRIMARY KEY,
    program_id TEXT NOT NULL REFERENCES learning_programs(id) ON DELETE CASCADE,
    card_id TEXT,
    kind TEXT NOT NULL CHECK(kind IN ('save_card','decide_duplicate','change_scheduler','review')),
    payload_hash TEXT NOT NULL,
    result_id TEXT,
    created_at INTEGER NOT NULL,
    FOREIGN KEY(card_id) REFERENCES study_cards(id) ON DELETE CASCADE
);

CREATE TABLE learning_portability_operations (
    operation_id TEXT PRIMARY KEY,
    program_id TEXT,
    kind TEXT NOT NULL CHECK(kind IN (
        'export','preview_import','apply_import','cancel_import','delete_source','reimport_source',
        'create_selector','change_scheduler'
    )),
    payload_hash TEXT NOT NULL,
    result_id TEXT,
    created_at INTEGER NOT NULL
);
