-- Correct the incomplete 20260428 ONNX purge without changing that historical
-- migration's checksum. By the time this runs, old ONNX children may already
-- be gone, so identify invalid local embedding rows by the invariant the
-- Candle runtime actually needs: at least one safetensors weight file.
--
-- Removing the parent cascades its config/tokenizer-only children and clears
-- any stale active-embedding flag. First-run setup can then offer a valid
-- replacement instead of treating an unloadable `completed` row as ready.
DELETE FROM models
WHERE model_type = 'embedding'
  AND backend != 'ollama'
  AND NOT EXISTS (
      SELECT 1
      FROM model_files mf
      WHERE mf.model_id = models.model_id
        AND lower(mf.file_name) LIKE '%.safetensors'
  );
