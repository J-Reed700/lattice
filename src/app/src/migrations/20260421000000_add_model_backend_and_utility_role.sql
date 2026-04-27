-- Unify local + Ollama models into a single registry and add a third role slot.
-- Rationale: provider/backend selection is a property of the model, not a
-- global routing switch. Role slots (chat/utility/embedding) are orthogonal
-- to which backend serves them.

-- Column 1: backend. 'local' = GGUF/safetensors on disk, 'ollama' = remote-hosted.
-- Default 'local' so existing rows (all local downloads) migrate cleanly.
ALTER TABLE models
    ADD COLUMN backend TEXT NOT NULL DEFAULT 'local'
    CHECK (backend IN ('local', 'ollama'));

-- Column 2: utility-role active flag. HyDE, router, and intent-classification
-- pull from this slot. Falls back to chat model at runtime if unset.
ALTER TABLE models
    ADD COLUMN is_active_for_utility INTEGER NOT NULL DEFAULT 0
    CHECK (is_active_for_utility IN (0, 1));

-- Single-active invariant, matching the existing chat/embedding triggers.
CREATE TRIGGER IF NOT EXISTS ensure_single_active_utility_model
    BEFORE UPDATE OF is_active_for_utility ON models
    FOR EACH ROW
    WHEN NEW.is_active_for_utility = 1
BEGIN
    UPDATE models SET is_active_for_utility = 0 WHERE id != NEW.id;
END;

CREATE INDEX IF NOT EXISTS idx_models_active_utility
    ON models(is_active_for_utility) WHERE is_active_for_utility = 1;

CREATE INDEX IF NOT EXISTS idx_models_backend ON models(backend);
