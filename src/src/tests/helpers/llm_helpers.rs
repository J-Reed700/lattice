#![allow(clippy::panic)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::assertions_on_constants)]
#![allow(clippy::useless_vec)]
#![allow(clippy::indexing_slicing)]
#![allow(unused_variables)]
#![allow(unused_imports)]
#![allow(deprecated)]

//! Test helpers for LLM integration tests.
// Test code - allow common test patterns
#![allow(clippy::panic)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::assertions_on_constants)]
#![allow(clippy::useless_vec)]
#![allow(unused_variables)]
#![allow(unused_imports)]

//!
//! Provides utilities for:
//! - Creating mock system capabilities
//! - Checking service availability
//! - Test model paths
//! - Common test fixtures

use std::path::PathBuf;
use std::sync::Arc;

use lattice::infrastructure::llm::{
    GPUInfo, GPUVendor, LLMClient, OllamaClient, Platform, SystemCapabilities,
};

// ============================================================================
// Mock System Capabilities
// ============================================================================

/// Create a mock system capabilities for testing.
///
/// # Arguments
/// * `ram_gb` - Total RAM in GB
/// * `has_gpu` - Whether to include GPU
///
/// # Returns
/// A `SystemCapabilities` struct with mock values
pub fn create_mock_capabilities(ram_gb: f64, has_gpu: bool) -> SystemCapabilities {
    SystemCapabilities {
        total_ram_gb: ram_gb,
        available_ram_gb: ram_gb * 0.8,
        gpu: if has_gpu {
            Some(create_mock_gpu())
        } else {
            None
        },
        cpu_cores: 8,
        cpu_threads: 16,
        cpu_model: Some("Test CPU Model".to_string()),
        cpu_architecture: Some("x86_64".to_string()),
        platform: Platform::Linux,
        os_version: Some("Test OS 1.0".to_string()),
    }
}

/// Create a low-end system (4GB RAM, no GPU)
pub fn create_low_end_system() -> SystemCapabilities {
    create_mock_capabilities(4.0, false)
}

/// Create a mid-range system (16GB RAM, GPU)
pub fn create_mid_range_system() -> SystemCapabilities {
    create_mock_capabilities(16.0, true)
}

/// Create a high-end system (32GB RAM, high-end GPU)
pub fn create_high_end_system() -> SystemCapabilities {
    let mut caps = create_mock_capabilities(32.0, true);

    // Override with high-end GPU
    caps.gpu = Some(GPUInfo {
        vendor: GPUVendor::Nvidia,
        name: "NVIDIA RTX 4090".to_string(),
        vram_gb: Some(24.0),
        compute_capability: Some("8.9".to_string()),
    });

    caps
}

/// Create a mock GPU info
pub fn create_mock_gpu() -> GPUInfo {
    GPUInfo {
        vendor: GPUVendor::Nvidia,
        name: "Test GPU".to_string(),
        vram_gb: Some(8.0),
        compute_capability: Some("8.6".to_string()),
    }
}

/// Create a mock Apple Silicon GPU
pub fn create_apple_gpu() -> GPUInfo {
    GPUInfo {
        vendor: GPUVendor::Apple,
        name: "Apple M1 Pro".to_string(),
        vram_gb: Some(16.0), // Unified memory
        compute_capability: None,
    }
}

// ============================================================================
// Service Availability Checks
// ============================================================================

/// Check if Ollama is running on localhost.
pub async fn is_ollama_available() -> bool {
    is_ollama_available_at("http://localhost:11434").await
}

/// Check if Ollama is running at a specific URL.
pub async fn is_ollama_available_at(url: &str) -> bool {
    match OllamaClient::new(url) {
        Ok(client) => client.health_check().await,
        Err(_) => false,
    }
}

/// Get an Ollama client for testing, or skip test if not available.
///
/// # Returns
/// `Some(client)` if Ollama is available, `None` otherwise
pub async fn get_test_ollama_client() -> Option<Arc<dyn LLMClient>> {
    if is_ollama_available().await {
        if let Ok(client) = OllamaClient::new("http://localhost:11434") {
            return Some(Arc::new(client));
        }
    }
    None
}

/// Check if GPU is available on the current system.
pub async fn is_gpu_available() -> bool {
    use lattice::infrastructure::llm::detect_capabilities;
    detect_capabilities().await.has_gpu()
}

// ============================================================================
// Test Model Paths
// ============================================================================

/// Get the path to test models directory.
///
/// Returns None if directory doesn't exist.
pub fn get_test_models_dir() -> Option<PathBuf> {
    let cache_dir = dirs::cache_dir()?;
    let models_dir = cache_dir.join("recall_test_models");

    if models_dir.exists() {
        Some(models_dir)
    } else {
        None
    }
}

/// Get path to a specific test model.
///
/// # Arguments
/// * `model_name` - Name of the model (e.g., "tinyllama-1.1b")
///
/// # Returns
/// Path to the model file if it exists
pub fn get_test_model_path(model_name: &str) -> Option<PathBuf> {
    let models_dir = get_test_models_dir()?;
    let model_path = models_dir.join(format!("{}.gguf", model_name));

    if model_path.exists() {
        Some(model_path)
    } else {
        None
    }
}

