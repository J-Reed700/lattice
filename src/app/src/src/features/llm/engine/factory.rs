//! LLM Factory for creating LLM clients with auto-detection and fallback.
//!
//! This factory implements the Factory Pattern to create LLM clients based on
//! configuration, with graceful fallback when resources are unavailable.
//!
//! # Purpose
//!
//! - Abstract LLM client creation logic
//! - Support multiple LLM backends (local, Ollama, mock)
//! - Auto-detect available resources (local models, Ollama service)
//! - Graceful degradation when resources unavailable
//!
//! # Design
//!
//! The factory tries to create the requested LLM client, falling back to a mock
//! implementation if the requested backend is unavailable. This ensures the
//! application can continue to function even when LLM services are not available.
//!
//! # Example
//!
//! ```rust
//! use crate::infrastructure::llm::factory::{LLMConfig, create_llm};
//! use std::path::PathBuf;
//!
//! // Try to create local LLM, fallback to mock if unavailable
//! let config = LLMConfig::Local {
//!     model_path: PathBuf::from("models/mistral-7b.gguf"),
//!     n_gpu_layers: 0,  // CPU only (safe default)
//!     generation_config: crate::llm::GenerationConfig::default(),
//! };
//!
//! let llm = create_llm(config).await?;
//! ```

use async_trait::async_trait;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tracing::{error, info, warn};

use crate::application::ports::LLMPort;
use crate::llm::models::{ModelFamily, ModelFormat, Quantization};
use crate::llm::traits::LLMClient;
use crate::llm::types::LLMError;
use crate::llm::{GenerationConfig, InferenceConfig, LocalLLMClient, ModelInfo, OllamaClient};
use crate::shared::error::AppError;
use crate::shared::result::Result;

// ============================================================================
// LLM Configuration
// ============================================================================

/// Configuration for LLM client creation.
///
/// Supports multiple backend types with specific configuration for each.
#[derive(Debug, Clone)]
pub enum LLMConfig {
    /// Local LLM using mistral.rs inference
    Local {
        /// Path to GGUF model file
        model_path: PathBuf,
        /// Number of GPU layers to offload (-1 = all, 0 = CPU only, 1-N = specific layers)
        n_gpu_layers: i32,
        /// Generation configuration (temperature, top-p, etc.)
        generation_config: GenerationConfig,
    },
    /// Ollama HTTP API
    Ollama {
        /// Ollama server endpoint (e.g., "http://localhost:11434")
        endpoint: String,
        /// Model name (e.g., "llama3.1:8b", "mistral")
        model: String,
        /// Optional auth header to include with requests
        auth_header: Option<(String, String)>,
        /// Generation configuration (temperature, top-p, etc.)
        generation_config: GenerationConfig,
    },
}

// ============================================================================
// Factory Function
// ============================================================================

/// Create an LLM client from configuration.
///
/// This factory function attempts to create the requested LLM client,
/// returning an error if the backend is unavailable.
///
/// # Arguments
///
/// * `config` - LLM configuration specifying backend and parameters
///
/// # Returns
///
/// `Ok(Arc<dyn LLMPort>)` if client created successfully, `Err` otherwise
///
/// # Errors
///
/// - `LLMError::ModelNotFound` if local model file doesn't exist
/// - `LLMError::ClientUnavailable` if Ollama service is unreachable
/// - `LLMError::LoadFailed` if model fails to load
///
/// # Example
///
/// ```rust
/// let config = LLMConfig::Local {
///     model_path: PathBuf::from("models/mistral-7b.gguf"),
///     n_gpu_layers: 0,  // CPU only
///     generation_config: crate::llm::GenerationConfig::default(),
/// };
///
/// match create_llm(config).await {
///     Ok(llm) => println!("LLM ready: {}", llm.model_name()),
///     Err(e) => eprintln!("Failed to create LLM: {}", e),
/// }
/// ```
pub async fn create_llm(config: LLMConfig) -> std::result::Result<Arc<dyn LLMPort>, LLMError> {
    match config {
        LLMConfig::Local {
            model_path,
            n_gpu_layers,
            generation_config,
        } => create_local_llm(&model_path, n_gpu_layers, generation_config).await,
        LLMConfig::Ollama {
            endpoint,
            model,
            auth_header,
            generation_config,
        } => create_ollama_llm(&endpoint, &model, auth_header, generation_config).await,
    }
}

