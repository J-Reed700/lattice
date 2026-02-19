-- Downloaded Models Tracking
-- Tracks models downloaded to local filesystem for chat and other features
CREATE TABLE IF NOT EXISTS downloaded_models (
    id TEXT PRIMARY KEY NOT NULL,
    model_name TEXT NOT NULL,
    model_id TEXT NOT NULL UNIQUE,
    file_path TEXT NOT NULL,
    file_size_bytes INTEGER NOT NULL,
    downloaded_at TEXT NOT NULL, -- ISO 8601 timestamp
    last_used_at TEXT, -- ISO 8601 timestamp, nullable
    use_count INTEGER NOT NULL DEFAULT 0,
    is_active_for_chat INTEGER NOT NULL DEFAULT 0 CHECK (is_active_for_chat IN (0, 1)),
    metadata TEXT -- JSON blob for model details (provider, version, capabilities, etc.)
) STRICT;

-- Index for fast lookups by model_id
CREATE INDEX IF NOT EXISTS idx_downloaded_models_model_id ON downloaded_models(model_id);

-- Index for finding active chat model quickly
CREATE INDEX IF NOT EXISTS idx_downloaded_models_active_chat ON downloaded_models(is_active_for_chat) WHERE is_active_for_chat = 1;

-- Trigger to ensure only one model can be active for chat at a time
CREATE TRIGGER IF NOT EXISTS ensure_single_active_chat_model
    BEFORE UPDATE OF is_active_for_chat ON downloaded_models
    WHEN NEW.is_active_for_chat = 1
BEGIN
    -- Set all other models to inactive
    UPDATE downloaded_models
    SET is_active_for_chat = 0
    WHERE id != NEW.id AND is_active_for_chat = 1;
END;
