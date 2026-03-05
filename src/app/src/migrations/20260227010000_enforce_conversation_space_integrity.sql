-- Enforce conversation ownership integrity:
-- conversations must belong to standard spaces only.
-- Journals are a separate overlay and must never own conversations.

-- Ensure legacy journal-owned conversations are represented in journal membership.
INSERT OR IGNORE INTO journal_conversation_entries (journal_space_id, conversation_id, created_at)
SELECT
    c.space_id,
    c.id,
    COALESCE(c.updated_at, CURRENT_TIMESTAMP)
FROM conversations c
INNER JOIN conversation_spaces s ON s.id = c.space_id
WHERE COALESCE(
    json_extract(s.tool_preferences_json, '$.spaceType'),
    json_extract(s.tool_preferences_json, '$.space_type'),
    'standard'
) = 'journal';

-- Repair legacy data: move conversations currently owned by journal-like spaces
-- back to a standard space.
-- 1) Try matching "<journal name without trailing ' Journal'>" to a standard space.
-- 2) Fallback to space_general if no match exists.
UPDATE conversations
SET space_id = COALESCE(
    (
        SELECT s2.id
        FROM conversation_spaces s2
        WHERE lower(trim(s2.name)) = lower(
            trim(
                CASE
                    WHEN lower(trim((
                        SELECT s1.name
                        FROM conversation_spaces s1
                        WHERE s1.id = conversations.space_id
                    ))) LIKE '% journal'
                    THEN substr(
                        trim((
                            SELECT s1.name
                            FROM conversation_spaces s1
                            WHERE s1.id = conversations.space_id
                        )),
                        1,
                        length(trim((
                            SELECT s1.name
                            FROM conversation_spaces s1
                            WHERE s1.id = conversations.space_id
                        ))) - 8
                    )
                    ELSE trim((
                        SELECT s1.name
                        FROM conversation_spaces s1
                        WHERE s1.id = conversations.space_id
                    ))
                END
            )
        )
        AND COALESCE(
            json_extract(s2.tool_preferences_json, '$.spaceType'),
            json_extract(s2.tool_preferences_json, '$.space_type'),
            'standard'
        ) <> 'journal'
        AND s2.is_archived = 0
        ORDER BY s2.sort_order ASC, s2.updated_at DESC
        LIMIT 1
    ),
    'space_general'
)
WHERE conversations.space_id IN (
    SELECT s.id
    FROM conversation_spaces s
    WHERE COALESCE(
        json_extract(s.tool_preferences_json, '$.spaceType'),
        json_extract(s.tool_preferences_json, '$.space_type'),
        'standard'
    ) = 'journal'
);

-- DB-level guards to prevent future integrity regressions.
CREATE TRIGGER IF NOT EXISTS trg_conversations_prevent_journal_space_insert
BEFORE INSERT ON conversations
WHEN EXISTS (
    SELECT 1
    FROM conversation_spaces s
    WHERE s.id = NEW.space_id
      AND COALESCE(
          json_extract(s.tool_preferences_json, '$.spaceType'),
          json_extract(s.tool_preferences_json, '$.space_type'),
          'standard'
      ) = 'journal'
)
BEGIN
    SELECT RAISE(ABORT, 'journal targets cannot own conversations');
END;

CREATE TRIGGER IF NOT EXISTS trg_conversations_prevent_journal_space_update
BEFORE UPDATE OF space_id ON conversations
WHEN EXISTS (
    SELECT 1
    FROM conversation_spaces s
    WHERE s.id = NEW.space_id
      AND COALESCE(
          json_extract(s.tool_preferences_json, '$.spaceType'),
          json_extract(s.tool_preferences_json, '$.space_type'),
          'standard'
      ) = 'journal'
)
BEGIN
    SELECT RAISE(ABORT, 'journal targets cannot own conversations');
END;