/// Create an LLM client with graceful fallback to mock.
///
/// This function wraps `create_llm` and provides automatic fallback to a
/// mock LLM client if the requested backend is unavailable. This ensures
/// the application can continue to function even when LLM services are down.
///
/// # Arguments
///
/// * `config` - LLM configuration specifying backend and parameters
///
/// # Returns
///
/// Always returns `Ok(Arc<dyn LLMPort>)`, falling back to mock if needed
///
/// # Example
///
/// ```rust
/// let config = LLMConfig::Local {
///     model_path: PathBuf::from("models/mistral-7b.gguf"),
///     n_gpu_layers: 0,  // CPU only
///     generation_config: crate::llm::GenerationConfig::default(),
/// };
///
/// // Always succeeds, even if model doesn't exist
/// let llm = create_llm_with_fallback(config).await;
/// ```
pub async fn create_llm_with_fallback(config: LLMConfig) -> Arc<dyn LLMPort> {
    match create_llm(config).await {
        Ok(llm) => {
            info!("LLM client created successfully: {}", llm.model_name());
            llm
        }
        Err(e) => {
            warn!("Failed to create LLM client, using mock: {}", e);
            Arc::new(MockLLMPort::new()) as Arc<dyn LLMPort>
        }
    }
}

// ============================================================================
// Backend-Specific Creation Functions
// ============================================================================

/// Create a local LLM client using mistral.rs.
async fn create_local_llm(
    model_path: &Path,
    n_gpu_layers: i32,
    generation_config: GenerationConfig,
) -> std::result::Result<Arc<dyn LLMPort>, LLMError> {
    info!("Creating local LLM client from: {}", model_path.display());

    // Validate model file exists
    if !model_path.exists() {
        error!("Model file not found: {}", model_path.display());
        return Err(LLMError::Other(format!(
            "Model file not found: {}",
            model_path.display()
        )));
    }

    // Infer model info from path
    let model_info = infer_model_info(model_path);
    info!(
        "Detected model: {} ({:?})",
        model_info.name, model_info.family
    );

    // Configure inference with specified GPU layers
    let inference_config = if n_gpu_layers == 0 {
        InferenceConfig::cpu_only()
    } else {
        InferenceConfig::with_gpu(n_gpu_layers)
    };

    // Create client
    let client = LocalLLMClient::new(model_path, model_info, inference_config, generation_config)
        .await
        .map_err(|e| {
            let mapped_error = map_local_model_load_error(e, model_path);
            error!("Failed to create local LLM client: {}", mapped_error);
            mapped_error
        })?;

    info!("Local LLM client created successfully");

    // Wrap in Arc and cast to LLMPort
    let client_arc = Arc::new(client);
    Ok(Arc::new(LocalLLMPortAdapter::new(client_arc)) as Arc<dyn LLMPort>)
}

