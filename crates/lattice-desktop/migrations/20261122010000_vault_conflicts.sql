-- Unread vault conflict notices recorded atomically alongside the conflict resolution,
-- so conflicts resolved during startup or before listeners mount are surfaced until dismissed.
CREATE TABLE IF NOT EXISTS vault_conflicts (
    id TEXT PRIMARY KEY NOT NULL,
    kind TEXT NOT NULL,
    note_id TEXT NOT NULL,
    note_title TEXT NOT NULL,
    copy_id TEXT,
    copy_title TEXT,
    set_aside_path TEXT,
    created_at TEXT NOT NULL
);
