use super::*;

#[test]
fn test_parse_model_category_llm() {
    let result = parse_model_category("LLM").unwrap();
    assert_eq!(result, ModelCategory::LLM);
}

#[test]
fn test_parse_model_category_embedding() {
    let result = parse_model_category("Embedding").unwrap();
    assert_eq!(result, ModelCategory::Embedding);
}

#[test]
fn test_parse_model_category_ocr() {
    let result = parse_model_category("OCR").unwrap();
    assert_eq!(result, ModelCategory::OCR);
}

#[test]
fn test_parse_model_category_invalid() {
    let result = parse_model_category("invalid");
    assert!(result.is_err());
}

fn hf_row(repo: &str, file: &str, downloads: u64) -> DownloadableHfModel {
    let mut model = lattice_core::domain::models::curated::get_curated_llm_models()
        .into_iter()
        .next()
        .expect("catalog has entries");
    model.id = format!("hf::{repo}::{file}");
    model.model_id = Some(repo.into());
    model.default_filename = Some(file.into());
    model.files.clear();
    model.minimum_ram_gb = 99.0;
    DownloadableHfModel {
        model,
        downloads,
        likes: 7,
    }
}

#[test]
fn a_catalog_repository_row_becomes_the_catalog_entry() {
    let mut models = vec![
        hf_row("unsloth/Qwen3.5-9B-GGUF", "Qwen3.5-9B-Q4_K_M.gguf", 1_000),
        hf_row("someone/Other-GGUF", "Other-Q4_K_M.gguf", 5),
    ];
    merge_curated_chat_models(&mut models, "qwen3.5 9b");

    let catalog: Vec<_> = models
        .iter()
        .filter(|entry| entry.model.id.starts_with("qwen3.5-9b"))
        .collect();
    // The plain 9B replaced its Hugging Face row; the MTP build matched the
    // search too and was added.
    assert_eq!(catalog.len(), 2);
    let plain = catalog
        .iter()
        .find(|entry| entry.model.id == "qwen3.5-9b-q4_k_m")
        .expect("plain 9B");
    assert_eq!(plain.downloads, 1_000, "keeps the repository's popularity");
    assert_eq!(plain.likes, 7);
    assert!(
        plain.model.minimum_ram_gb < 10.0,
        "catalog memory, not the guess"
    );
    assert!(models
        .iter()
        .any(|entry| entry.model.id == "hf::someone/Other-GGUF::Other-Q4_K_M.gguf"));
    let ids: HashSet<_> = models.iter().map(|entry| entry.model.id.clone()).collect();
    assert_eq!(ids.len(), models.len(), "no row twice");
}

#[test]
fn the_catalog_overview_lists_every_chat_entry_once() {
    let mut models = vec![hf_row(
        "unsloth/Qwen3.6-35B-A3B-GGUF",
        "Qwen3.6-35B-A3B-UD-Q4_K_M.gguf",
        50,
    )];
    merge_curated_chat_models(&mut models, "gguf");
    let catalog = lattice_core::domain::models::curated::get_curated_llm_models();
    assert_eq!(models.len(), catalog.len());
    for model in catalog {
        assert_eq!(
            models
                .iter()
                .filter(|entry| entry.model.id == model.id)
                .count(),
            1,
            "{}",
            model.id
        );
    }
}

#[test]
fn test_map_cpu_architecture() {
    let arch = map_cpu_architecture();
    // Just verify it returns a valid value
    assert!(matches!(
        arch,
        lattice_core::domain::CpuArchitecture::ARM64
            | lattice_core::domain::CpuArchitecture::X86_64
    ));
}