/// Create an Ollama LLM client.
pub async fn create_ollama_llm(
    endpoint: &str,
    model: &str,
    auth_header: Option<(String, String)>,
    generation_config: GenerationConfig,
) -> std::result::Result<Arc<dyn LLMPort>, LLMError> {
    info!("Creating Ollama LLM client: {} @ {}", model, endpoint);

    // Create Ollama client
    let mut client = OllamaClient::with_model_and_timeouts_and_header(
        endpoint,
        model,
        Duration::from_secs(120),
        Duration::from_secs(300),
        auth_header,
    )
    .map_err(|e| {
        error!("Failed to create Ollama client: {}", e);
        match e {
            AppError::InvalidInput(msg) => LLMError::InvalidConfig(msg),
            AppError::Network(msg) => LLMError::ClientUnavailable(msg),
            _ => LLMError::ClientUnavailable(e.to_string()),
        }
    })?;

    *client.generation_config_mut() = generation_config;

    // Verify Ollama is reachable
    if !client.health_check().await {
        error!("Ollama health check failed: {}", endpoint);
        return Err(LLMError::ClientUnavailable(format!(
            "Ollama not reachable at {}",
            endpoint
        )));
    }

    info!("Ollama LLM client created successfully");
    Ok(Arc::new(client) as Arc<dyn LLMPort>)
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Infer model information from file path.
///
/// This is a simple heuristic that extracts model metadata from the filename.
/// For production use, consider storing model metadata separately.
fn infer_model_info(path: &Path) -> ModelInfo {
    let filename = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown");

    let lowercase_filename = filename.to_ascii_lowercase();
    let has_token = |needle: &str| {
        lowercase_filename
            .split(|c: char| !c.is_ascii_alphanumeric())
            .any(|token| token == needle)
    };

    // Extract model family from filename
    let family = if has_token("llama") {
        ModelFamily::Llama
    } else if has_token("mistral") || has_token("mixtral") {
        ModelFamily::Mistral
    } else if has_token("gemma") {
        ModelFamily::Gemma
    } else if has_token("phi") {
        ModelFamily::Phi
    } else {
        ModelFamily::Other
    };

    // Extract quantization from filename
    let quantization = if filename.contains("Q4") || filename.contains("q4") {
        Some(Quantization::Q4)
    } else if filename.contains("Q5") || filename.contains("q5") {
        Some(Quantization::Q5)
    } else if filename.contains("Q8") || filename.contains("q8") {
        Some(Quantization::Q8)
    } else if filename.contains("F16") || filename.contains("f16") {
        Some(Quantization::F16)
    } else if filename.contains("F32") || filename.contains("f32") {
        Some(Quantization::F32)
    } else {
        None
    };

    // Detect on-disk format. A directory with config.json + safetensors
    // shards is the HF safetensors layout; anything else is treated as
    // GGUF (single-file blob).
    let format = detect_model_format(path);

    // Estimate size. For GGUF, the file size; for safetensors, walk the
    // directory and sum every .safetensors shard. Defaults to 4 GB if
    // neither path resolves cleanly.
    let size_mb = match format {
        ModelFormat::Gguf => std::fs::metadata(path)
            .map(|m| m.len() / 1_048_576)
            .unwrap_or(4000),
        ModelFormat::Safetensors => sum_safetensors_dir_size_mb(path).unwrap_or(8000),
    };

    ModelInfo {
        name: filename.to_string(),
        family,
        quantization,
        size_mb,
        format,
    }
}

/// Decide whether a model path on disk is GGUF or safetensors.
///
/// Rules:
/// - File ending in `.gguf` (case-insensitive) → `Gguf`
/// - Directory containing `config.json` AND at least one `*.safetensors`
///   shard → `Safetensors`
/// - Anything else → `Gguf` (the conservative default; downstream
///   loader will surface a clean error if it's neither)
///
/// Public so call sites that have a `DownloadedModel` (which doesn't
/// yet carry an explicit `format` field — schema migration follow-up)
/// can resolve the format right before handing the path to
/// `InferenceEngine::from_path`. Once `DownloadedModel.format` lands,
/// most callers will read the field directly and this helper falls
/// back to detecting at the catalog/import boundary only.
pub fn detect_model_format(path: &Path) -> ModelFormat {
    if path.is_file() {
        let lower = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase());
        if lower.as_deref() == Some("gguf") {
            return ModelFormat::Gguf;
        }
        // Files that aren't .gguf fall through to GGUF default; the
        // loader's pre-flight magic-bytes check will reject anything
        // that isn't actually GGUF with a clean error.
        return ModelFormat::Gguf;
    }

    if path.is_dir() {
        let has_config = path.join("config.json").is_file();
        let has_safetensors = std::fs::read_dir(path)
            .map(|entries| {
                entries.flatten().any(|e| {
                    e.path()
                        .extension()
                        .and_then(|x| x.to_str())
                        .is_some_and(|x| x.eq_ignore_ascii_case("safetensors"))
                })
            })
            .unwrap_or(false);
        if has_config && has_safetensors {
            return ModelFormat::Safetensors;
        }
    }

    ModelFormat::Gguf
}

/// Sum the size of every `.safetensors` shard in a HF model directory.
/// Returns size in MiB. Used for the memory pre-flight check before
/// loading. Errors during traversal collapse to `None` so the caller
/// can fall back to a conservative default.
fn sum_safetensors_dir_size_mb(dir: &Path) -> Option<u64> {
    let entries = std::fs::read_dir(dir).ok()?;
    let mut total: u64 = 0;
    for entry in entries.flatten() {
        let path = entry.path();
        let is_shard = path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("safetensors"));
        if is_shard {
            if let Ok(meta) = std::fs::metadata(&path) {
                total = total.saturating_add(meta.len());
            }
        }
    }
    if total == 0 {
        None
    } else {
        Some(total / 1_048_576)
    }
}

