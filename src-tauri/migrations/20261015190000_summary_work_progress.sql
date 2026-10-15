-- Model calls a summary run has finished, kept until the document's summaries
-- are published. A run does a bounded amount of model work and then yields to
-- other documents; the next run resumes from here instead of starting over.
-- Rows of an older revision of the document are ignored and replaced.
CREATE TABLE summary_work_progress (
    document_id TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    call_id TEXT NOT NULL,
    revision TEXT NOT NULL,
    summary_text TEXT NOT NULL,
    PRIMARY KEY (document_id, call_id)
);
