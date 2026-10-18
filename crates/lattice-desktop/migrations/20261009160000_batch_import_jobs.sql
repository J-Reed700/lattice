-- A batch import is a job: the job runtime owns its status, progress,
-- cancellation, retry and recovery. These rows are the import's own facts,
-- one per file or URL, keyed by the job. A retry moves them to the new
-- attempt. Batch history saved under the old tables is not carried over.
DROP TABLE batch_job_items;
DROP TABLE batch_jobs;

CREATE TABLE batch_import_items (
    id TEXT PRIMARY KEY,
    job_id TEXT NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
    position INTEGER NOT NULL CHECK(position >= 0),
    target TEXT NOT NULL CHECK(length(target) > 0),
    document_id TEXT REFERENCES documents(id) ON DELETE SET NULL,
    status TEXT NOT NULL CHECK(status IN ('pending','processing','completed','failed','cancelled')),
    error_message TEXT,
    processed_at INTEGER,
    UNIQUE(job_id, position)
) STRICT;
CREATE INDEX batch_import_items_job_status ON batch_import_items(job_id, status, position);
CREATE INDEX batch_import_items_document ON batch_import_items(document_id) WHERE document_id IS NOT NULL;
