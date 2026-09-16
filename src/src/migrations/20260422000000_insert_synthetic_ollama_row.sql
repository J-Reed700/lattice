-- Insert a single synthetic Ollama-server row into the unified models
-- registry. The AI Models tab toggles its role flags (is_active_for_chat
-- / is_active_for_utility); the actual model tags live in LLM settings.
--
-- `model_id` is stable and immutable (`__ollama_server__`) so subsequent
-- app runs idempotently find the same row. The `backend = 'ollama'`
-- column is what loaders use to decide between local-file dispatch and
-- remote-HTTP dispatch.
--
-- Existing single-active triggers give us mutual exclusion for free: if
-- the user toggles Ollama-for-chat ON, any previously-active local row
-- is automatically cleared.

INSERT OR IGNORE INTO models (
    id,
    model_name,
    model_id,
    base_path,
    total_size_bytes,
    status,
    model_type,
    architecture,
    downloaded_at,
    last_used_at,
    use_count,
    is_active_for_chat,
    is_active_for_embedding,
    is_active_for_utility,
    backend,
    metadata
)
VALUES (
    '__ollama_server__',
    'Ollama Server',
    '__ollama_server__',
    '',
    0,
    'completed',
    'chat',
    'ollama',
    CURRENT_TIMESTAMP,
    NULL,
    0,
    0,
    0,
    0,
    'ollama',
    '{"synthetic":true,"source":"ollama_server_row"}'
);
