-- Embedding runtime migrated to candle (safetensors). Old ONNX downloads
-- are no longer loadable. Drop their rows so the user can re-download cleanly.
-- The on-disk files at ~/.cache/lattice/models/<model_id>/ are harmless once
-- the DB row is gone.
--
-- Active-flag handling: model_files has ON DELETE CASCADE on models, and the
-- is_active_for_embedding column lives on the models row itself. Deleting a
-- removed model also removes its active flag, so no separate UPDATE is needed.

DELETE FROM model_files WHERE file_name LIKE '%.onnx' OR file_name LIKE '%.onnx_data';
DELETE FROM models WHERE model_id IN ('all-mpnet-base-v2', 'instructor-xl');
