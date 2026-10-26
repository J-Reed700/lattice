-- Remove standalone indexes that duplicate a UNIQUE/PRIMARY KEY index.
-- These names are not referenced by INDEXED BY anywhere in the application.
DROP INDEX IF EXISTS idx_conversation_summaries_conversation_id;
DROP INDEX IF EXISTS idx_document_space_memberships_document_space;
DROP INDEX IF EXISTS idx_document_tags_composite;
DROP INDEX IF EXISTS idx_favorites_document_id;
DROP INDEX IF EXISTS idx_mentions_name;
DROP INDEX IF EXISTS idx_models_model_id;
DROP INDEX IF EXISTS idx_recent_documents_document_id;
DROP INDEX IF EXISTS idx_tags_name;
DROP INDEX IF EXISTS idx_watch_folders_path;

-- Child-side indexes support FK cascade work and embedding-generation cleanup.
CREATE INDEX IF NOT EXISTS idx_embedding_generation_vectors_chunk_id
    ON embedding_generation_vectors(chunk_id);
CREATE INDEX IF NOT EXISTS idx_image_metadata_document_id
    ON image_metadata(document_id);
CREATE INDEX IF NOT EXISTS idx_conversation_summaries_up_to_message_id
    ON conversation_summaries(up_to_message_id);

-- Fail the upgrade with a visible CHECK failure if existing data violates the
-- new document rules. This is a read-only validation; it never repairs rows.
CREATE TEMP TABLE persistence_invariant_validation (
    violation_count INTEGER NOT NULL
        CONSTRAINT documents_id_size_status_invariant CHECK (violation_count = 0)
);
INSERT INTO persistence_invariant_validation
SELECT COUNT(*) FROM documents
WHERE id IS NULL OR size_bytes < 0
   OR typeof(size_bytes) <> 'integer'
   OR status NOT IN ('pending', 'processing', 'indexed', 'failed');
DROP TABLE persistence_invariant_validation;

-- SQLite TEXT PRIMARY KEY historically permits NULL unless explicitly NOT NULL.
-- These guards make new and modified documents conform to the Rust domain model
-- without rebuilding documents (which is referenced by many cascading FKs).
-- Existing rows are intentionally left untouched; no data is silently rewritten.
CREATE TRIGGER IF NOT EXISTS documents_invariants_before_insert
BEFORE INSERT ON documents
WHEN NEW.id IS NULL
  OR NEW.size_bytes < 0
  OR typeof(NEW.size_bytes) <> 'integer'
  OR NEW.status NOT IN ('pending', 'processing', 'indexed', 'failed')
BEGIN
    SELECT RAISE(ABORT, 'documents invariant: id required, size_bytes nonnegative, and status valid');
END;

CREATE TRIGGER IF NOT EXISTS documents_invariants_before_update
BEFORE UPDATE OF id, size_bytes, status ON documents
WHEN NEW.id IS NULL
  OR NEW.size_bytes < 0
  OR typeof(NEW.size_bytes) <> 'integer'
  OR NEW.status NOT IN ('pending', 'processing', 'indexed', 'failed')
BEGIN
    SELECT RAISE(ABORT, 'documents invariant: id required, size_bytes nonnegative, and status valid');
END;
