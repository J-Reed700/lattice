-- A folder's own settings in the Explorer.
--
-- `instructions` is the folder's system prompt. It stands in for the space's
-- prompt in the folder's threads, and a thread's own prompt still wins over
-- it. NULL or blank means none, and the space's prompt applies as before.
--
-- `space_id` is the space the folder's threads belong to, which decides the
-- library they search and the space memory they read. NULL is General. A
-- deleted space sends its folders back to General.
ALTER TABLE explorer_folders ADD COLUMN instructions TEXT;
ALTER TABLE explorer_folders ADD COLUMN space_id TEXT
    REFERENCES conversation_spaces(id) ON DELETE SET NULL;