/// Map low-level model load failures to user-friendly compatibility messages.
fn map_local_model_load_error(error: LLMError, model_path: &Path) -> LLMError {
    let error_message = error.to_string();

    if error_message.contains("Cannot find tensor info for") {
        let model_name = model_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("selected model");

        return LLMError::Other(format!(
            "Model '{}' is incompatible with the current GGUF loader (missing tensor metadata). \
             This GGUF variant likely uses tensor names/layout unsupported by this build. \
             Try a different GGUF for this model family or update to a newer mistral.rs integration. \
             Original error: {}",
            model_name, error_message
        ));
    }

    error
}

// ============================================================================
// Mock LLM Port Implementation
// ============================================================================

/// Mock LLM port for testing and fallback.
///
/// Returns fixed responses without actual LLM inference.
pub struct MockLLMPort {
    model_name: String,
}

impl Default for MockLLMPort {
    fn default() -> Self {
        Self::new()
    }
}

impl MockLLMPort {
    pub fn new() -> Self {
        Self {
            model_name: "mock-llm".to_string(),
        }
    }
}

#[async_trait]
impl LLMPort for MockLLMPort {
    async fn generate(
        &self,
        prompt: &str,
        context: &[String],
        _images: Option<Vec<String>>,
    ) -> Result<String> {
        warn!("Using mock LLM - returning intelligent response");

        // Generate an intelligent mock response based on the prompt
        let response = if !context.is_empty() {
            // If context is provided, acknowledge it
            format!(
                "Based on the provided context, I can help answer your question about: {}. \
                However, I'm currently running in mock mode as no LLM backend is available. \
                To enable full AI capabilities, please either:\n\n\
                1. Start Ollama with a model (e.g., `ollama run llama3.1:8b`), or\n\
                2. Download a local model from Settings → Models\n\n\
                Your question: {}\n\nContext references: {} document(s) available",
                prompt,
                prompt,
                context.len()
            )
        } else {
            // Provide contextual responses based on common queries
            let prompt_lower = prompt.to_lowercase();

            if prompt_lower.contains("hello")
                || prompt_lower.contains("hi ")
                || prompt_lower.contains("hey")
            {
                "Hello! I'm Lattice's AI assistant. I'm currently running in mock mode because no LLM backend is available. \
                To enable full conversational capabilities, please start Ollama or download a local model from Settings → Models.".to_string()
            } else if prompt_lower.contains("help") || prompt_lower.contains("what can you do") {
                "I'm Lattice, your personal knowledge assistant. I can:\n\n\
                • Answer questions about your indexed documents\n\
                • Search through your knowledge base semantically\n\
                • Provide contextual information from your files\n\n\
                Note: I'm currently in mock mode. To unlock full AI capabilities, please configure an LLM backend (Ollama or local model).".to_string()
            } else if prompt_lower.contains("how are you") || prompt_lower.contains("how do you do")
            {
                "I'm functioning in mock mode! While I can't provide AI-powered responses right now, \
                I'm ready to help once you configure an LLM backend. You can:\n\n\
                1. Start Ollama: `ollama run llama3.1:8b`\n\
                2. Download a model from Settings → Models".to_string()
            } else {
                // Generic helpful response
                format!(
                    "I understand you're asking about: \"{}\"\n\n\
                    I'm currently running in mock mode and can't provide full AI responses. \
                    To enable intelligent question answering and document analysis, please:\n\n\
                    1. Start Ollama with a model (recommended: llama3.1:8b or mistral)\n\
                    2. Or download a local GGUF model from Settings → Models\n\n\
                    Once configured, I'll be able to help you explore your knowledge base with natural language!",
                    prompt
                )
            }
        };

        Ok(response)
    }

    async fn generate_streaming(
        &self,
        prompt: &str,
        context: &[String],
        _images: Option<Vec<String>>,
    ) -> Result<Box<dyn futures::stream::Stream<Item = Result<String>> + Send + Unpin + '_>> {
        warn!("Using mock LLM streaming - returning single chunk");

        // Return a simple stream with one chunk
        let response = self.generate(prompt, context, None).await?;
        let stream = futures::stream::once(async move { Ok(response) });

        Ok(Box::new(Box::pin(stream)))
    }

    fn model_name(&self) -> &str {
        &self.model_name
    }

    fn max_context_tokens(&self) -> usize {
        4096
    }

    fn count_tokens(&self, text: &str) -> usize {
        // Simple heuristic: 1 token ≈ 4 characters
        text.len().div_ceil(4)
    }

    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }

    fn provider_name(&self) -> &str {
        "mock"
    }
}

