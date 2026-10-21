//! Tests for the curated model catalog and its lookups.

#![cfg(test)]

use super::chat::CURATED_CHAT_MODELS;
use super::*;
use crate::domain::models::selection::{ModelFormat, PerformanceTier};
use std::path::Path;

#[test]
fn test_llm_models_count() {
    let models = get_curated_llm_models();
    assert_eq!(models.len(), 9);
}

/// The fresh-install bugs of September came from catalog URLs nobody had
/// fetched. Every chat entry is now one file whose size and digest were
/// read from Hugging Face, and the download is checked against both.
#[test]
fn every_chat_entry_downloads_one_pinned_file() {
    for model in get_curated_llm_models() {
        let repo = model.model_id.as_deref().expect("names its repository");
        let file = model.default_filename.as_deref().expect("names its file");
        assert_eq!(model.files.len(), 1, "{}", model.id);
        let pinned = &model.files[0];
        assert_eq!(pinned.filename, file, "{}", model.id);
        assert_eq!(
            pinned.url,
            format!("https://huggingface.co/{repo}/resolve/main/{file}"),
            "{}",
            model.id
        );
        assert!(pinned.size_bytes > 0, "{}", model.id);
        assert_eq!(pinned.size_bytes, model.total_size_bytes, "{}", model.id);
        let digest = pinned.checksum.as_deref().expect("pins a digest");
        assert_eq!(digest.len(), 64, "{}", model.id);
        assert!(
            digest
                .chars()
                .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c)),
            "{} digest is not lower-case hex",
            model.id
        );
        assert!(file.ends_with(".gguf"), "{}", model.id);
        assert!(!model.requires_auth, "{}", model.id);
    }
}

/// The figures the catalog browser shows as "Est. memory": weights, KV
/// cache at 8K (minimum) and 32K (comfortable) from each GGUF header, and
/// a gigabyte of runtime. Pinned so a changed entry changes them on
/// purpose.
#[test]
fn memory_estimates_are_weights_plus_kv_plus_runtime() {
    let expected = [
        ("qwen3.5-2b-q4_k_m", 2.4, 2.7),
        ("qwen3.5-4b-q4_k_m", 4.1, 4.9),
        ("gemma-4-e4b-it-q4_k_m", 6.2, 6.6),
        ("qwen3.5-9b-q4_k_m", 7.0, 7.8),
        ("qwen3.5-9b-mtp-q4_k_m", 7.2, 8.1),
        ("gemma-4-12b-it-q4_k_m", 8.8, 9.2),
        ("qwen3.8-27b-ud-q4_k_m", 18.1, 19.8),
        ("gemma-4-26b-a4b-it-ud-q4_k_m", 18.5, 19.0),
        ("qwen3.6-35b-a3b-ud-q4_k_xl", 23.6, 24.1),
    ];
    let models = get_curated_llm_models();
    let ids: Vec<&str> = models.iter().map(|m| m.id.as_str()).collect();
    let expected_ids: Vec<&str> = expected.iter().map(|(id, _, _)| *id).collect();
    assert_eq!(ids, expected_ids, "smallest working set first");
    for (model, (id, minimum, comfortable)) in models.iter().zip(expected) {
        assert!(
            (model.minimum_ram_gb - minimum).abs() < 1e-9,
            "{id}: minimum {} GB, expected {minimum}",
            model.minimum_ram_gb
        );
        assert!(
            (model.recommended_ram_gb - comfortable).abs() < 1e-9,
            "{id}: comfortable {} GB, expected {comfortable}",
            model.recommended_ram_gb
        );
        assert!(
            model.minimum_ram_gb > model.size_gb,
            "{id} counts its weights"
        );
    }
}

/// The verdict `computeModelFit` (catalogUtils.ts) gives an Apple Silicon
/// or CPU-only machine: memory may use 70% of RAM, and "Fits" means the
/// model needs at most 80% of that.
fn fits(minimum_ram_gb: f64, machine_ram_gb: f64) -> bool {
    minimum_ram_gb <= 0.8 * 0.7 * machine_ram_gb
}

