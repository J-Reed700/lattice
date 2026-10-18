//! Tests for Hub metadata mapping, weight selection, rate limiting and live search.
use super::*;

#[test]
fn catalog_variants_keep_exact_precision_size_and_identity() {
    let repo: HuggingFaceModel = serde_json::from_value(serde_json::json!({
        "id": "publisher/model-Q4", "tags": ["text-generation", "gguf"],
        "siblings": [
            {"rfilename": "model-Q4_0.gguf", "size": 4_000_000_000_u64},
            {"rfilename": "model-Q8_0.gguf", "size": 8_000_000_000_u64},
            {"rfilename": "model-IQ4_XS.gguf", "size": 3_500_000_000_u64},
            {"rfilename": "model-UD-Q5_K_XL.gguf", "size": 5_500_000_000_u64},
            {"rfilename": "model-BF16.gguf", "size": 16_000_000_000_u64},
            {"rfilename": "unlabelled.gguf"},
            {"rfilename": "mmproj-F16.gguf", "size": 500},
            {"rfilename": "Q8/model-Q8_0.gguf", "size": 500},
            {"rfilename": "model-Q8_0-00001-of-00002.gguf", "size": 500},
            {"rfilename": "../model.gguf", "size": 500},
            {"rfilename": "README.md", "size": 500}
        ]
    }))
    .unwrap();
    let variants = repo.gguf_variants();
    assert_eq!(variants.len(), 6);
    for (file, quant, size) in [
        ("model-Q4_0.gguf", "Q4_0", 4.0),
        ("model-Q8_0.gguf", "Q8_0", 8.0),
        ("model-IQ4_XS.gguf", "IQ4_XS", 3.5),
        ("model-UD-Q5_K_XL.gguf", "UD-Q5_K_XL", 5.5),
        ("model-BF16.gguf", "BF16", 16.0),
    ] {
        let variant = variants
            .iter()
            .find(|v| v.preferred_filename.as_deref() == Some(file))
            .unwrap()
            .to_domain_model()
            .unwrap();
        assert_eq!(variant.supported_quantizations, vec![quant]);
        assert_eq!(variant.size_gb, size);
        assert_eq!(variant.minimum_ram_gb, size * 1.5);
        assert_eq!(
            ExternalModelMetadata::decode_hf_download_id(&variant.id),
            Some(("publisher/model-Q4".into(), file.into()))
        );
    }
    let unknown = variants.last().unwrap().to_domain_model().unwrap();
    assert_eq!(unknown.size_gb, 0.0);
    assert_eq!(unknown.minimum_ram_gb, 0.0);
    assert!(unknown.supported_quantizations.is_empty());
}

#[tokio::test]
#[ignore = "Requires internet; reads public metadata, never downloads weights"]
async fn test_model_variants_integration() {
    let variants = HuggingFaceAdapter::new()
        .get_model_variants("unsloth/Qwen3.5-9B-GGUF")
        .await
        .unwrap();
    assert!(variants.len() > 1);
    assert!(variants
        .iter()
        .all(|v| v.preferred_size_bytes.is_some_and(|size| size > 0)));
    assert!(variants.iter().all(|v| v.to_domain_model().is_ok()));
}

fn embedding_repo_with(files: &[&str]) -> HuggingFaceModel {
    let siblings: Vec<serde_json::Value> = files
        .iter()
        .map(|name| serde_json::json!({ "rfilename": name, "size": 4_usize }))
        .collect();
    serde_json::from_value(serde_json::json!({
        "modelId": "BAAI/bge-m3",
        "id": "BAAI/bge-m3",
        "tags": ["sentence-transformers", "xlm-roberta"],
        "pipeline_tag": "feature-extraction",
        "siblings": siblings,
    }))
    .expect("deserialize embedding repo")
}

#[test]
fn weights_selection_falls_back_to_pytorch_bin() {
    let repo = embedding_repo_with(&["pytorch_model.bin", "tokenizer.json", "config.json"]);
    let (filename, _) = repo
        .select_preferred_weights_file()
        .expect("pickle-only repo is loadable");
    assert_eq!(filename, "pytorch_model.bin");
    assert_eq!(
        repo.select_preferred_file().map(|(name, _)| name),
        Some("pytorch_model.bin".to_string()),
    );
}

#[test]
fn weights_selection_prefers_safetensors_over_pytorch_bin() {
    let repo = embedding_repo_with(&[
        "pytorch_model.bin",
        "model.safetensors",
        "tokenizer.json",
        "config.json",
    ]);
    let (filename, _) = repo
        .select_preferred_weights_file()
        .expect("repo with both is loadable");
    assert_eq!(filename, "model.safetensors");
}

#[test]
fn weights_selection_still_requires_tokenizer_and_config() {
    let repo = embedding_repo_with(&["pytorch_model.bin", "config.json"]);
    assert!(repo.select_preferred_weights_file().is_none());
}

