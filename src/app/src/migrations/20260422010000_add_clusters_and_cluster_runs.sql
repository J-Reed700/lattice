-- Phase 5.3: corpus-shape clustering overlay.
-- Read-only overlay: we never touch documents/embeddings/text_chunks.
-- Clusters are regenerated on demand; dropping a run cascades its clusters
-- and members but never touches source documents.

CREATE TABLE IF NOT EXISTS cluster_runs (
    id TEXT PRIMARY KEY,
    ran_at TEXT NOT NULL,
    doc_count INTEGER NOT NULL,
    cluster_count INTEGER NOT NULL,
    noise_count INTEGER NOT NULL,
    params_hash TEXT NOT NULL,
    duration_ms INTEGER NOT NULL,
    llm_calls INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_cluster_runs_ran_at ON cluster_runs(ran_at DESC);

CREATE TABLE IF NOT EXISTS clusters (
    id TEXT PRIMARY KEY,
    run_id TEXT NOT NULL REFERENCES cluster_runs(id) ON DELETE CASCADE,
    label TEXT NOT NULL,
    description TEXT,
    member_count INTEGER NOT NULL,
    centroid BLOB NOT NULL,
    fingerprint TEXT NOT NULL,
    label_source TEXT NOT NULL,
    inherited_from_cluster_id TEXT,
    created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_clusters_run_id ON clusters(run_id);
CREATE INDEX IF NOT EXISTS idx_clusters_fingerprint ON clusters(fingerprint);

CREATE TABLE IF NOT EXISTS cluster_members (
    cluster_id TEXT NOT NULL REFERENCES clusters(id) ON DELETE CASCADE,
    document_id TEXT NOT NULL,
    membership_probability REAL NOT NULL,
    is_representative INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (cluster_id, document_id)
);

CREATE INDEX IF NOT EXISTS idx_cluster_members_doc ON cluster_members(document_id);
