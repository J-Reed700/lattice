-- An entry pinned in a journal stays at the top of that journal's list.
-- The pin belongs to the entry, not the conversation: the same chat can be
-- pinned in one journal and not in another, and a journal pin is not the
-- Chat sidebar's pin. NULL is unpinned.
ALTER TABLE journal_conversation_entries ADD COLUMN pinned_at TEXT;
