-- Course-outline evidence rebuild (docs/design/2026-10-14-outline-evidence-rebuild.md).
-- Draft programs carry the old mandatory-citation findings and quote issues in
-- their state_json. Pre-release: delete them rather than translate them. Every
-- draft table, source table and the source library cascade from
-- learning_programs, and the production pool enables foreign keys.
DELETE FROM learning_programs WHERE status = 'draft';
