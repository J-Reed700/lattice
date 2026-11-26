CREATE TABLE content_edits (
    id TEXT PRIMARY KEY,
    kind TEXT NOT NULL,
    content_key TEXT NOT NULL,
    scope_feature TEXT NOT NULL,
    scope_container TEXT NOT NULL,
    seq INTEGER NOT NULL,
    origin_json TEXT NOT NULL,
    target_json TEXT,
    ops_json TEXT NOT NULL,
    inverse_json TEXT NOT NULL,
    before_revision TEXT NOT NULL,
    after_revision TEXT NOT NULL,
    reverts_edit_id TEXT REFERENCES content_edits(id) ON DELETE SET NULL,
    request_sha256 TEXT NOT NULL,
    outcome_json TEXT,
    app_version TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    UNIQUE(kind, content_key, seq)
) STRICT;
CREATE INDEX content_edits_by_scope
    ON content_edits(scope_feature, scope_container, created_at);

CREATE TABLE content_edit_receipts (
    edit_id TEXT NOT NULL REFERENCES content_edits(id) ON DELETE CASCADE,
    reactor TEXT NOT NULL,
    receipt_json TEXT NOT NULL,
    PRIMARY KEY(edit_id, reactor)
) STRICT;

CREATE TRIGGER content_edits_leave_with_their_course
AFTER DELETE ON learning_programs
BEGIN
    DELETE FROM content_edits WHERE scope_feature='learning' AND scope_container=OLD.id;
END;
CREATE TRIGGER content_edits_leave_with_their_deck
AFTER DELETE ON study_decks
BEGIN
    DELETE FROM content_edits WHERE scope_feature='study' AND scope_container=OLD.id;
END;