/// First run downloads what this picks, so on every machine size it has to
/// read "Fits" in the catalog, not merely load. A 4 GB machine is the one
/// exception: nothing current fits it comfortably, and the 2B still loads.
#[test]
fn first_run_tier_fits_every_machine_size() {
    let models = get_curated_llm_models();
    for ram in [4.0, 6.0, 8.0, 12.0, 15.9, 16.0, 24.0, 32.0, 36.0, 64.0] {
        let id = recommend_chat_model_for_ram(ram);
        let entry = models
            .iter()
            .find(|m| m.id == id)
            .unwrap_or_else(|| panic!("{id} is recommended at {ram} GB but is not in the catalog"));
        if ram < 6.0 {
            assert!(entry.minimum_ram_gb <= 0.7 * ram, "{id} at {ram} GB");
        } else {
            assert!(fits(entry.minimum_ram_gb, ram), "{id} at {ram} GB");
        }
    }
}

/// The large entries are honest about the machines they need: the 27B
/// and the Gemma MoE fit from 36 GB, the 35B A3B is too large for 32 GB.
#[test]
fn large_entries_name_the_machines_they_fit() {
    let models = get_curated_llm_models();
    let minimum = |id: &str| {
        models
            .iter()
            .find(|m| m.id == id)
            .map(|m| m.minimum_ram_gb)
            .expect("in the catalog")
    };
    for id in ["qwen3.8-27b-ud-q4_k_m", "gemma-4-26b-a4b-it-ud-q4_k_m"] {
        assert!(
            !fits(minimum(id), 32.0) && minimum(id) <= 0.7 * 32.0,
            "{id}"
        );
        assert!(fits(minimum(id), 36.0), "{id}");
    }
    let moe = minimum("qwen3.6-35b-a3b-ud-q4_k_xl");
    assert!(moe > 0.7 * 32.0, "too large for 32 GB");
    assert!(moe <= 0.7 * 36.0, "loads on 36 GB");
    assert!(fits(moe, 48.0), "fits on 48 GB");
}

/// Only the entry built for the MTP experiment opts in, and only where a
/// catalog download of it lands: the plain 9B publishes the same file name.
#[test]
fn only_the_mtp_build_is_served_with_mtp() {
    let models = Path::new("/Users/someone/.cache/lattice/models");
    assert!(chat_model_uses_mtp_draft(
        &models.join("qwen3.5-9b-mtp-q4_k_m/Qwen3.5-9B-Q4_K_M.gguf")
    ));
    assert!(!chat_model_uses_mtp_draft(
        &models.join("qwen3.5-9b-q4_k_m/Qwen3.5-9B-Q4_K_M.gguf")
    ));
    assert!(!chat_model_uses_mtp_draft(
        &models.join("qwen3.8-27b-ud-q4_k_m/Qwen3.8-27B-UD-Q4_K_M.gguf")
    ));
    assert!(!chat_model_uses_mtp_draft(Path::new(
        "Qwen3.5-9B-Q4_K_M.gguf"
    )));
    let flagged: Vec<_> = CURATED_CHAT_MODELS
        .iter()
        .filter(|model| model.mtp_draft)
        .map(|model| model.id)
        .collect();
    assert_eq!(flagged, ["qwen3.5-9b-mtp-q4_k_m"]);
}

/// Pre-Qwen-3.5 families were removed, not hidden.
#[test]
fn chat_catalog_is_current_generation_only() {
    for model in get_curated_llm_models() {
        let id = model.id.as_str();
        assert!(
            id.starts_with("qwen3.5-")
                || id.starts_with("qwen3.6-")
                || id.starts_with("qwen3.8-")
                || id.starts_with("gemma-4-"),
            "{id} is an older generation"
        );
    }
}

#[test]
fn a_catalog_file_in_the_version_list_is_the_catalog_entry() {
    let entry = curated_chat_model_for_file(
        "unsloth/Qwen3.6-35B-A3B-GGUF",
        "Qwen3.6-35B-A3B-UD-Q4_K_XL.gguf",
    )
    .expect("a catalog file");
    assert_eq!(entry.id, "qwen3.6-35b-a3b-ud-q4_k_xl");
    assert_eq!(entry.files[0].size_bytes, 22_360_456_160);
    // Same file name, other repository: the MTP build, not the plain 9B.
    let mtp = curated_chat_model_for_file("unsloth/Qwen3.5-9B-MTP-GGUF", "Qwen3.5-9B-Q4_K_M.gguf")
        .expect("a catalog file");
    assert_eq!(mtp.id, "qwen3.5-9b-mtp-q4_k_m");
    // Another quantisation of a catalog repository is not vouched for.
    assert!(
        curated_chat_model_for_file("unsloth/Qwen3.5-9B-GGUF", "Qwen3.5-9B-Q8_0.gguf").is_none()
    );
}

