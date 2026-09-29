-- Spill focus-rescan membership to SQLite so reconciliation stays bounded in
-- process memory even for very large vaults. run_id isolates concurrent scans.
CREATE TABLE IF NOT EXISTS vault_rescan_seen (
    run_id TEXT NOT NULL,
    note_id TEXT NOT NULL,
    PRIMARY KEY (run_id, note_id)
);

CREATE INDEX IF NOT EXISTS idx_vault_rescan_seen_run
    ON vault_rescan_seen(run_id);
