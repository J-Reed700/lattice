use super::*;

#[test]
fn test_infer_model_info_llama() {
    let path = PathBuf::from("models/llama-3.1-8b-Q4_K_M.gguf");
    let info = infer_model_info(&path);

    assert!(matches!(info.family, ModelFamily::Llama));
    assert!(matches!(info.quantization, Some(Quantization::Q4)));
}

#[test]
fn test_infer_model_info_mistral() {
    let path = PathBuf::from("models/mistral-7b-Q5_K_M.gguf");
    let info = infer_model_info(&path);

    assert!(matches!(info.family, ModelFamily::Mistral));
    assert!(matches!(info.quantization, Some(Quantization::Q5)));
}

#[test]
fn test_infer_model_info_dolphin_mixtral_not_phi() {
    let path = PathBuf::from("models/dolphin-2.7-mixtral-8x7b.Q3_K_M.gguf");
    let info = infer_model_info(&path);

    assert!(matches!(info.family, ModelFamily::Mistral));
    assert_eq!(info.quantization, None);
}

#[test]
fn test_detect_format_gguf_file_extension() {
    // Doesn't have to exist on disk — extension is the gating signal.
    let path = PathBuf::from("/some/where/llama-3.1-8b-Q4_K_M.gguf");
    assert_eq!(detect_model_format(&path), ModelFormat::Gguf);
}

#[test]
fn test_detect_format_safetensors_directory() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("config.json"), "{}").unwrap();
    std::fs::write(dir.path().join("model.safetensors"), [0u8; 16]).unwrap();
    assert_eq!(detect_model_format(dir.path()), ModelFormat::Safetensors);
}

#[test]
fn test_detect_format_safetensors_sharded() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("config.json"), "{}").unwrap();
    std::fs::write(
        dir.path().join("model-00001-of-00002.safetensors"),
        [0u8; 16],
    )
    .unwrap();
    std::fs::write(
        dir.path().join("model-00002-of-00002.safetensors"),
        [0u8; 16],
    )
    .unwrap();
    assert_eq!(detect_model_format(dir.path()), ModelFormat::Safetensors);
}

#[test]
fn test_detect_format_directory_missing_config_falls_back_to_gguf() {
    // No config.json — not a valid HF safetensors layout.
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("model.safetensors"), [0u8; 16]).unwrap();
    assert_eq!(detect_model_format(dir.path()), ModelFormat::Gguf);
}

#[test]
fn test_detect_format_directory_missing_safetensors_falls_back_to_gguf() {
    // config.json present but no shard — not a valid HF layout.
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("config.json"), "{}").unwrap();
    assert_eq!(detect_model_format(dir.path()), ModelFormat::Gguf);
}

#[test]
fn test_sum_safetensors_dir_sums_all_shards() {
    let dir = tempfile::tempdir().unwrap();
    // 1 MiB + 2 MiB total
    std::fs::write(
        dir.path().join("model-00001-of-00002.safetensors"),
        vec![0u8; 1_048_576],
    )
    .unwrap();
    std::fs::write(
        dir.path().join("model-00002-of-00002.safetensors"),
        vec![0u8; 2 * 1_048_576],
    )
    .unwrap();
    // Files that aren't shards should be ignored.
    std::fs::write(dir.path().join("config.json"), "{}").unwrap();
    std::fs::write(dir.path().join("tokenizer.json"), "{}").unwrap();

    let mb = sum_safetensors_dir_size_mb(dir.path()).unwrap();
    assert_eq!(mb, 3);
}

#[test]
fn test_sum_safetensors_dir_returns_none_when_empty() {
    let dir = tempfile::tempdir().unwrap();
    assert!(sum_safetensors_dir_size_mb(dir.path()).is_none());
}

#[test]
fn test_map_local_model_load_error_tensor_metadata() {
    let path = PathBuf::from("models/dolphin-2.7-mixtral-8x7b.Q3_K_M.gguf");
    let original = LLMError::Other("Cannot find tensor info for blk.0.ffn_gate.weight".to_string());

    let mapped = map_local_model_load_error(original, &path);
    let message = mapped.to_string();

    assert!(message.contains("incompatible"));
    assert!(message.contains("dolphin-2.7-mixtral-8x7b.Q3_K_M.gguf"));
    assert!(message.contains("Cannot find tensor info for blk.0.ffn_gate.weight"));
}

#[tokio::test]

async fn test_mock_llm_port() {
    let mock = MockLLMPort::new();

    use crate::application::services::completion_input::{complete_text, TextCall};
    let response = complete_text(&mock, "test prompt", &[], TextCall::default())
        .await
        .unwrap();
    assert!(response.contains("mock mode") || response.contains("asking about"));

    let response_with_context =
        complete_text(&mock, "test", &["doc1".to_string()], TextCall::default())
            .await
            .unwrap();
    assert!(response_with_context.contains("context"));

    let ready = mock.is_ready().await.unwrap();
    assert!(ready);
}

#[tokio::test]
async fn test_create_llm_with_invalid_path() {
    let config = LLMConfig::Local {
        model_path: PathBuf::from("/nonexistent/model.gguf"),
        n_gpu_layers: 0,
        generation_config: GenerationConfig::default(),
        app_handle: None,
        server: LocalServerOptions::default(),
    };

    let result = create_llm(
        config,
        &SchedulerRegistry::default(),
        &BackendDevices::default(),
    )
    .await;
    assert!(result.is_err());
}
