ALTER TABLE daily_notes_workspace ADD COLUMN revision INTEGER NOT NULL DEFAULT 0;

CREATE TRIGGER workspace_note_revision
AFTER UPDATE ON daily_notes_workspace
WHEN NEW.revision = OLD.revision
BEGIN
    UPDATE daily_notes_workspace SET revision = OLD.revision + 1 WHERE id = NEW.id;
END;
