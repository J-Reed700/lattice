-- Migration: Add model metadata to download_sessions
-- Description: Add model_name and model_id columns to track which model is being downloaded

-- Add model metadata columns
ALTER TABLE download_sessions ADD COLUMN model_name TEXT;
ALTER TABLE download_sessions ADD COLUMN model_id TEXT;

-- Create index for fast lookups by model_id
CREATE INDEX IF NOT EXISTS idx_download_sessions_model_id ON download_sessions(model_id);
