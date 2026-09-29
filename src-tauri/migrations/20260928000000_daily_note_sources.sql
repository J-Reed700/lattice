ALTER TABLE daily_notes_workspace
    ADD COLUMN sources_json TEXT NOT NULL DEFAULT '[]';
