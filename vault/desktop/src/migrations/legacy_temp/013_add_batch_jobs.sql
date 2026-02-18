-- Migration: Add batch jobs tracking tables
-- Version: 013
-- Description: Creates tables for tracking batch URL import jobs and their items

-- Batch jobs table: tracks overall batch import operations
CREATE TABLE IF NOT EXISTS batch_jobs (
    id TEXT PRIMARY KEY,
    job_type TEXT NOT NULL,
    status TEXT NOT NULL CHECK(status IN ('pending', 'running', 'completed', 'failed', 'cancelled')),
    total_items INTEGER NOT NULL,
    completed_items INTEGER NOT NULL DEFAULT 0,
    failed_items INTEGER NOT NULL DEFAULT 0,
    progress REAL NOT NULL DEFAULT 0.0,
    options TEXT,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    started_at TEXT,
    completed_at TEXT,
    error_message TEXT
) STRICT;

-- Indexes for batch jobs queries
CREATE INDEX IF NOT EXISTS idx_batch_jobs_status ON batch_jobs(status);
CREATE INDEX IF NOT EXISTS idx_batch_jobs_created_at ON batch_jobs(created_at DESC);
CREATE INDEX IF NOT EXISTS idx_batch_jobs_type_status ON batch_jobs(job_type, status);

-- Batch job items table: tracks individual items within a batch
CREATE TABLE IF NOT EXISTS batch_job_items (
    id TEXT PRIMARY KEY,
    job_id TEXT NOT NULL,
    item_url TEXT NOT NULL,
    document_id TEXT,
    status TEXT NOT NULL CHECK(status IN ('pending', 'processing', 'completed', 'failed', 'skipped')),
    error_message TEXT,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    processed_at TEXT,
    FOREIGN KEY (job_id) REFERENCES batch_jobs(id) ON DELETE CASCADE,
    FOREIGN KEY (document_id) REFERENCES documents(id) ON DELETE SET NULL
) STRICT;

-- Indexes for batch job items queries
CREATE INDEX IF NOT EXISTS idx_batch_job_items_job_id ON batch_job_items(job_id);
CREATE INDEX IF NOT EXISTS idx_batch_job_items_status ON batch_job_items(status);
CREATE INDEX IF NOT EXISTS idx_batch_job_items_job_status ON batch_job_items(job_id, status);
CREATE INDEX IF NOT EXISTS idx_batch_job_items_document_id ON batch_job_items(document_id) WHERE document_id IS NOT NULL;
