-- Conversation Spotlight search index:
-- indexes conversation titles, messages, and bookmark metadata.

CREATE VIRTUAL TABLE IF NOT EXISTS conversation_search_fts USING fts5(
    conversation_id UNINDEXED,
    message_id UNINDEXED,
    source UNINDEXED,
    content,
    tokenize = 'unicode61 remove_diacritics 2'
);

-- Backfill FTS rows.
INSERT INTO conversation_search_fts (conversation_id, message_id, source, content)
SELECT id, NULL, 'title', COALESCE(title, '')
FROM conversations;

INSERT INTO conversation_search_fts (conversation_id, message_id, source, content)
SELECT conversation_id, id, 'message', COALESCE(content, '')
FROM conversation_messages;

INSERT INTO conversation_search_fts (conversation_id, message_id, source, content)
SELECT
    b.conversation_id,
    b.message_id,
    'bookmark',
    TRIM(COALESCE(b.title, '') || ' ' || COALESCE(b.note, ''))
FROM conversation_message_bookmarks b;

-- Conversation title sync.
CREATE TRIGGER IF NOT EXISTS trg_conversation_search_fts_conversation_ai
AFTER INSERT ON conversations
BEGIN
    INSERT INTO conversation_search_fts (conversation_id, message_id, source, content)
    VALUES (new.id, NULL, 'title', COALESCE(new.title, ''));
END;

CREATE TRIGGER IF NOT EXISTS trg_conversation_search_fts_conversation_au
AFTER UPDATE OF title ON conversations
BEGIN
    DELETE FROM conversation_search_fts
    WHERE conversation_id = old.id AND source = 'title';
    INSERT INTO conversation_search_fts (conversation_id, message_id, source, content)
    VALUES (new.id, NULL, 'title', COALESCE(new.title, ''));
END;

CREATE TRIGGER IF NOT EXISTS trg_conversation_search_fts_conversation_ad
AFTER DELETE ON conversations
BEGIN
    DELETE FROM conversation_search_fts
    WHERE conversation_id = old.id;
END;

-- Conversation message sync.
CREATE TRIGGER IF NOT EXISTS trg_conversation_search_fts_message_ai
AFTER INSERT ON conversation_messages
BEGIN
    INSERT INTO conversation_search_fts (conversation_id, message_id, source, content)
    VALUES (new.conversation_id, new.id, 'message', COALESCE(new.content, ''));
END;

CREATE TRIGGER IF NOT EXISTS trg_conversation_search_fts_message_au
AFTER UPDATE OF content ON conversation_messages
BEGIN
    DELETE FROM conversation_search_fts
    WHERE conversation_id = old.conversation_id AND message_id = old.id AND source = 'message';
    INSERT INTO conversation_search_fts (conversation_id, message_id, source, content)
    VALUES (new.conversation_id, new.id, 'message', COALESCE(new.content, ''));
END;

CREATE TRIGGER IF NOT EXISTS trg_conversation_search_fts_message_ad
AFTER DELETE ON conversation_messages
BEGIN
    DELETE FROM conversation_search_fts
    WHERE conversation_id = old.conversation_id AND message_id = old.id;
END;

-- Bookmark sync.
CREATE TRIGGER IF NOT EXISTS trg_conversation_search_fts_bookmark_ai
AFTER INSERT ON conversation_message_bookmarks
BEGIN
    INSERT INTO conversation_search_fts (conversation_id, message_id, source, content)
    VALUES (
        new.conversation_id,
        new.message_id,
        'bookmark',
        TRIM(COALESCE(new.title, '') || ' ' || COALESCE(new.note, ''))
    );
END;

CREATE TRIGGER IF NOT EXISTS trg_conversation_search_fts_bookmark_au
AFTER UPDATE OF title, note ON conversation_message_bookmarks
BEGIN
    DELETE FROM conversation_search_fts
    WHERE conversation_id = old.conversation_id AND message_id = old.message_id AND source = 'bookmark';
    INSERT INTO conversation_search_fts (conversation_id, message_id, source, content)
    VALUES (
        new.conversation_id,
        new.message_id,
        'bookmark',
        TRIM(COALESCE(new.title, '') || ' ' || COALESCE(new.note, ''))
    );
END;

CREATE TRIGGER IF NOT EXISTS trg_conversation_search_fts_bookmark_ad
AFTER DELETE ON conversation_message_bookmarks
BEGIN
    DELETE FROM conversation_search_fts
    WHERE conversation_id = old.conversation_id AND message_id = old.message_id AND source = 'bookmark';
END;
