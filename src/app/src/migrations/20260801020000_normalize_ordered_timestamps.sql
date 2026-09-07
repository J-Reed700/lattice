-- Normalize timestamp columns that participate in text ordering. SQLite's
-- datetime parser accepts both its legacy `YYYY-MM-DD HH:MM:SS` form and
-- RFC3339 values with offsets; strftime converts both to UTC RFC3339.

UPDATE documents
SET modified_at = COALESCE(strftime('%Y-%m-%dT%H:%M:%fZ', modified_at), modified_at),
    indexed_at = COALESCE(strftime('%Y-%m-%dT%H:%M:%fZ', indexed_at), indexed_at),
    created_at = COALESCE(strftime('%Y-%m-%dT%H:%M:%fZ', created_at), created_at),
    updated_at = COALESCE(strftime('%Y-%m-%dT%H:%M:%fZ', updated_at), updated_at),
    published_at = CASE
        WHEN published_at IS NULL THEN NULL
        ELSE COALESCE(strftime('%Y-%m-%dT%H:%M:%fZ', published_at), published_at)
    END;

UPDATE conversations
SET created_at = COALESCE(strftime('%Y-%m-%dT%H:%M:%fZ', created_at), created_at),
    updated_at = COALESCE(strftime('%Y-%m-%dT%H:%M:%fZ', updated_at), updated_at),
    saved_at = CASE WHEN saved_at IS NULL THEN NULL ELSE COALESCE(strftime('%Y-%m-%dT%H:%M:%fZ', saved_at), saved_at) END,
    bookmarked_at = CASE WHEN bookmarked_at IS NULL THEN NULL ELSE COALESCE(strftime('%Y-%m-%dT%H:%M:%fZ', bookmarked_at), bookmarked_at) END,
    pinned_at = CASE WHEN pinned_at IS NULL THEN NULL ELSE COALESCE(strftime('%Y-%m-%dT%H:%M:%fZ', pinned_at), pinned_at) END,
    archived_at = CASE WHEN archived_at IS NULL THEN NULL ELSE COALESCE(strftime('%Y-%m-%dT%H:%M:%fZ', archived_at), archived_at) END;

UPDATE conversation_messages
SET created_at = COALESCE(strftime('%Y-%m-%dT%H:%M:%fZ', created_at), created_at);

UPDATE conversation_spaces
SET created_at = COALESCE(strftime('%Y-%m-%dT%H:%M:%fZ', created_at), created_at),
    updated_at = COALESCE(strftime('%Y-%m-%dT%H:%M:%fZ', updated_at), updated_at);

UPDATE journals
SET created_at = COALESCE(strftime('%Y-%m-%dT%H:%M:%fZ', created_at), created_at),
    updated_at = COALESCE(strftime('%Y-%m-%dT%H:%M:%fZ', updated_at), updated_at);

UPDATE journal_conversation_entries
SET created_at = COALESCE(strftime('%Y-%m-%dT%H:%M:%fZ', created_at), created_at);
