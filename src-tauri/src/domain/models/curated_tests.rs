//! Tests for the curated model catalog and its lookups.

#![cfg(test)]

use super::*;

#[test]
fn test_llm_models_count() {
    let models = get_curated_llm_models();
    assert_eq!(models.len(), 10);
}

/// The default install runs whatever tier first run picks, so each tier is
/// what turns the tool loop on for most users.
#[test]
fn every_first_run_tier_can_call_tools_and_unknown_files_cannot() {
    for ram in [4.0, 12.0, 32.0] {
        let id = recommend_chat_model_for_ram(ram);
        let entry = get_curated_llm_models()
            .into_iter()
            .find(|model| model.id == id)
            .expect("tier is in the catalog");
        let file = entry.default_filename.expect("tier names a file");
        assert!(chat_model_file_supports_tool_calling(&file), "{file}");
        assert!(chat_model_file_supports_tool_calling(&file.to_lowercase()));
    }
    assert!(!chat_model_file_supports_tool_calling(
        "tinyllama-1.1b-chat-v1.0.Q4_K_M.gguf"
    ));
    assert!(!chat_model_file_supports_tool_calling(
        "my-own-finetune.gguf"
    ));
}

/// First run downloads whatever this names, so every tier has to be a real
/// catalog entry with a file to fetch.
#[test]
fn every_ram_tier_recommends_a_downloadable_catalog_entry() {
    let models = get_curated_llm_models();
    for ram in [4.0, 8.0, 15.9, 16.0, 64.0] {
        let id = recommend_chat_model_for_ram(ram);
        let entry = models
            .iter()
            .find(|m| m.id == id)
            .unwrap_or_else(|| panic!("{id} is recommended at {ram} GB but is not in the catalog"));
        assert!(entry.default_filename.is_some(), "{id} names no file");
        assert!(
            !entry.requires_auth,
            "{id} cannot be fetched without a token"
        );
        assert!(
            entry.minimum_ram_gb <= ram.max(4.0),
            "{id} needs {} GB, more than the {ram} GB tier it is recommended for",
            entry.minimum_ram_gb
        );
    }
}

#[test]
fn embedding_catalog_contains_supported_runtime_models() {
    let models = get_curated_embedding_models();
    let ids: std::collections::HashSet<_> = models.iter().map(|model| model.id.as_str()).collect();
    assert_eq!(
        ids,
        std::collections::HashSet::from(["qwen3-embedding-0.6b", "all-minilm-l6-v2"])
    );
}

#[test]
fn test_ocr_models_count() {
    let models = get_curated_ocr_models();
    assert_eq!(models.len(), 2);
}

#[test]
fn test_transcription_models_count() {
    let models = get_curated_transcription_models();
    assert_eq!(models.len(), 2);
}

#[test]
fn all_curated_models_have_unique_ids_and_valid_categories() {
    let models = get_all_curated_models();
    let ids: std::collections::HashSet<_> = models.iter().map(|model| model.id.as_str()).collect();
    assert_eq!(ids.len(), models.len());
    for model in models {
        assert_eq!(get_model_category_by_id(&model.id), Some(model.category));
    }
}

#[test]
fn test_get_by_category() {
    let llms = get_curated_models_by_category(ModelCategory::LLM);
    assert_eq!(llms.len(), 10);

    let embeddings = get_curated_models_by_category(ModelCategory::Embedding);
    assert_eq!(embeddings.len(), 2);

    let ocr = get_curated_models_by_category(ModelCategory::OCR);
    assert_eq!(ocr.len(), 2);

    let transcription = get_curated_models_by_category(ModelCategory::Transcription);
    assert_eq!(transcription.len(), 2);
}

#[test]
fn transcription_catalog_has_two_entries() {
    let models = get_curated_transcription_models();
    assert_eq!(models.len(), 2);
    assert!(models
        .iter()
        .all(|model| model.category == ModelCategory::Transcription));
    assert_eq!(
        get_model_category_by_id("whisper-tiny-q8"),
        Some(ModelCategory::Transcription)
    );
}

#[test]
fn every_transcription_entry_declares_its_three_files() {
    for model in get_curated_transcription_models() {
        assert_eq!(model.files.len(), 3, "{} file count", model.id);
        let names: Vec<&str> = model
            .files
            .iter()
            .map(|file| file.filename.as_str())
            .collect();
        assert!(
            names.contains(&"model.gguf"),
            "{} missing weights",
            model.id
        );
        assert!(
            names.contains(&"config.json"),
            "{} missing config",
            model.id
        );
        assert!(
            names.contains(&"tokenizer.json"),
            "{} missing tokenizer",
            model.id
        );
    }
}

/// Regression guard: the original entries pointed at
/// `model-base-q80.gguf` / `config-base.json` / `melfilters.bytes` in
/// `lmz/candle-whisper`, none of which exist — every download 404'd. Keep
/// the remote filenames pinned to ones that were actually fetched.
#[test]
fn transcription_urls_point_at_files_that_exist() {
    let urls: Vec<String> = get_curated_transcription_models()
        .into_iter()
        .flat_map(|model| model.files.into_iter().map(|file| file.url))
        .collect();

    for url in &urls {
        assert!(
            url.starts_with("https://huggingface.co/"),
            "{url} is not on HF"
        );
        assert!(url.contains("/resolve/main/"), "{url} is not a resolve URL");
        assert!(
            !url.contains("-base") && !url.contains("-small"),
            "{url} points at a whisper size that has no candle GGUF build"
        );
        assert!(
            !url.ends_with("melfilters.bytes"),
            "{url} downloads a table that is bundled with the engine"
        );
    }
}

