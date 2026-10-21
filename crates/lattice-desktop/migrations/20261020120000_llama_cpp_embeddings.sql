-- Safetensors embedding models cannot be loaded by the llama.cpp runtime: their rows go, their files stay for the user to remove in Settings.
DELETE FROM models WHERE model_type = 'embedding' AND storage_kind = 'local_dir';
