-- Replace `backend` (local|ollama) + the implicit single-vs-directory shape
-- of `base_path` with a unified `storage_kind` discriminator that makes
-- invalid combinations unrepresentable.
--
-- Background: `base_path` was a path string that meant either "the weight
-- file" (GGUF / single-shape ONNX/safetensors) or "the model directory"
-- (multi-file safetensors layouts where Candle/mistralrs takes a dir as
-- input). Consumers had to guess which. A SQL CTE in the repository
-- separately tried to re-derive a "primary file" by alphabetical order
-- and overrode the saga-written value, so for safetensors models the
-- DB returned `…/config.json` instead of the directory. Class of bug
-- gone for good with a typed discriminator.
--
-- backend=ollama rows have no on-disk artifact; storage_path is NULL.

ALTER TABLE models
    ADD COLUMN storage_kind TEXT NOT NULL DEFAULT 'local_file'
    CHECK (storage_kind IN ('local_file', 'local_dir', 'remote_ollama'));

ALTER TABLE models
    ADD COLUMN storage_path TEXT;

-- Backfill storage_kind from existing data.
UPDATE models
SET storage_kind = CASE
    WHEN backend = 'ollama' THEN 'remote_ollama'
    WHEN EXISTS (
        SELECT 1 FROM model_files mf
        WHERE mf.model_id = models.model_id
          AND mf.file_name = 'config.json'
    ) THEN 'local_dir'
    ELSE 'local_file'
END;

-- Backfill storage_path from model_files (the SSOT for what's on disk).
-- For local_dir: take the parent directory of config.json.
-- For local_file: prefer .gguf, then .onnx, then .safetensors.
-- For remote_ollama: leave NULL.
UPDATE models
SET storage_path = (
    SELECT
        CASE
            WHEN models.storage_kind = 'local_dir' THEN
                -- SQLite has no dirname(); strip "/config.json" or "\config.json" suffix.
                CASE
                    WHEN substr(mf.file_path, length(mf.file_path) - 11) = '/config.json'
                        THEN substr(mf.file_path, 1, length(mf.file_path) - 12)
                    WHEN substr(mf.file_path, length(mf.file_path) - 11) = '\config.json'
                        THEN substr(mf.file_path, 1, length(mf.file_path) - 12)
                    ELSE mf.file_path
                END
            ELSE mf.file_path
        END
    FROM model_files mf
    WHERE mf.model_id = models.model_id
      AND (
          (models.storage_kind = 'local_dir' AND mf.file_name = 'config.json')
          OR (models.storage_kind = 'local_file' AND (
              mf.file_name LIKE '%.gguf'
              OR (mf.file_name LIKE '%.onnx' AND mf.file_name NOT LIKE '%.onnx_data')
              OR mf.file_name LIKE '%.safetensors'
          ))
      )
    ORDER BY
        CASE
            WHEN mf.file_name LIKE '%.gguf' THEN 0
            WHEN mf.file_name LIKE '%.onnx' AND mf.file_name NOT LIKE '%.onnx_data' THEN 1
            WHEN mf.file_name LIKE '%.safetensors' THEN 2
            ELSE 3
        END
    LIMIT 1
)
WHERE storage_kind != 'remote_ollama';

-- Backfill total_size_bytes from model_files. The CTE we are about to
-- delete was COALESCE-ing this from a SUM aggregate; if we drop the CTE
-- without backfilling, every legacy row reads as 0 bytes.
UPDATE models
SET total_size_bytes = COALESCE((
    SELECT SUM(size_bytes) FROM model_files mf
    WHERE mf.model_id = models.model_id
), total_size_bytes, 0)
WHERE backend != 'ollama';

-- Invariant guard: every non-ollama row must have a storage_path after
-- backfill. If this trips, a row exists in `models` with no corresponding
-- `model_files` rows — likely orphaned by an earlier failed download.
-- Surface it loudly rather than silently leaving a NULL that crashes the
-- loader later.
--
-- Rather than failing the migration outright we emit a sentinel value
-- the loader will reject with a clear "redownload this model" error.
UPDATE models
SET storage_path = '__missing_after_migration__'
WHERE storage_kind != 'remote_ollama' AND storage_path IS NULL;