#[test]
fn pytorch_bin_repo_is_reported_candle_compatible() {
    let repo = embedding_repo_with(&["pytorch_model.bin", "tokenizer.json", "config.json"]);
    let compatibility = repo
        .to_external_metadata()
        .embedding_compatibility
        .expect("embedding repos carry a compatibility verdict");
    assert!(
        matches!(
            compatibility,
            lattice_core::domain::models::selection::EmbeddingCompatibility::Compatible { .. }
        ),
        "pickle-only repo must not be rejected: {compatibility:?}",
    );
}

#[test]
fn repo_without_any_weights_names_both_candidates() {
    let repo = embedding_repo_with(&["tokenizer.json", "config.json"]);
    let compatibility = repo
        .to_external_metadata()
        .embedding_compatibility
        .expect("embedding repos carry a compatibility verdict");
    let reason = match compatibility {
        lattice_core::domain::models::selection::EmbeddingCompatibility::Incompatible {
            reason,
            ..
        } => reason,
        other => panic!("expected an incompatible verdict, got {other:?}"),
    };
    assert!(reason.contains("model.safetensors"), "{reason}");
    assert!(reason.contains("pytorch_model.bin"), "{reason}");
}

#[test]
fn test_huggingface_adapter_creation() {
    let _adapter = HuggingFaceAdapter::new();
}

#[test]
fn task_discovery_uses_hub_filter_instead_of_name_search() {
    let url = HuggingFaceAdapter::search_url("filter:feature-extraction", 500);
    assert!(url.contains("filter=feature-extraction"), "{url}");
    assert!(!url.contains("search="), "{url}");
    assert!(url.contains("limit=500"), "{url}");
}

#[test]
fn test_huggingface_model_to_external_metadata() {
    let hf_model = HuggingFaceModel {
        model_id: Some("test/model".into()),
        id: None,
        name: Some("Test Model".into()),
        author: None,
        tags: vec!["text-generation".into()],
        downloads: 1000,
        last_modified: Some("2024-01-01T00:00:00Z".into()),
        license: Some("apache-2.0".into()),
        library_name: None,
        likes: 0,
        pipeline_tag: None,
        config: None,
        gguf: None,
        card_data: None,
        gated: None,
        siblings: vec![],
    };

    let external = hf_model.to_external_metadata();

    assert_eq!(external.id, "test/model");
    assert_eq!(external.name, "Test Model");
    assert_eq!(external.tags, vec!["text-generation"]);
    assert_eq!(external.downloads, 1000);
    assert_eq!(external.license, "apache-2.0");
}

#[test]
fn test_huggingface_model_fallbacks() {
    let hf_model = HuggingFaceModel {
        model_id: None,
        id: None,
        name: None,
        author: None,
        tags: vec![],
        downloads: 0,
        last_modified: None,
        gated: None,
        license: None,
        library_name: None,
        likes: 0,
        pipeline_tag: None,
        config: None,
        gguf: None,
        card_data: None,
        siblings: vec![],
    };

    let external = hf_model.to_external_metadata();

    assert_eq!(external.id, "unknown");
    assert_eq!(external.name, "unknown");
    assert_eq!(external.license, "unknown");
    assert_eq!(external.last_modified, "unknown");
}

#[test]
fn test_huggingface_model_deserializes_string_gated() {
    let json = r#"{
            "modelId":"test/model",
            "id":"test/model",
            "name":"Test Model",
            "tags":["text-generation"],
            "downloads":42,
            "lastModified":"2024-01-01T00:00:00Z",
            "license":"apache-2.0",
            "pipeline_tag":"text-generation",
            "gated":"auto",
            "siblings":[]
        }"#;

    let model: HuggingFaceModel = serde_json::from_str(json).expect("deserialize model");
    assert_eq!(model.gated, Some(true));
}

#[test]
fn test_huggingface_model_uses_card_data_summary_for_description() {
    let hf_model = HuggingFaceModel {
        model_id: Some("org/model".into()),
        id: None,
        name: Some("Model".into()),
        author: Some("org".into()),
        tags: vec!["text-generation".into()],
        downloads: 1234,
        last_modified: Some("2024-01-01T00:00:00Z".into()),
        license: Some("apache-2.0".into()),
        library_name: Some("gguf".into()),
        likes: 99,
        pipeline_tag: Some("text-generation".into()),
        config: None,
        gguf: None,
        card_data: Some(serde_json::json!({
            "summary": "A compact instruction-tuned model for chat and coding."
        })),
        gated: None,
        siblings: vec![],
    };

    let external = hf_model.to_external_metadata();
    assert_eq!(
        external.description,
        "A compact instruction-tuned model for chat and coding."
    );
}

