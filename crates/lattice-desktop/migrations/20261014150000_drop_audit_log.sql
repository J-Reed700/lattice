-- Drops the local audit log. Its sink created these tables itself at startup
-- (no migration ever did), recorded only redacted action names and outcomes,
-- and nothing read them back. The audit module is gone, so nothing writes or
-- prunes them now either. Dropping `audit_events` also drops its two
-- row-count triggers.
DROP TABLE IF EXISTS audit_events;
DROP TABLE IF EXISTS audit_retention_state;
DROP TABLE IF EXISTS audit_schema_metadata;
