-- Split journals from conversation spaces at the persistence layer.
-- After this migration:
-- - conversation_spaces stores standard spaces only.
-- - journals stores journal notebooks only.
-- - journal_conversation_entries references journals(id), not conversation_spaces(id).

CREATE TABLE IF NOT EXISTS journals (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    description TEXT,
    icon TEXT,
    accent_color TEXT,
    space_prompt TEXT,
    default_model_name TEXT,
    tool_preferences_json TEXT,
    is_archived INTEGER NOT NULL DEFAULT 0 CHECK (is_archived IN (0, 1)),
    sort_order INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_journals_sort
    ON journals(sort_order, updated_at DESC);

CREATE INDEX IF NOT EXISTS idx_journals_archived
    ON journals(is_archived, updated_at DESC);

-- Move legacy journal rows out of conversation_spaces into journals.
INSERT OR IGNORE INTO journals (
    id,
    name,
    description,
    icon,
    accent_color,
    space_prompt,
    default_model_name,
    tool_preferences_json,
    is_archived,
    sort_order,
    created_at,
    updated_at
)
SELECT
    s.id,
    s.name,
    s.description,
    s.icon,
    s.accent_color,
    s.space_prompt,
    s.default_model_name,
    s.tool_preferences_json,
    s.is_archived,
    s.sort_order,
    s.created_at,
    s.updated_at
FROM conversation_spaces s
WHERE COALESCE(
    json_extract(s.tool_preferences_json, '$.spaceType'),
    json_extract(s.tool_preferences_json, '$.space_type'),
    'standard'
) = 'journal';

-- Ensure legacy conversations that still point at journal ids are recorded
-- as journal memberships before repairing ownership.
INSERT OR IGNORE INTO journal_conversation_entries (journal_space_id, conversation_id, created_at)
SELECT
    c.space_id,
    c.id,
    COALESCE(c.updated_at, CURRENT_TIMESTAMP)
FROM conversations c
INNER JOIN journals j ON j.id = c.space_id;

-- conversations must always belong to a standard space.
UPDATE conversations
SET space_id = 'space_general'
WHERE space_id IN (SELECT id FROM journals);

-- Rebuild journal_conversation_entries so FK targets journals(id).
DROP INDEX IF EXISTS idx_journal_conversation_entries_space_created;
DROP INDEX IF EXISTS idx_journal_conversation_entries_conversation;

CREATE TABLE IF NOT EXISTS journal_conversation_entries_new (
    journal_space_id TEXT NOT NULL,
    conversation_id TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (journal_space_id, conversation_id),
    FOREIGN KEY (journal_space_id) REFERENCES journals(id) ON DELETE CASCADE,
    FOREIGN KEY (conversation_id) REFERENCES conversations(id) ON DELETE CASCADE
);

INSERT OR IGNORE INTO journal_conversation_entries_new (journal_space_id, conversation_id, created_at)
SELECT
    jce.journal_space_id,
    jce.conversation_id,
    jce.created_at
FROM journal_conversation_entries jce
INNER JOIN journals j ON j.id = jce.journal_space_id;

DROP TABLE journal_conversation_entries;
ALTER TABLE journal_conversation_entries_new RENAME TO journal_conversation_entries;

CREATE INDEX IF NOT EXISTS idx_journal_conversation_entries_space_created
    ON journal_conversation_entries (journal_space_id, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_journal_conversation_entries_conversation
    ON journal_conversation_entries (conversation_id, journal_space_id);

-- Remove journal rows from spaces now that journals are first-class.
DELETE FROM conversation_spaces
WHERE id IN (SELECT id FROM journals);

-- Replace old journal-specific ownership triggers with strict space existence checks.
DROP TRIGGER IF EXISTS trg_conversations_prevent_journal_space_insert;
DROP TRIGGER IF EXISTS trg_conversations_prevent_journal_space_update;

CREATE TRIGGER IF NOT EXISTS trg_conversations_require_valid_space_insert
BEFORE INSERT ON conversations
WHEN NOT EXISTS (
    SELECT 1
    FROM conversation_spaces s
    WHERE s.id = NEW.space_id
)
BEGIN
    SELECT RAISE(ABORT, 'conversation must belong to an existing space');
END;

CREATE TRIGGER IF NOT EXISTS trg_conversations_require_valid_space_update
BEFORE UPDATE OF space_id ON conversations
WHEN NOT EXISTS (
    SELECT 1
    FROM conversation_spaces s
    WHERE s.id = NEW.space_id
)
BEGIN
    SELECT RAISE(ABORT, 'conversation must belong to an existing space');
END;