#[test]
fn catalog_search_matches_every_term() {
    let ids = |query: &str| -> Vec<String> {
        curated_chat_models_matching(query)
            .into_iter()
            .map(|model| model.id)
            .collect()
    };
    assert_eq!(ids("").len(), CURATED_CHAT_MODELS.len());
    assert_eq!(ids("gguf").len(), CURATED_CHAT_MODELS.len());
    assert_eq!(ids("gemma 12b"), ["gemma-4-12b-it-q4_k_m".to_string()]);
    assert_eq!(ids("Qwen3.6").len(), 1);
    assert!(ids("qwen").len() >= 6);
    assert!(ids("llama").is_empty());
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

#[test]
fn embedding_catalog_contains_supported_runtime_models() {
    let models = get_curated_embedding_models();
    let ids: std::collections::HashSet<_> = models.iter().map(|model| model.id.as_str()).collect();
    assert_eq!(
        ids,
        std::collections::HashSet::from(["qwen3-embedding-0.6b"])
    );
}

#[test]
fn test_ocr_models_count() {
    let models = get_curated_ocr_models();
    assert_eq!(models.len(), 1);
}

/// The OCR engine loads `<dir>/model.gguf` beside `<dir>/mmproj.gguf`, and
/// the download records the first `.gguf` as the model's location.
#[test]
fn every_ocr_entry_downloads_weights_then_projector_with_digests() {
    for model in get_curated_ocr_models() {
        let names: Vec<&str> = model.files.iter().map(|f| f.filename.as_str()).collect();
        assert_eq!(names, [OCR_MODEL_FILE, OCR_PROJECTOR_FILE], "{}", model.id);
        for file in &model.files {
            assert!(file.size_bytes > 0, "{} has no size", file.filename);
            let digest = file.checksum.as_deref().unwrap_or_default();
            assert_eq!(digest.len(), 64, "{} has no sha256", file.filename);
            assert!(file.url.starts_with("https://huggingface.co/"));
            assert!(file.url.contains("/resolve/main/"));
        }
        assert_eq!(
            model.total_size_bytes,
            model.files.iter().map(|f| f.size_bytes).sum::<u64>()
        );
    }
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
    assert_eq!(llms.len(), 9);

    let embeddings = get_curated_models_by_category(ModelCategory::Embedding);
    assert_eq!(embeddings.len(), 1);

    let ocr = get_curated_models_by_category(ModelCategory::OCR);
    assert_eq!(ocr.len(), 1);

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
    assert_eq!(gguf_count, models.len(), "every curated chat model is GGUF");
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
fn only_matryoshka_trained_embedding_models_may_be_truncated() {
    assert!(embedding_model_supports_matryoshka("qwen3-embedding-0.6b"));
    assert!(embedding_model_supports_matryoshka(
        "Qwen/Qwen3-Embedding-0.6B"
    ));
    // Silence is not a promise.
    assert!(!embedding_model_supports_matryoshka("some-local-model"));
    assert!(!embedding_model_supports_matryoshka(""));
}

#[test]
fn test_lookup_embedding_model() {
    let category = get_model_category_by_id("qwen3-embedding-0.6b");
    assert_eq!(category, Some(ModelCategory::Embedding));
}

#[test]
fn test_lookup_case_insensitive() {
    let category1 = get_model_category_by_id("QWEN3-EMBEDDING-0.6B");
    let category2 = get_model_category_by_id("qwen3-embedding-0.6b");
    assert_eq!(category1, category2);
    assert_eq!(category1, Some(ModelCategory::Embedding));
}

#[test]
fn test_lookup_nonexistent_model() {
    let category = get_model_category_by_id("nonexistent-model-xyz-123");
    assert_eq!(category, None);
}

/// The embedding runtime is the bundled llama-server: one F16 GGUF, plus the
/// `tokenizer.json` passages are cut with, each pinned to a commit and a
/// digest so the engine verifies it and the GGUF's digest is the identity.
#[test]
fn every_embedding_entry_downloads_one_pinned_gguf_and_its_tokenizer() {
    use crate::domain::models::checkpoint::TOKENIZER_JSON;
    use crate::domain::models::selection::ModelFormat;
    for model in get_curated_embedding_models() {
        assert_eq!(model.format, ModelFormat::Gguf, "{}", model.id);
        assert_eq!(model.supported_quantizations, ["F16"], "{}", model.id);
        let names: Vec<&str> = model.files.iter().map(|f| f.filename.as_str()).collect();
        assert_eq!(names.len(), 2, "{}", model.id);
        assert!(names[0].ends_with(".gguf"), "{}", model.id);
        assert_eq!(names[1], TOKENIZER_JSON, "{}", model.id);
        for file in &model.files {
            assert!(file.size_bytes > 0, "{} has no size", file.filename);
            let digest = file.checksum.as_deref().unwrap_or_default();
            assert_eq!(digest.len(), 64, "{} has no sha256", file.filename);
            let revision = file
                .url
                .split("/resolve/")
                .nth(1)
                .and_then(|rest| rest.split('/').next())
                .unwrap_or_default();
            assert!(
                revision.len() == 40 && revision.bytes().all(|b| b.is_ascii_hexdigit()),
                "{} is not pinned to a commit",
                file.url
            );
        }
        assert_eq!(
            model.total_size_bytes,
            model.files.iter().map(|f| f.size_bytes).sum::<u64>()
        );
    }
}

/// Every chat entry's repository publishes an F16 projector, pinned like
/// the weights; the projector stays out of `files`, so a model download is
/// still the weights alone, and the catalog tags the entry as seeing.
#[test]
fn every_chat_entry_pins_a_projector_and_is_tagged_as_seeing() {
    use crate::domain::models::vision::VISION_CAPABILITY;
    for model in CURATED_CHAT_MODELS {
        let vision = model
            .vision
            .expect("every catalog repository ships a projector");
        assert_eq!(
            vision.projector_remote_file, "mmproj-F16.gguf",
            "{}",
            model.id
        );
        assert_eq!(vision.projector_sha256.len(), 64, "{}", model.id);
        assert!(vision.projector_bytes > 100_000_000, "{}", model.id);
        assert_eq!(vision.geometry.problem(), None, "{}", model.id);
        let metadata = curated_chat_model_for_file(model.repo, model.file).unwrap();
        assert!(metadata.capabilities.iter().any(|c| c == VISION_CAPABILITY));
        assert_eq!(metadata.files.len(), 1, "{}", model.id);
    }
}

/// The projector is found where a catalog download lands, beside the
/// weights, and the MTP build gets its own repository's projector.
#[test]
fn a_downloaded_entry_finds_its_projector_beside_its_weights() {
    let models = Path::new("/Users/someone/.cache/lattice/models");
    let plain = chat_model_vision(&models.join("qwen3.5-9b-q4_k_m/Qwen3.5-9B-Q4_K_M.gguf"))
        .expect("the 9B declares a projector");
    assert_eq!(
        plain.projector_path,
        models.join("qwen3.5-9b-q4_k_m").join(CHAT_PROJECTOR_FILE)
    );
    assert_eq!(
        plain.projector.url,
        "https://huggingface.co/unsloth/Qwen3.5-9B-GGUF/resolve/main/mmproj-F16.gguf"
    );
    assert!(plain.is_complete(918_166_080));
    assert!(!plain.is_complete(918_166_079));
    let mtp = chat_model_vision(&models.join("qwen3.5-9b-mtp-q4_k_m/Qwen3.5-9B-Q4_K_M.gguf"))
        .expect("the MTP build declares a projector");
    assert_ne!(mtp.projector.checksum, plain.projector.checksum);
    assert!(chat_model_vision(Path::new("Qwen3.5-9B-Q4_K_M.gguf")).is_none());
    assert!(chat_model_vision(&models.join("custom/Qwen3.5-9B-Q4_K_M.gguf")).is_none());
}

/// Qwen pictures stay inside the pinned server's 4,096-square ceiling, so it
/// never shrinks them again; Gemma 4's stay inside its 1,120-token budget.
#[test]
fn catalog_geometries_send_what_the_pinned_server_keeps() {
    let qwen = chat_model_vision(Path::new("m/qwen3.5-4b-q4_k_m/Qwen3.5-4B-Q4_K_M.gguf"))
        .unwrap()
        .geometry;
    let fitted = qwen.plan(4000, 3000);
    assert!(
        fitted.tokens - 2 <= 2048 && fitted.width.is_multiple_of(32),
        "{fitted:?}"
    );
    let gemma = chat_model_vision(Path::new(
        "m/gemma-4-e4b-it-q4_k_m/gemma-4-E4B-it-Q4_K_M.gguf",
    ))
    .unwrap()
    .geometry;
    let fitted = gemma.plan(4000, 3000);
    assert!(
        fitted.tokens - 2 <= 1120 && fitted.height.is_multiple_of(48),
        "{fitted:?}"
    );
    assert_eq!(gemma.min_tokens(), 72);
}
