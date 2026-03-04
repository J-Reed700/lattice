CREATE EXTENSION IF NOT EXISTS pgcrypto;

DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_type WHERE typname = 'sync_action') THEN
        CREATE TYPE sync_action AS ENUM ('create', 'update', 'delete');
    END IF;
    IF NOT EXISTS (SELECT 1 FROM pg_type WHERE typname = 'conflict_status') THEN
        CREATE TYPE conflict_status AS ENUM (
            'pending',
            'resolved_local',
            'resolved_remote',
            'resolved_merge'
        );
    END IF;
END
$$;

CREATE TABLE IF NOT EXISTS devices (
    id BIGSERIAL PRIMARY KEY,
    user_id BIGINT NOT NULL,
    device_uuid UUID NOT NULL,
    device_name TEXT NOT NULL,
    last_seen_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (user_id, device_uuid)
);

CREATE INDEX IF NOT EXISTS idx_devices_user_id ON devices (user_id);

CREATE TABLE IF NOT EXISTS document_heads (
    id BIGSERIAL PRIMARY KEY,
    user_id BIGINT NOT NULL,
    path TEXT NOT NULL,
    title TEXT,
    content TEXT,
    content_hash VARCHAR(64),
    version BIGINT NOT NULL DEFAULT 1,
    last_modified_device_id BIGINT REFERENCES devices (id) ON DELETE SET NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    deleted_at TIMESTAMPTZ,
    deleted_by_device_id BIGINT REFERENCES devices (id) ON DELETE SET NULL,
    UNIQUE (user_id, path)
);

CREATE INDEX IF NOT EXISTS idx_document_heads_user_updated
    ON document_heads (user_id, updated_at DESC);
CREATE INDEX IF NOT EXISTS idx_document_heads_user_active
    ON document_heads (user_id, deleted_at);

CREATE TABLE IF NOT EXISTS document_ops (
    seq BIGSERIAL PRIMARY KEY,
    op_id UUID NOT NULL DEFAULT gen_random_uuid(),
    user_id BIGINT NOT NULL,
    device_id BIGINT NOT NULL REFERENCES devices (id) ON DELETE CASCADE,
    client_op_id UUID NOT NULL,
    action sync_action NOT NULL,
    path TEXT NOT NULL,
    title TEXT,
    content TEXT,
    content_hash VARCHAR(64),
    base_version BIGINT,
    applied_version BIGINT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (user_id, device_id, client_op_id)
);

CREATE INDEX IF NOT EXISTS idx_document_ops_user_seq
    ON document_ops (user_id, seq);
CREATE INDEX IF NOT EXISTS idx_document_ops_user_path_seq
    ON document_ops (user_id, path, seq DESC);

CREATE TABLE IF NOT EXISTS device_checkpoints (
    user_id BIGINT NOT NULL,
    device_id BIGINT NOT NULL REFERENCES devices (id) ON DELETE CASCADE,
    last_acked_seq BIGINT NOT NULL DEFAULT 0,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (user_id, device_id)
);

CREATE TABLE IF NOT EXISTS conflicts (
    id BIGSERIAL PRIMARY KEY,
    user_id BIGINT NOT NULL,
    path TEXT NOT NULL,
    server_op_seq BIGINT REFERENCES document_ops (seq) ON DELETE SET NULL,
    incoming_device_id BIGINT REFERENCES devices (id) ON DELETE SET NULL,
    incoming_client_op_id UUID,
    server_version BIGINT,
    incoming_base_version BIGINT,
    status conflict_status NOT NULL DEFAULT 'pending',
    resolution_content TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    resolved_at TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS idx_conflicts_user_status
    ON conflicts (user_id, status, created_at DESC);

CREATE TABLE IF NOT EXISTS outbox (
    id BIGSERIAL PRIMARY KEY,
    topic TEXT NOT NULL,
    key TEXT NOT NULL,
    payload JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    published_at TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS idx_outbox_unpublished
    ON outbox (published_at, id)
    WHERE published_at IS NULL;