/// Check if a small test model is available for offline testing.
///
/// Looks for TinyLlama or similar small models.
pub fn has_small_test_model() -> bool {
    get_test_model_path("tinyllama-1.1b").is_some() || get_test_model_path("tinyllama-1b").is_some()
}

// ============================================================================
// Test Assertions
// ============================================================================

/// Assert that a system capabilities struct has valid values.
pub fn assert_valid_capabilities(caps: &SystemCapabilities) {
    assert!(caps.total_ram_gb > 0.0, "Total RAM should be positive");
    assert!(caps.cpu_cores > 0, "Should have at least one CPU core");
    assert!(
        caps.cpu_threads >= caps.cpu_cores,
        "Threads should be >= cores"
    );
}

/// Assert that a recommendation score is valid.
pub fn assert_valid_score(score: f64) {
    assert!(
        score >= 0.0 && score <= 1.0,
        "Score should be in [0, 1] range, got {}",
        score
    );
}

// ============================================================================
// Test Fixtures
// ============================================================================

/// Common test prompts
pub mod prompts {
    pub const SIMPLE: &str = "Hello";
    pub const SHORT_QUESTION: &str = "What is 2+2?";
    pub const MEDIUM: &str = "Explain quantum computing in simple terms.";
    pub const LONG: &str = "Write a detailed essay about the history of artificial intelligence.";
    pub const COUNT_TO_5: &str = "Count to 5";
    pub const SAY_HELLO: &str = "Say hello and nothing else";
}

/// Common system prompts
pub mod system_prompts {
    pub const HELPFUL_ASSISTANT: &str = "You are a helpful assistant.";
    pub const CONCISE: &str = "You are a helpful assistant. Keep responses brief.";
    pub const MATH_TEACHER: &str = "You are a math teacher. Explain concepts clearly.";
    pub const CODE_EXPERT: &str = "You are a code expert. Provide code examples.";
}

/// Expected model names for testing
pub mod model_names {
    pub const TINY_LLAMA: &str = "tinyllama";
    pub const LLAMA_2: &str = "llama2";
    pub const LLAMA_3: &str = "llama3";
    pub const MISTRAL: &str = "mistral";
}

// ============================================================================
// Performance Helpers
// ============================================================================

/// Performance thresholds for tests
pub mod thresholds {
    use std::time::Duration;

    /// Maximum time for cold start (first request)
    pub const COLD_START_MAX: Duration = Duration::from_secs(30);

    /// Maximum time for warm request
    pub const WARM_REQUEST_MAX: Duration = Duration::from_secs(10);

    /// Maximum time for recommendation engine
    pub const RECOMMENDATION_MAX: Duration = Duration::from_millis(100);

    /// Maximum time for capability detection
    pub const CAPABILITY_DETECTION_MAX: Duration = Duration::from_millis(500);
}

// ============================================================================
// Test Cleanup
// ============================================================================

/// Clean up test artifacts
pub fn cleanup_test_artifacts() {
    // Clean up any temporary test files
    if let Some(test_dir) = get_test_models_dir() {
        let _ = std::fs::remove_dir_all(test_dir);
    }
}

// ============================================================================
// Conditional Test Skipping
// ============================================================================

/// Skip test if Ollama is not available
#[macro_export]
macro_rules! skip_if_no_ollama {
    () => {
        if !$crate::helpers::llm_helpers::is_ollama_available().await {
            eprintln!("⚠ Skipping test: Ollama not available");
            return;
        }
    };
}

/// Skip test if no GPU is available
#[macro_export]
macro_rules! skip_if_no_gpu {
    () => {
        if !$crate::helpers::llm_helpers::is_gpu_available() {
            eprintln!("⚠ Skipping test: GPU not available");
            return;
        }
    };
}

/// Skip test if no test model is available
#[macro_export]
macro_rules! skip_if_no_test_model {
    () => {
        if !$crate::helpers::llm_helpers::has_small_test_model() {
            eprintln!("⚠ Skipping test: No test model available");
            return;
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_mock_capabilities() {
        let caps = create_mock_capabilities(16.0, true);
        assert_valid_capabilities(&caps);
        assert!(caps.has_gpu());
    }

    #[test]
    fn test_low_end_system() {
        let caps = create_low_end_system();
        assert_eq!(caps.total_ram_gb, 4.0);
        assert!(!caps.has_gpu());
    }

    #[test]
    fn test_mid_range_system() {
        let caps = create_mid_range_system();
        assert_eq!(caps.total_ram_gb, 16.0);
        assert!(caps.has_gpu());
    }

    #[test]
    fn test_high_end_system() {
        let caps = create_high_end_system();
        assert_eq!(caps.total_ram_gb, 32.0);
        assert!(caps.has_gpu());

        if let Some(gpu) = &caps.gpu {
            assert_eq!(gpu.vram_gb, Some(24.0));
        }
    }

    #[test]
    fn test_assert_valid_capabilities() {
        let caps = create_mock_capabilities(16.0, false);
        assert_valid_capabilities(&caps);
    }

    #[test]
    fn test_assert_valid_score() {
        assert_valid_score(0.0);
        assert_valid_score(0.5);
        assert_valid_score(1.0);
    }

    #[test]
    #[should_panic]
    fn test_assert_invalid_score_negative() {
        assert_valid_score(-0.1);
    }

    #[test]
    #[should_panic]
    fn test_assert_invalid_score_too_high() {
        assert_valid_score(1.1);
    }
}
