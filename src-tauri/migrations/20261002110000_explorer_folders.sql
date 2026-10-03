-- The folders picked in the Explorer ("Your folders" on its start screen).
-- `root` is the canonical absolute path, as `conversations.explorer_root`
-- holds it. A row is added or touched whenever the folder's index is opened
-- and removed only when the user removes the folder. The index itself lives
-- outside this database, under <data_dir>/folder-index/.
CREATE TABLE IF NOT EXISTS explorer_folders (
    root TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    pinned INTEGER NOT NULL DEFAULT 0 CHECK (pinned IN (0, 1)),
    added_at TEXT NOT NULL,
    last_opened_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_explorer_folders_order
    ON explorer_folders(pinned DESC, last_opened_at DESC);
