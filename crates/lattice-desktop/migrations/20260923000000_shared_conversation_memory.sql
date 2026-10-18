-- Additive: preserve existing libraries and source-backed ledger identities.
CREATE TABLE conversation_memory_attributes (
    item_id TEXT PRIMARY KEY REFERENCES conversation_memory_items(id) ON DELETE CASCADE,
    scope TEXT NOT NULL DEFAULT 'conversation' CHECK(scope IN ('conversation','space','personal')),
    space_id TEXT REFERENCES conversation_spaces(id) ON DELETE CASCADE,
    valid_from TEXT,
    valid_until TEXT,
    verified_at TEXT,
    forgotten INTEGER NOT NULL DEFAULT 0 CHECK(forgotten IN (0,1)),
    CHECK ((scope='space' AND space_id IS NOT NULL) OR (scope!='space' AND space_id IS NULL)),
    CHECK (valid_from IS NULL OR valid_until IS NULL OR valid_from < valid_until)
);
CREATE INDEX idx_memory_attributes_scope ON conversation_memory_attributes(scope, space_id, forgotten);
-- Sharing is deliberate; an extracted correction inherits its predecessor's
-- scope, but does not silently acquire a new validity interval or verification.
CREATE TRIGGER trg_memory_scope_superseded AFTER UPDATE OF superseded_by ON conversation_memory_items
WHEN NEW.superseded_by IS NOT NULL
BEGIN
    INSERT OR IGNORE INTO conversation_memory_attributes(item_id,scope,space_id,forgotten)
    SELECT NEW.superseded_by,scope,space_id,forgotten FROM conversation_memory_attributes WHERE item_id=NEW.id;
END;

-- A rebuild may replace derived item IDs, but must not undo a user's decision
-- to stop saving the same source assertion. These contain no copied text.
CREATE TABLE conversation_memory_suppressions (
    message_id TEXT NOT NULL REFERENCES conversation_messages(id) ON DELETE CASCADE,
    start_byte INTEGER NOT NULL,
    end_byte INTEGER NOT NULL,
    content_digest TEXT NOT NULL,
    PRIMARY KEY(message_id,start_byte,end_byte,content_digest)
);
CREATE TRIGGER trg_memory_suppression_evidence AFTER INSERT ON conversation_memory_evidence
WHEN NEW.purpose='assertion' AND EXISTS (
    SELECT 1 FROM conversation_memory_suppressions s WHERE s.message_id=NEW.message_id
      AND s.content_digest=NEW.content_digest AND s.start_byte<NEW.end_byte AND s.end_byte>NEW.start_byte
)
BEGIN
    INSERT INTO conversation_memory_attributes(item_id,forgotten) VALUES (NEW.item_id,1)
    ON CONFLICT(item_id) DO UPDATE SET forgotten=1,scope='conversation',space_id=NULL;
    UPDATE conversation_memory_items SET state='resolved' WHERE id=NEW.item_id;
END;
