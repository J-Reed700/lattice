-- The event bridge revisits terminal sessions until their durable model-file
-- state agrees. Restrict candidate scans to terminal rows before joining each
-- model; without this partial index SQLite walks every session for a model.
CREATE INDEX IF NOT EXISTS idx_download_sessions_terminal_reconciliation
    ON download_sessions(model_id, state, model_file_name)
    WHERE state IN ('completed', 'failed', 'cancelled')
      AND model_id IS NOT NULL
      AND model_file_name IS NOT NULL;

-- Both completion branches check whether any file for the model is still
-- incomplete. Keep the status in the index so those correlated checks do not
-- fetch each model_files row just to reject it.
CREATE INDEX IF NOT EXISTS idx_model_files_model_status
    ON model_files(model_id, status);
