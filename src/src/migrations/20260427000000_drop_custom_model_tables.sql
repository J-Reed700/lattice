-- Custom model slice was orphaned (no Tauri commands, no frontend callers) and
-- contained latent bugs (SQL CHECK constraints disagreed with domain enums).
-- Delete the unused tables. Future custom-model support should fold into the
-- existing models/model_files tables instead.
DROP TABLE IF EXISTS custom_model_files;
DROP TABLE IF EXISTS custom_models;
