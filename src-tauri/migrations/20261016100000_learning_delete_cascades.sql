-- Deleting a course removes everything under it. Assessment rows that point at
-- outcomes, blueprints, candidates and form items, and the self-references in
-- curriculum revisions and practice sessions, used RESTRICT / NO ACTION, so a
-- course delete failed once assessment or revision data existed.
--
-- Only ON DELETE actions change, which SQLite keeps in the schema text and not
-- in table pages. sqlx-sqlite 0.8.6 runs every migration inside a transaction
-- (it ignores `-- no-transaction`), where PRAGMA foreign_keys=OFF is a no-op, so
-- the 12-step copy/drop/rename rebuild would cascade-delete child rows on DROP
-- or fail on RESTRICT. This uses SQLite's documented in-place procedure for
-- constraint-only changes instead (lang_altertable.html#otheralter): edit the
-- stored CREATE TABLE text, reload this connection's schema, and bump the
-- schema cookie so pooled connections reload too. No row is copied or moved.

PRAGMA writable_schema = ON;

UPDATE sqlite_schema
SET sql = replace(sql, 'ON DELETE RESTRICT', 'ON DELETE CASCADE')
WHERE type = 'table' AND name IN (
    'learning_assessment_blueprint_slots',
    'learning_assessment_candidates',
    'learning_assessment_candidate_outcomes',
    'learning_assessment_forms',
    'learning_assessment_form_items',
    'learning_assessment_form_item_outcomes',
    'learning_assessment_item_results'
);

UPDATE sqlite_schema
SET sql = replace(sql, 'ON DELETE RESTRICT', 'ON DELETE SET NULL')
WHERE type = 'table' AND name = 'learning_curriculum_revisions';

UPDATE sqlite_schema
SET sql = replace(
    sql,
    'revises_session_id TEXT REFERENCES learning_practice_sessions(id)',
    'revises_session_id TEXT REFERENCES learning_practice_sessions(id) ON DELETE SET NULL'
)
WHERE type = 'table' AND name = 'learning_practice_sessions'
  AND sql NOT LIKE '%revises_session_id TEXT REFERENCES learning_practice_sessions(id) ON DELETE%';

-- RESET turns writable_schema off and reparses the edited schema; a syntax
-- error fails the migration here and the transaction rolls the edit back.
PRAGMA writable_schema = RESET;

-- Fail (and roll back) unless every intended action is now in force. The
-- CREATE/DROP of a main-schema table also bumps the schema cookie.
CREATE TABLE _lattice_delete_cascades_guard (ok INTEGER NOT NULL CHECK (ok = 1));
INSERT INTO _lattice_delete_cascades_guard (ok)
SELECT
    (SELECT count(*) FROM pragma_foreign_key_list('learning_assessment_blueprint_slots') WHERE on_delete = 'RESTRICT')
  + (SELECT count(*) FROM pragma_foreign_key_list('learning_assessment_candidates') WHERE on_delete = 'RESTRICT')
  + (SELECT count(*) FROM pragma_foreign_key_list('learning_assessment_candidate_outcomes') WHERE on_delete = 'RESTRICT')
  + (SELECT count(*) FROM pragma_foreign_key_list('learning_assessment_forms') WHERE on_delete = 'RESTRICT')
  + (SELECT count(*) FROM pragma_foreign_key_list('learning_assessment_form_items') WHERE on_delete = 'RESTRICT')
  + (SELECT count(*) FROM pragma_foreign_key_list('learning_assessment_form_item_outcomes') WHERE on_delete = 'RESTRICT')
  + (SELECT count(*) FROM pragma_foreign_key_list('learning_assessment_item_results') WHERE on_delete = 'RESTRICT')
  + (SELECT count(*) FROM pragma_foreign_key_list('learning_curriculum_revisions') WHERE on_delete = 'RESTRICT')
  = 0
  AND (SELECT count(*) FROM pragma_foreign_key_list('learning_assessment_form_items')
       WHERE "table" IN ('learning_assessment_candidates', 'learning_outcome_definitions')
         AND on_delete = 'CASCADE') = 2
  AND (SELECT on_delete FROM pragma_foreign_key_list('learning_curriculum_revisions')
       WHERE "from" = 'predecessor_id') = 'SET NULL'
  AND (SELECT on_delete FROM pragma_foreign_key_list('learning_practice_sessions')
       WHERE "from" = 'revises_session_id') = 'SET NULL';
DROP TABLE _lattice_delete_cascades_guard;
