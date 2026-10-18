//! Tests for the mock catalog and the mapping of external metadata to domain models.

#![cfg(test)]

use super::*;

#[tokio::test]

async fn test_mock_catalog_search() {
    let catalog = MockModelCatalogPort::new();
    let results = catalog.search_models("llama", 10).await.unwrap();

    assert_eq!(results.len(), 3);
    assert!(results[0].id.contains("llama"));
}

#[tokio::test]
async fn test_mock_catalog_get_by_id() {
    let catalog = MockModelCatalogPort::new();
    let model = catalog.get_model_by_id("llama-3.2-7b").await.unwrap();

    assert!(model.is_some());
    let model = model.unwrap();
    assert_eq!(model.name, "Llama 3.2 7B");
}

#[tokio::test]
async fn test_mock_catalog_not_found() {
    let catalog = MockModelCatalogPort::new();
    let model = catalog.get_model_by_id("nonexistent").await.unwrap();

    assert!(model.is_none());
}

#[tokio::test]
async fn test_mock_catalog_empty() {
    let catalog = MockModelCatalogPort::empty();
    let results = catalog.search_models("test", 10).await.unwrap();

    assert_eq!(results.len(), 0);
}

#[test]
fn test_external_metadata_to_domain_llm() {
    let external = ExternalModelMetadata {
        id: "test/llama-7b-q4".into(),
        name: "Llama 7B Q4".into(),
        description: "Test LLM model".into(),
        tags: vec!["text-generation".into(), "llama".into()],
        downloads: 1000,
        likes: 0,
        download_url: None,
        license: "apache-2.0".into(),
        last_modified: "2024-01-01T00:00:00Z".into(),
        gated: None,
        preferred_filename: None,
        preferred_size_bytes: None,
        embedding_compatibility: None,
    };

    let domain = external.to_domain_model().unwrap();

    assert_eq!(domain.category, ModelCategory::LLM);
    assert!(domain.size_gb > 0.0);
    assert!(domain.minimum_ram_gb >= 4.0);
    assert!(domain.capabilities.contains(&"chat".into()));
}

#[test]
fn test_external_metadata_to_domain_embedding() {
    let external = ExternalModelMetadata {
        id: "test/bge-m3".into(),
        name: "BGE-M3".into(),
        description: "Embedding model".into(),
        tags: vec!["sentence-transformers".into(), "feature-extraction".into()],
        downloads: 500,
        likes: 0,
        download_url: None,
        license: "MIT".into(),
        last_modified: "2024-01-01T00:00:00Z".into(),
        gated: None,
        preferred_filename: None,
        preferred_size_bytes: None,
        embedding_compatibility: None,
    };

    let domain = external.to_domain_model().unwrap();

    assert_eq!(domain.category, ModelCategory::Embedding);
    assert!(domain.capabilities.contains(&"embedding".into()));
}

#[test]
fn test_infer_category_llm() {
    let external = ExternalModelMetadata {
        id: "test".into(),
        name: "Test".into(),
        description: "".into(),
        tags: vec!["text-generation".into()],
        downloads: 0,
        likes: 0,
        download_url: None,
        license: "".into(),
        last_modified: "".into(),
        gated: None,
        preferred_filename: None,
        preferred_size_bytes: None,
        embedding_compatibility: None,
    };

    let category = external.infer_category().unwrap();
    assert_eq!(category, ModelCategory::LLM);
}

#[test]
fn embedding_task_wins_over_decoder_architecture_tag() {
    let external = ExternalModelMetadata {
        id: "Qwen/Qwen3-Embedding-0.6B".into(),
        name: "Qwen3 Embedding 0.6B".into(),
        description: "Decoder-backed embedding model".into(),
        tags: vec![
            "sentence-transformers".into(),
            "qwen3".into(),
            "text-generation".into(),
            "feature-extraction".into(),
        ],
        downloads: 0,
        likes: 0,
        download_url: None,
        license: "apache-2.0".into(),
        last_modified: "".into(),
        gated: Some(false),
        preferred_filename: Some("model.safetensors".into()),
        preferred_size_bytes: None,
        embedding_compatibility: Some(
            crate::domain::models::selection::EmbeddingCompatibility::Compatible {
                architecture: "qwen3".into(),
            },
        ),
    };

    assert_eq!(external.infer_category().unwrap(), ModelCategory::Embedding);
    assert_eq!(
        external.to_domain_model().unwrap().category,
        ModelCategory::Embedding
    );
}

#[test]
fn test_estimate_size_7b_q4() {
    let external = ExternalModelMetadata {
        id: "test/model-7b-q4".into(),
        name: "Model 7B Q4".into(),
        description: "".into(),
        tags: vec![],
        downloads: 0,
        likes: 0,
        download_url: None,
        license: "".into(),
        last_modified: "".into(),
        gated: None,
        preferred_filename: None,
        preferred_size_bytes: None,
        embedding_compatibility: None,
    };

    let size = external.estimate_size_gb();
    assert!(size > 3.0 && size < 6.0);
}

