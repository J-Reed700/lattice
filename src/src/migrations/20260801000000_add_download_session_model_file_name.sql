-- Preserve the manifest-relative model file identity through the download
-- session. The destination basename cannot represent names such as
-- `onnx/model.onnx`, yet the completion saga updates `model_files` by the
-- exact manifest name.
ALTER TABLE download_sessions ADD COLUMN model_file_name TEXT;