// ============================================================================
// LocalLLMClient Port Adapter
// ============================================================================

/// Adapter to convert LocalLLMClient (LLMClient trait) to LLMPort.
///
/// LocalLLMClient implements the infrastructure LLMClient trait,
/// but the application layer needs the LLMPort trait. This adapter
/// bridges the two.
struct LocalLLMPortAdapter {
    client: Arc<LocalLLMClient>,
}

impl LocalLLMPortAdapter {
    fn new(client: Arc<LocalLLMClient>) -> Self {
        Self { client }
    }
}

#[async_trait]
impl LLMPort for LocalLLMPortAdapter {
    async fn generate(
        &self,
        prompt: &str,
        context: &[String],
        _images: Option<Vec<String>>,
    ) -> Result<String> {
        use crate::infrastructure::llm::inference::engine::parse_context_message;
        use mistralrs::TextMessageRole;

        // DIAGNOSTIC: Log raw context array
        tracing::info!(
            context_count = context.len(),
            context_items = ?context,
            "LocalLLMPortAdapter::generate - RAW context received"
        );

        // Try parsing context messages as role-structured messages
        let parsed_messages: Vec<_> = context
            .iter()
            .filter_map(|msg| match parse_context_message(msg) {
                Some(parsed) => {
                    tracing::debug!(
                        original = msg,
                        role = ?parsed.0,
                        content_len = parsed.1.len(),
                        "Successfully parsed context message"
                    );
                    Some(parsed)
                }
                None => {
                    tracing::warn!(
                        message = msg,
                        "FAILED to parse context message - falling back"
                    );
                    None
                }
            })
            .collect();

        // Log if we had partial parsing failures
        if !parsed_messages.is_empty() && parsed_messages.len() < context.len() {
            tracing::warn!(
                parsed = parsed_messages.len(),
                total = context.len(),
                "Some context messages failed to parse - proceeding with parsed messages only"
            );
        }

        // If we successfully parsed context messages, use message-based generation
        if !parsed_messages.is_empty() {
            // Add the new user message
            let mut messages = parsed_messages;
            messages.push((TextMessageRole::User, prompt.to_string()));

            // DIAGNOSTIC: Log final message sequence before sending to LLM
            tracing::info!(
                message_count = messages.len(),
                message_roles = ?messages.iter().map(|(role, _)| format!("{:?}", role)).collect::<Vec<_>>(),
                "Using message-based generation with parsed context"
            );

            // Use message-based generation (preserves role alternation)
            self.client
                .generate_with_messages(&messages)
                .await
                .map_err(|e| {
                    crate::shared::error::AppError::Other(format!("LLM generation failed: {}", e))
                })
        } else {
            // Fallback: Build system message from context (legacy behavior)
            let system = if context.is_empty() {
                None
            } else {
                Some(format!(
                    "Use the following context to answer the question:\n\n{}",
                    context.join("\n\n")
                ))
            };

            // Call LocalLLMClient with traditional approach
            self.client
                .generate(prompt, system.as_deref(), None)
                .await
                .map_err(|e| {
                    crate::shared::error::AppError::Other(format!("LLM generation failed: {}", e))
                })
        }
    }

    async fn generate_streaming(
        &self,
        prompt: &str,
        context: &[String],
        _images: Option<Vec<String>>,
    ) -> Result<Box<dyn futures::stream::Stream<Item = Result<String>> + Send + Unpin + '_>> {
        use crate::infrastructure::llm::inference::engine::parse_context_message;
        use mistralrs::TextMessageRole;

        // Try parsing context as role-structured chat history.
        let parsed_messages: Vec<_> = context
            .iter()
            .filter_map(|msg| parse_context_message(msg))
            .collect();

        let stream = if !parsed_messages.is_empty() {
            let mut messages = parsed_messages;
            messages.push((TextMessageRole::User, prompt.to_string()));

            self.client
                .generate_stream_with_messages(&messages)
                .await
                .map_err(|e| {
                    crate::shared::error::AppError::Other(format!("LLM streaming failed: {}", e))
                })?
        } else {
            // Fallback to legacy flattened system context if parsing fails.
            let system = if context.is_empty() {
                None
            } else {
                Some(format!(
                    "Use the following context to answer the question:\n\n{}",
                    context.join("\n\n")
                ))
            };

            self.client
                .generate_stream(prompt, system.as_deref(), None)
                .await
                .map_err(|e| {
                    crate::shared::error::AppError::Other(format!("LLM streaming failed: {}", e))
                })?
        };

