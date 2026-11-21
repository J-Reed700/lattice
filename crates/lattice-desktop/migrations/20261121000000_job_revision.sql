ALTER TABLE jobs ADD COLUMN revision INTEGER NOT NULL DEFAULT 0 CHECK(revision >= 0);

CREATE TRIGGER jobs_revision AFTER UPDATE ON jobs
WHEN NEW.revision = OLD.revision
BEGIN
    UPDATE jobs SET revision = OLD.revision + 1 WHERE id = NEW.id;
END;
