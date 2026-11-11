//! Tests for system capability helpers and download URL safety.

#![cfg(test)]

use super::*;

fn create_test_capabilities() -> SystemCapabilities {
    SystemCapabilities {
        total_ram_gb: 16.0,
        // Removed: available_ram_gb
        cpu_cores: 8,
        cpu_architecture: CpuArchitecture::ARM64,
        gpu_type: GpuType::AppleSilicon,
        gpu_acceleration: GpuAcceleration::Metal,
        vram_gb: None,
        available_disk_gb: 100.0,
    }
}

fn create_test_model() -> ModelMetadata {
    ModelMetadata {
        id: "phi-3-mini".into(),
        name: "Phi-3 Mini".into(),
        category: ModelCategory::LLM,
        description: "Compact model".into(),
        size_gb: 1.8,
        context_length: 4096,
        performance_tier: PerformanceTier::Fast,
        supported_quantizations: vec!["Q4_K_M".into()],
        capabilities: vec!["chat".into()],
        download_url: None,
        license: "MIT".into(),
        requires_auth: false,
        model_id: Some("microsoft/Phi-3-mini-4k-instruct-gguf".into()),
        default_filename: Some("Phi-3-mini-4k-instruct-q4.gguf".into()),
        files: vec![],
        total_size_bytes: 0,
        embedding_dimensions: None,
        embedding_compatibility: None,
        format: ModelFormat::Gguf,
    }
}

#[test]
fn test_system_capabilities_helpers() {
    let capabilities = create_test_capabilities();

    assert!(capabilities.has_sufficient_ram(4.0));
    assert!(capabilities.has_sufficient_ram(16.0)); // 16 GB total IS sufficient for 16 GB required
    assert!(!capabilities.has_sufficient_ram(20.0)); // But NOT sufficient for 20 GB
    assert!(capabilities.has_sufficient_disk(50.0));
    assert!(capabilities.has_gpu_acceleration());
    assert_eq!(capabilities.effective_ram_gb(), 16.0);
}

#[test]
fn test_model_fits_on_disk() {
    let model = create_test_model();

    assert!(model.fits_on_disk(50.0));
    assert!(!model.fits_on_disk(1.0));
}

#[test]
fn test_build_download_url_security_valid() {
    let model = create_test_model();
    let url = model.build_download_url();

    assert!(url.is_ok());
    assert!(url.unwrap().contains("huggingface.co"));
}

#[test]
fn test_build_download_url_prevents_path_traversal_in_model_id() {
    let mut model = create_test_model();
    model.model_id = Some("../../../etc/passwd".into());

    let result = model.build_download_url();

    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("Invalid model_id format"));
}

#[test]
fn test_build_download_url_prevents_path_traversal_in_filename() {
    let mut model = create_test_model();
    model.default_filename = Some("../../../etc/passwd".into());

    let result = model.build_download_url();

    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("Invalid filename: not a single plain file name"));
}

#[test]
fn test_build_download_url_prevents_directory_separators_in_filename() {
    let mut model = create_test_model();
    model.default_filename = Some("subdir/malicious.gguf".into());

    let result = model.build_download_url();

    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("Invalid filename: not a single plain file name"));
}

#[test]
fn a_download_url_keeps_two_dots_inside_a_name() {
    let mut model = create_test_model();
    model.model_id = Some("org/repo..v2".into());
    model.default_filename = Some("model..Q4_K_M.gguf".into());

    let url = model.build_download_url().unwrap();

    assert!(url.ends_with("/org/repo..v2/resolve/main/model..Q4_K_M.gguf"));
}

#[test]
fn test_build_download_url_requires_slash_in_model_id() {
    let mut model = create_test_model();
    model.model_id = Some("invalid-without-slash".into());

    let result = model.build_download_url();

    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("Invalid model_id format"));
}

#[test]
fn test_build_download_url_url_encodes_components() {
    let mut model = create_test_model();
    model.model_id = Some("org/repo with spaces".into());
    model.default_filename = Some("file with spaces.gguf".into());

    let result = model.build_download_url();

    assert!(result.is_ok());
    let url = result.unwrap();
    // URL encoding should replace spaces with %20
    assert!(url.contains("%20"));
    assert!(!url.contains(" "));
}

#[test]
fn test_build_download_url_falls_back_to_download_url() {
    let mut model = create_test_model();
    model.model_id = None;
    model.default_filename = None;
    model.download_url = Some("https://example.com/model.gguf".into());

    let result = model.build_download_url();

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), "https://example.com/model.gguf");
}

#[test]
fn test_build_download_url_no_url_available() {
    let mut model = create_test_model();
    model.model_id = None;
    model.default_filename = None;
    model.download_url = None;

    let result = model.build_download_url();

    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("No download URL available"));
}
