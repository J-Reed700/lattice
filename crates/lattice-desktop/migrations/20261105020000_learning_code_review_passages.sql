-- Spot-the-errors drills (docs/design/2026-11-05-spot-the-errors-drills.md, section 6): a drill is code, or an
-- explanation of a passage of one of a course's sources.
--
-- `material` says which. A drill on a passage has no language, and keeps in `source_json` the passage its
-- explanation was written from: the source version, the byte range of the passage in it, its title and its text.
--
-- `language` loses its NOT NULL by SQLite's documented procedure for constraint-only changes
-- (lang_altertable.html#otheralter), as 20261016100000 does: sqlx-sqlite runs every migration inside a transaction
-- where PRAGMA foreign_keys=OFF is a no-op, so a copy/drop/rename rebuild would cascade-delete every drill's pattern
-- rows and attempts on DROP. Removing a NOT NULL never invalidates a stored row, so no row is copied or moved. The
-- columns are added after it, which reloads the edited schema and bumps the schema cookie so pooled connections
-- reload it too; their CHECKs are tested against every stored row as they are added.

PRAGMA writable_schema = ON;

UPDATE sqlite_schema
SET sql = replace(sql, 'language TEXT NOT NULL,', 'language TEXT,')
WHERE type = 'table' AND name = 'learning_drills';

-- RESET turns writable_schema off and reparses the edited schema; a syntax
-- error fails the migration here and the transaction rolls the edit back.
PRAGMA writable_schema = RESET;

ALTER TABLE learning_drills ADD COLUMN material TEXT NOT NULL DEFAULT 'code'
    CHECK(material IN ('code','passage'))
    CHECK((material = 'code') = (language IS NOT NULL));

ALTER TABLE learning_drills ADD COLUMN source_json TEXT
    CHECK(source_json IS NULL OR json_valid(source_json))
    CHECK((material = 'passage') = (source_json IS NOT NULL));

-- The oldest ready drill on a passage of a course, and the passages a course's drills were built on.
CREATE INDEX learning_drills_course_bank ON learning_drills(program_id, status, created_at)
    WHERE material = 'passage';

-- Fail (and roll back) unless a drill's language may now be null.
CREATE TABLE _lattice_drill_passages_guard (ok INTEGER NOT NULL CHECK (ok = 1));
INSERT INTO _lattice_drill_passages_guard (ok)
SELECT "notnull" = 0 FROM pragma_table_info('learning_drills') WHERE name = 'language';
DROP TABLE _lattice_drill_passages_guard;