        // Convert LLMError to AppError
        use futures::StreamExt;
        let mapped_stream = stream.map(|result| {
            result
                .map_err(|e| crate::shared::error::AppError::Other(format!("Stream error: {}", e)))
        });

        Ok(Box::new(Box::pin(mapped_stream)))
    }

    fn model_name(&self) -> &str {
        self.client.model_name()
    }

    fn max_context_tokens(&self) -> usize {
        // Get from model info or default to 4K
        4096
    }

    fn count_tokens(&self, text: &str) -> usize {
        // Simple heuristic: 1 token ≈ 4 characters
        text.len().div_ceil(4)
    }

    async fn is_ready(&self) -> Result<bool> {
        Ok(self.client.health_check().await)
    }

    fn supports_tool_calling(&self) -> bool {
        false // Local models via mistral.rs don't currently support tool calling
    }

    fn provider_name(&self) -> &str {
        "local"
    }
}

// ============================================================================
// Auto-Detection Helpers
// ============================================================================

/// Find a local model in common locations.
///
/// Searches for GGUF model files in standard directories.
///
/// # Returns
///
/// Path to first found model, or None if no models found
pub fn find_local_model() -> Option<PathBuf> {
    let mut search_paths = vec![PathBuf::from("models"), PathBuf::from("../models")];

    // Add data directory if available
    if let Some(data_dir) = std::env::var_os("XDG_DATA_HOME")
        .or_else(|| std::env::var_os("APPDATA"))
        .map(PathBuf::from)
    {
        search_paths.push(data_dir.join("lattice/models"));
    }

    // Add home directory if available
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        search_paths.push(home.join(".lattice/models"));
    }

    for search_path in search_paths {
        if let Ok(entries) = std::fs::read_dir(&search_path) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("gguf") {
                    info!("Found local model: {}", path.display());
                    return Some(path);
                }
            }
        }
    }

    None
}

/// Check if Ollama is available at the default endpoint.
///
/// # Returns
///
/// `true` if Ollama is reachable, `false` otherwise
pub async fn is_ollama_available() -> bool {
    let endpoint = "http://localhost:11434";

    match OllamaClient::new(endpoint) {
        Ok(client) => client.health_check().await,
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
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

    // === ModelFormat detection ===

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
        std::fs::write(dir.path().join("model.safetensors"), &[0u8; 16]).unwrap();
        assert_eq!(detect_model_format(dir.path()), ModelFormat::Safetensors);
    }

    #[test]
    fn test_detect_format_safetensors_sharded() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("config.json"), "{}").unwrap();
        std::fs::write(dir.path().join("model-00001-of-00002.safetensors"), &[0u8; 16]).unwrap();
        std::fs::write(dir.path().join("model-00002-of-00002.safetensors"), &[0u8; 16]).unwrap();
        assert_eq!(detect_model_format(dir.path()), ModelFormat::Safetensors);
    }

    #[test]
    fn test_detect_format_directory_missing_config_falls_back_to_gguf() {
        // No config.json — not a valid HF safetensors layout.
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("model.safetensors"), &[0u8; 16]).unwrap();
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
        let original =
            LLMError::Other("Cannot find tensor info for blk.0.ffn_gate.weight".to_string());

        let mapped = map_local_model_load_error(original, &path);
        let message = mapped.to_string();

        assert!(message.contains("incompatible"));
        assert!(message.contains("dolphin-2.7-mixtral-8x7b.Q3_K_M.gguf"));
        assert!(message.contains("Cannot find tensor info for blk.0.ffn_gate.weight"));
    }

    #[tokio::test]

    async fn test_mock_llm_port() {
        let mock = MockLLMPort::new();

        let response = mock.generate("test prompt", &[], None).await.unwrap();
        // Mock now returns intelligent responses, check for key phrases
        assert!(response.contains("mock mode") || response.contains("asking about"));

        // Test with context
        let response_with_context = mock
            .generate("test", &["doc1".to_string()], None)
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
        };

        let result = create_llm(config).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_create_llm_with_fallback() {
        let config = LLMConfig::Local {
            model_path: PathBuf::from("/nonexistent/model.gguf"),
            n_gpu_layers: 0,
            generation_config: GenerationConfig::default(),
        };

        // Should always succeed with fallback
        let llm = create_llm_with_fallback(config).await;
        assert_eq!(llm.model_name(), "mock-llm");
    }
}