#[test]
fn test_extract_context_length() {
    let external = ExternalModelMetadata {
        id: "test".into(),
        name: "Model 32k context".into(),
        description: "".into(),
        tags: vec![],
        downloads: 0,
        likes: 0,
        download_url: None,
        license: "".into(),
        last_modified: "".into(),
        gated: None,
        preferred_filename: None,
        preferred_size_bytes: None,
        embedding_compatibility: None,
    };

    let context = external.extract_context_length();
    assert_eq!(context, 32768);
}

#[test]
fn test_extract_quantizations() {
    let external = ExternalModelMetadata {
        id: "test/model-q4-gguf".into(),
        name: "Test".into(),
        description: "".into(),
        tags: vec![],
        downloads: 0,
        likes: 0,
        download_url: None,
        license: "".into(),
        last_modified: "".into(),
        gated: None,
        preferred_filename: None,
        preferred_size_bytes: None,
        embedding_compatibility: None,
    };

    let quants = external.extract_quantizations();
    assert_eq!(quants, vec!["Q4"]);
}

#[test]
fn test_extract_capabilities() {
    let external = ExternalModelMetadata {
        id: "test".into(),
        name: "Test".into(),
        description: "".into(),
        tags: vec!["chat".into(), "code".into(), "reasoning".into()],
        downloads: 0,
        likes: 0,
        download_url: None,
        license: "".into(),
        last_modified: "".into(),
        gated: None,
        preferred_filename: None,
        preferred_size_bytes: None,
        embedding_compatibility: None,
    };

    let caps = external.extract_capabilities();
    assert!(caps.contains(&"chat".into()));
    assert!(caps.contains(&"code".into()));
    assert!(caps.contains(&"reasoning".into()));
}

#[test]
fn test_hf_id_round_trip() {
    let internal = ExternalModelMetadata::encode_hf_download_id(
        "meta-llama/Llama-3.2-3B-Instruct-GGUF",
        "Llama-3.2-3B-Instruct-Q4_K_M.gguf",
    );

    let decoded = ExternalModelMetadata::decode_hf_download_id(&internal).unwrap();
    assert_eq!(decoded.0, "meta-llama/Llama-3.2-3B-Instruct-GGUF");
    assert_eq!(decoded.1, "Llama-3.2-3B-Instruct-Q4_K_M.gguf");
}

#[test]
fn test_to_domain_model_uses_internal_hf_id_when_downloadable() {
    let external = ExternalModelMetadata {
        id: "meta-llama/Llama-3.2-3B-Instruct-GGUF".into(),
        name: "Llama 3.2 3B Instruct".into(),
        description: "Test model".into(),
        tags: vec!["text-generation".into(), "gguf".into()],
        downloads: 100,
        likes: 0,
        download_url: None,
        license: "llama".into(),
        last_modified: "2024-01-01T00:00:00Z".into(),
        gated: Some(true),
        preferred_filename: Some("Llama-3.2-3B-Instruct-Q4_K_M.gguf".into()),
        preferred_size_bytes: Some(2_300_000_000),
        embedding_compatibility: None,
    };

    let domain = external.to_domain_model().unwrap();
    let decoded = ExternalModelMetadata::decode_hf_download_id(&domain.id).unwrap();

    assert_eq!(decoded.0, "meta-llama/Llama-3.2-3B-Instruct-GGUF");
    assert_eq!(decoded.1, "Llama-3.2-3B-Instruct-Q4_K_M.gguf");
    assert_eq!(
        domain.model_id,
        Some("meta-llama/Llama-3.2-3B-Instruct-GGUF".into())
    );
    assert_eq!(
        domain.default_filename,
        Some("Llama-3.2-3B-Instruct-Q4_K_M.gguf".into())
    );
    assert!(domain.requires_auth);
}

#[test]
fn non_llm_gguf_modalities_are_not_misclassified_as_chat_llms() {
    for tags in [
        vec!["text-to-speech", "gguf", "audio.cpp"],
        vec!["music-generation", "text-generation", "gguf"],
        vec!["image-text-to-video", "comfyui", "gguf"],
        vec!["text-to-image", "gguf"],
        vec!["object-detection", "locate-anything.cpp", "gguf"],
    ] {
        let external = ExternalModelMetadata {
            id: "org/non-llm-gguf".into(),
            name: "Non LLM".into(),
            description: "".into(),
            tags: tags.into_iter().map(String::from).collect(),
            downloads: 100,
            likes: 0,
            download_url: None,
            license: "apache-2.0".into(),
            last_modified: "".into(),
            gated: None,
            preferred_filename: Some("model-Q4_K_M.gguf".into()),
            preferred_size_bytes: Some(1_000_000_000),
            embedding_compatibility: None,
        };
        assert!(external.to_domain_model().is_err());
    }

    let asr = ExternalModelMetadata {
        id: "org/asr-gguf".into(),
        name: "ASR".into(),
        description: "".into(),
        tags: vec!["automatic-speech-recognition".into(), "gguf".into()],
        downloads: 100,
        likes: 0,
        download_url: None,
        license: "apache-2.0".into(),
        last_modified: "".into(),
        gated: None,
        preferred_filename: Some("asr-Q8_0.gguf".into()),
        preferred_size_bytes: Some(500_000_000),
        embedding_compatibility: None,
    };
    assert_eq!(
        asr.to_domain_model().unwrap().category,
        ModelCategory::Transcription
    );
}
