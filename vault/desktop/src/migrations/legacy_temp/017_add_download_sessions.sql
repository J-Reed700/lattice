-- Migration: Add download_sessions table
-- Description: Track model download sessions with state, progress, and checksums

CREATE TABLE IF NOT EXISTS download_sessions (
    id TEXT PRIMARY KEY NOT NULL,
    url TEXT NOT NULL,
    destination TEXT NOT NULL,
    state TEXT NOT NULL CHECK(state IN ('pending', 'downloading', 'paused', 'completed', 'failed', 'cancelled')),

    -- Progress tracking
    bytes_downloaded INTEGER NOT NULL DEFAULT 0,
    total_bytes INTEGER,
    bytes_per_second REAL NOT NULL DEFAULT 0.0,

    -- Checksum verification
    checksum_algorithm TEXT CHECK(checksum_algorithm IN ('sha256', 'md5')),
    checksum_value TEXT,

    -- Error handling
    error_message TEXT,
    retry_count INTEGER NOT NULL DEFAULT 0,
    max_retries INTEGER NOT NULL DEFAULT 3,

    -- Timestamps
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    started_at TEXT,
    completed_at TEXT
);

CREATE INDEX IF NOT EXISTS idx_download_sessions_state ON download_sessions(state);
CREATE INDEX IF NOT EXISTS idx_download_sessions_created_at ON download_sessions(created_at);
CREATE INDEX IF NOT EXISTS idx_download_sessions_url ON download_sessions(url);
