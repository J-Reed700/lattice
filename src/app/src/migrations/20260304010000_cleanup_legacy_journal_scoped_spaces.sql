-- Cleanup pass for legacy "journal scoped chat" rows that were persisted as spaces.
-- These should be journals, not conversation spaces.

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
WHERE
    COALESCE(
        json_extract(s.tool_preferences_json, '$.spaceType'),
        json_extract(s.tool_preferences_json, '$.space_type'),
        'standard'
    ) = 'journal'
    OR lower(trim(s.name)) LIKE 'journal sources %'
    OR lower(trim(COALESCE(s.description, ''))) LIKE 'scoped source chat for journal %';

INSERT OR IGNORE INTO journal_conversation_entries (journal_space_id, conversation_id, created_at)
SELECT
    c.space_id,
    c.id,
    COALESCE(c.updated_at, CURRENT_TIMESTAMP)
FROM conversations c
INNER JOIN journals j ON j.id = c.space_id;

UPDATE conversations
SET space_id = 'space_general'
WHERE space_id IN (
    SELECT s.id
    FROM conversation_spaces s
    WHERE
        COALESCE(
            json_extract(s.tool_preferences_json, '$.spaceType'),
            json_extract(s.tool_preferences_json, '$.space_type'),
            'standard'
        ) = 'journal'
        OR lower(trim(s.name)) LIKE 'journal sources %'
        OR lower(trim(COALESCE(s.description, ''))) LIKE 'scoped source chat for journal %'
);

DELETE FROM conversation_spaces
WHERE id IN (
    SELECT s.id
    FROM conversation_spaces s
    WHERE
        COALESCE(
            json_extract(s.tool_preferences_json, '$.spaceType'),
            json_extract(s.tool_preferences_json, '$.space_type'),
            'standard'
        ) = 'journal'
        OR lower(trim(s.name)) LIKE 'journal sources %'
        OR lower(trim(COALESCE(s.description, ''))) LIKE 'scoped source chat for journal %'
);