#[test]
fn test_llm_entries_use_the_sidecar_compatible_format() {
    let models = get_curated_llm_models();
    assert!(models.iter().all(|model| model.format == ModelFormat::Gguf));
}

#[test]
fn test_existing_gguf_entries_default_to_gguf_format() {
    let models = get_curated_llm_models();
    let gguf_count = models
        .iter()
        .filter(|m| m.format == ModelFormat::Gguf)
        .count();
    assert_eq!(gguf_count, 10, "every curated chat model is GGUF");
}

#[test]
fn test_all_models_have_required_fields() {
    let models = get_all_curated_models();

    for model in models {
        assert!(!model.id.is_empty());
        assert!(!model.name.is_empty());
        assert!(!model.description.is_empty());
        assert!(model.size_gb > 0.0);
        assert!(model.minimum_ram_gb > 0.0);
        assert!(model.recommended_ram_gb >= model.minimum_ram_gb);
        assert!(model.context_length > 0);
        assert!(!model.capabilities.is_empty());
        assert!(!model.license.is_empty());
    }
}

#[test]
fn test_llm_models_have_download_urls() {
    let models = get_curated_llm_models();

    for model in models {
        assert!(model.download_url.is_some());
        let url = model.download_url.unwrap();
        assert!(url.starts_with("https://"));
    }
}

#[test]
fn test_model_categories_match() {
    let llms = get_curated_llm_models();
    for model in llms {
        assert_eq!(model.category, ModelCategory::LLM);
    }

    let embeddings = get_curated_embedding_models();
    for model in embeddings {
        assert_eq!(model.category, ModelCategory::Embedding);
    }

    let ocr = get_curated_ocr_models();
    for model in ocr {
        assert_eq!(model.category, ModelCategory::OCR);
    }
}

#[test]
fn test_small_llm_models() {
    let models = get_curated_llm_models();
    let small_models: Vec<_> = models.iter().filter(|m| m.size_gb < 2.5).collect();

    assert!(!small_models.is_empty());
    for model in small_models {
        assert!(model.minimum_ram_gb <= 4.0);
    }
}

#[test]
fn test_performance_tiers() {
    let models = get_all_curated_models();

    let has_fast = models
        .iter()
        .any(|m| matches!(m.performance_tier, PerformanceTier::Fast));
    let has_balanced = models
        .iter()
        .any(|m| matches!(m.performance_tier, PerformanceTier::Balanced));
    let has_accurate = models
        .iter()
        .any(|m| matches!(m.performance_tier, PerformanceTier::Accurate));

    assert!(has_fast);
    assert!(has_balanced);
    assert!(has_accurate);
}

#[test]
fn test_phi3_mini_metadata() {
    let models = get_curated_llm_models();
    let phi3 = models.iter().find(|m| m.id.contains("phi-3-mini")).unwrap();

    assert_eq!(phi3.category, ModelCategory::LLM);
    assert!(phi3.size_gb < 3.0);
    assert_eq!(phi3.license, "MIT");
    assert!(phi3.capabilities.contains(&"chat".into()));
    assert!(phi3.capabilities.contains(&"code".into()));
}

#[test]
fn only_matryoshka_trained_embedding_models_may_be_truncated() {
    assert!(embedding_model_supports_matryoshka("qwen3-embedding-0.6b"));
    assert!(embedding_model_supports_matryoshka(
        "Qwen/Qwen3-Embedding-0.6B"
    ));
    // Documented for multi-granularity retrieval, not nested dimensions.
    assert!(!embedding_model_supports_matryoshka("all-minilm-l6-v2"));
    // Silence is not a promise.
    assert!(!embedding_model_supports_matryoshka("some-local-model"));
    assert!(!embedding_model_supports_matryoshka(""));
}

#[test]
fn test_lookup_embedding_model() {
    let category = get_model_category_by_id("all-minilm-l6-v2");
    assert_eq!(category, Some(ModelCategory::Embedding));
}

#[test]
fn test_lookup_case_insensitive() {
    let category1 = get_model_category_by_id("ALL-MINILM-L6-V2");
    let category2 = get_model_category_by_id("all-minilm-l6-v2");
    assert_eq!(category1, category2);
    assert_eq!(category1, Some(ModelCategory::Embedding));
}

#[test]
fn test_lookup_nonexistent_model() {
    let category = get_model_category_by_id("nonexistent-model-xyz-123");
    assert_eq!(category, None);
}

#[test]
fn embedding_models_use_safetensors_not_onnx() {
    let models = get_curated_embedding_models();
    for model in &models {
        for file in &model.files {
            assert!(
                !file.filename.ends_with(".onnx") && !file.filename.ends_with(".onnx_data"),
                "Embedding model '{}' file '{}' is ONNX; runtime is candle/safetensors only",
                model.id,
                file.filename,
            );
            assert!(
                !file.url.contains("/onnx/"),
                "Embedding model '{}' URL '{}' contains /onnx/ subdir; use the root safetensors path",
                model.id, file.url,
            );
        }
        // The Candle runtime needs a supported weights artifact.
        let has_weights = model
            .files
            .iter()
            .any(|f| f.filename == "model.safetensors" || f.filename == "pytorch_model.bin");
        assert!(
            has_weights,
            "Embedding model '{}' lists neither model.safetensors nor pytorch_model.bin",
            model.id,
        );
    }
}
