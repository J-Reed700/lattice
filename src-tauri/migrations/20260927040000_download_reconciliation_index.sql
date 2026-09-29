-- Supports the bounded terminal-session/model-file reconciliation scan.
CREATE INDEX IF NOT EXISTS idx_download_sessions_reconciliation
    ON download_sessions(model_id, model_file_name, created_at, id);
