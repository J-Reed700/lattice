-- Preserve the diagnostic attached to a failed document across repository
-- reads and aggregate saves.
ALTER TABLE documents ADD COLUMN error_message TEXT;
