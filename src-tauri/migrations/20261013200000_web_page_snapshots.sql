-- Frozen web snapshots: one self-contained, inert HTML copy of a cited page
-- per (conversation, normalized URL). The file lives at
-- ~/.lattice/web-snapshots/<id>.html; sha256 and byte_size are written in the
-- same transaction that flips status to 'ready'.
-- Design: docs/design/2026-10-13-frozen-web-snapshots.md §7.
CREATE TABLE web_page_snapshots (
    id TEXT PRIMARY KEY,
    conversation_id TEXT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
    normalized_url TEXT NOT NULL,
    requested_url TEXT NOT NULL,
    final_url TEXT,
    title TEXT,
    status TEXT NOT NULL CHECK (status IN ('pending','ready','failed')),
    method TEXT CHECK (method IS NULL OR method IN ('http','browser')),
    captured_at TEXT,
    byte_size INTEGER,
    sha256 TEXT,
    resource_count INTEGER NOT NULL DEFAULT 0,
    missing_resource_count INTEGER NOT NULL DEFAULT 0,
    partial INTEGER NOT NULL DEFAULT 0,
    partial_reasons TEXT,
    error_code TEXT,
    error_message TEXT,
    favicon_data_url TEXT,
    job_id TEXT,
    created_at TEXT NOT NULL,
    UNIQUE (conversation_id, normalized_url)
);
CREATE INDEX idx_web_page_snapshots_conversation ON web_page_snapshots(conversation_id);
