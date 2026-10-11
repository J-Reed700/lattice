-- The thread a folder's chat column last showed, so reopening the folder
-- lands on it. Kept with the folder's row rather than in the renderer so a
-- backup or restore carries it. Deleting the thread clears it, and the
-- Explorer then opens the folder's most recent thread instead.
ALTER TABLE explorer_folders ADD COLUMN last_thread_id TEXT
    REFERENCES conversations(id) ON DELETE SET NULL;