#[test]
fn filters_out_auxiliary_draft_and_fork_only_gguf_files() {
    // 1. Hadamard-rotated PrismML Bonsai 2 repo (PQ2_0 / PTQ1_0 / rotated F16) offers no downloadable GGUFs
    let bonsai2: HuggingFaceModel = serde_json::from_value(serde_json::json!({
        "id": "prism-ml/Ternary-Bonsai-2-27B-gguf",
        "tags": ["text-generation", "gguf", "prismml", "bonsai"],
        "siblings": [
            {"rfilename": "Ternary-Bonsai-2-27B-F16.gguf", "size": 53_560_288_416_u64},
            {"rfilename": "Ternary-Bonsai-2-27B-PQ2_0.gguf", "size": 7_206_168_928_u64},
            {"rfilename": "Ternary-Bonsai-2-27B-PTQ1_0.gguf", "size": 5_907_625_120_u64}
        ]
    }))
    .unwrap();
    assert!(bonsai2.gguf_variants().is_empty());
    assert!(bonsai2.to_external_metadata().preferred_filename.is_none());

    // 2. Ternary-Bonsai v1 skips fork-only g128 -Q2_0.gguf and selects mainline -Q2_0_g64.gguf over F16
    let bonsai1: HuggingFaceModel = serde_json::from_value(serde_json::json!({
        "id": "prism-ml/Ternary-Bonsai-8B-gguf",
        "tags": ["text-generation", "gguf", "prismml", "bonsai"],
        "siblings": [
            {"rfilename": "Ternary-Bonsai-8B-F16.gguf"},
            {"rfilename": "Ternary-Bonsai-8B-Q2_0.gguf"},
            {"rfilename": "Ternary-Bonsai-8B-Q2_0_g64.gguf"}
        ]
    }))
    .unwrap();
    assert_eq!(
        bonsai1.to_external_metadata().preferred_filename.as_deref(),
        Some("Ternary-Bonsai-8B-Q2_0_g64.gguf")
    );
    let variants: Vec<_> = bonsai1
        .gguf_variants()
        .into_iter()
        .filter_map(|v| v.preferred_filename)
        .collect();
    assert_eq!(
        variants,
        vec![
            "Ternary-Bonsai-8B-F16.gguf",
            "Ternary-Bonsai-8B-Q2_0_g64.gguf"
        ]
    );

    // 3. Auxiliary files (mtp-, dflash-, -dspark-, imatrix, -vision.gguf) are never selected
    let mixed: HuggingFaceModel = serde_json::from_value(serde_json::json!({
        "id": "prism-ml/Bonsai-27B-gguf",
        "tags": ["text-generation", "gguf"],
        "siblings": [
            {"rfilename": "Bonsai-27B-dspark-Q4_1.gguf"},
            {"rfilename": "dflash-Bonsai-27B-Q4_0.gguf"},
            {"rfilename": "mtp-Bonsai-27B-Q8_0.gguf"},
            {"rfilename": "imatrix-bonsai.gguf"},
            {"rfilename": "Bonsai-27B-Vision.gguf"},
            {"rfilename": "Bonsai-27B-F16.gguf"},
            {"rfilename": "Bonsai-27B-Q1_0.gguf"}
        ]
    }))
    .unwrap();
    assert_eq!(
        mixed.to_external_metadata().preferred_filename.as_deref(),
        Some("Bonsai-27B-Q1_0.gguf")
    );
}

#[test]
fn config_model_type_drives_embedding_compatibility_when_tags_do_not() {
    let model: HuggingFaceModel = serde_json::from_value(serde_json::json!({
        "modelId": "org/modern-embedder",
        "tags": ["sentence-transformers", "safetensors"],
        "pipeline_tag": "feature-extraction",
        "config": { "model_type": "modernbert" },
        "siblings": [
            { "rfilename": "model.safetensors", "size": 1000 },
            { "rfilename": "tokenizer.json", "size": 100 },
            { "rfilename": "config.json", "size": 100 }
        ]
    }))
    .expect("deserialize model");

    let metadata = model.to_external_metadata();
    assert!(metadata.tags.iter().any(|tag| tag == "modernbert"));
    assert!(matches!(
        metadata.embedding_compatibility,
        Some(lattice_core::domain::models::selection::EmbeddingCompatibility::Compatible {
            ref architecture
        }) if architecture == "modernbert"
    ));
}

// Integration tests (requires network, mark with #[ignore])
#[tokio::test]
#[ignore = "Requires network access"]
async fn test_search_models_integration() {
    let adapter = HuggingFaceAdapter::new();
    let results = adapter.search_models("phi-3", 5).await.unwrap();

    assert!(!results.is_empty());
    for result in results {
        assert!(!result.id.is_empty());
        assert!(!result.name.is_empty());
    }
}

#[tokio::test]
#[ignore = "Requires network access"]
async fn test_get_model_by_id_integration() {
    let adapter = HuggingFaceAdapter::new();
    let model = adapter
        .get_model_by_id("microsoft/phi-3-mini-4k-instruct")
        .await
        .unwrap();

    assert!(model.is_some());
    let model = model.unwrap();
    assert!(model.id.contains("phi-3"));
}

#[tokio::test]
#[ignore = "Requires network access"]
async fn test_get_model_by_id_not_found() {
    let adapter = HuggingFaceAdapter::new();
    let model = adapter
        .get_model_by_id("nonexistent/model-that-does-not-exist-12345")
        .await
        .unwrap();

    assert!(model.is_none());
}
