//! Core inference engine using mistral.rs v0.5.0+.
//!
//! Pure Rust implementation with Metal backend support.
//! Fully async with native tokio integration.

use async_stream::stream;
use lazy_regex::regex;
use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;
use tokio::time::{timeout, Duration};
use tokio_stream::{Stream, StreamExt};

use mistralrs::{Model, RequestBuilder, SamplingParams, TextMessageRole};

use super::config::InferenceConfig;
use super::loader::ModelLoader;
use crate::llm::models::ModelFormat;
use crate::llm::traits::GenerationConfig;
use crate::llm::types::LLMError;

/// Regex pattern for matching hex escape sequences (compile-time verified).
///
/// This regex matches patterns like `<0x0A>`, `<0x20>`, etc.
/// Note: lazy_regex::regex! validates at compile-time, eliminating runtime panics
fn hex_pattern() -> &'static regex::Regex {
    regex!(r"<0x([0-9A-Fa-f]{2})>")
}

/// Clean LLM output by converting hex escape sequences to actual characters.
///
/// Fixes BUG 2: Some models output `<0x0A>` instead of newlines.
/// This function converts all `<0xNN>` patterns to their actual byte values.
///
/// # Arguments
/// * `text` - Raw LLM output text
///
/// # Returns
/// Cleaned text with hex sequences converted to actual characters
fn clean_llm_output(text: &str) -> String {
    hex_pattern()
        .replace_all(text, |caps: &regex::Captures| {
            // Parse hex value
            if let Ok(byte_val) = u8::from_str_radix(&caps[1], 16) {
                // Convert byte to char - handle newlines specially
                if byte_val == 0x0A {
                    "\n".to_string()
                } else if byte_val.is_ascii() && !byte_val.is_ascii_control() {
                    (byte_val as char).to_string()
                } else {
                    // For non-printable characters, keep original
                    caps[0].to_string()
                }
            } else {
                // If parsing fails, keep original
                caps[0].to_string()
            }
        })
        .to_string()
}

/// Parse a context message string into (role, content).
///
/// Expected formats:
/// - "User: <content>"
/// - "Assistant: <content>"
/// - "System: <content>"
///
/// Returns None if message doesn't match expected format.
pub fn parse_context_message(message: &str) -> Option<(TextMessageRole, String)> {
    let message = message.trim();

    // Try User role (case-insensitive)
    if let Some(content) = message
        .strip_prefix("User:")
        .or_else(|| message.strip_prefix("user:"))
        .or_else(|| message.strip_prefix("USER:"))
    {
        let content = content.trim();
        if !content.is_empty() {
            return Some((TextMessageRole::User, content.to_string()));
        }
    }

    // Try Assistant role (case-insensitive)
    if let Some(content) = message
        .strip_prefix("Assistant:")
        .or_else(|| message.strip_prefix("assistant:"))
        .or_else(|| message.strip_prefix("ASSISTANT:"))
    {
        let content = content.trim();
        if !content.is_empty() {
            return Some((TextMessageRole::Assistant, content.to_string()));
        }
    }

    // Try System role (case-insensitive)
    if let Some(content) = message
        .strip_prefix("System:")
        .or_else(|| message.strip_prefix("system:"))
        .or_else(|| message.strip_prefix("SYSTEM:"))
    {
        let content = content.trim();
        if !content.is_empty() {
            return Some((TextMessageRole::System, content.to_string()));
        }
    }

    // No recognized prefix
    None
}

/// Inference engine for local LLM execution using mistral.rs.
pub struct InferenceEngine {
    model: Arc<Model>,
    config: InferenceConfig,
}

impl InferenceEngine {
    /// Create inference engine from a model on disk.
    ///
    /// # Arguments
    /// * `model_path` - For `Gguf`, a `.gguf` file. For `Safetensors`,
    ///   the HF directory containing `config.json` + shards.
    /// * `format` - Explicit format selector. Callers determine this
    ///   from their own metadata; the loader does not sniff.
    /// * `config` - Inference configuration
    ///
    /// # Errors
    /// Returns `LLMError::ModelNotLoaded` if path doesn't exist
    /// Returns `LLMError::InsufficientMemory` if a safetensors model
    ///   would exceed available RAM
    /// Returns `LLMError::GenerationFailed` on mistralrs builder errors
    pub async fn from_path<P: AsRef<Path>>(
        model_path: P,
        format: ModelFormat,
        config: InferenceConfig,
    ) -> Result<Self, LLMError> {
        let loader = ModelLoader::new(config.clone());
        let model = loader.load(model_path, format).await?;

        Ok(Self {
            model: Arc::new(model),
            config,
        })
    }

    /// Convert GenerationConfig to mistral.rs RequestBuilder.
    ///
    /// This is an adapter layer that converts our generic GenerationConfig
    /// to mistral.rs-specific API calls. Other LLM backends (llama.cpp, etc.)
    /// would have their own conversion logic.
    fn create_request_builder(&self, gen_config: &GenerationConfig) -> RequestBuilder {
        // CRITICAL FIX: mistral.rs requires explicit sampling parameters to be set.
        // Without them, the request builder uses SamplingParams::deterministic() which
        // has max_len: None, causing "weight is negative" errors during sampling.
        //
        // The bug was that RequestBuilder::new() creates a deterministic sampler with
        // NO max_len, which fails during mistral.rs's internal sampling weight calculations.
        //
        // Root cause: When max_len is None, mistral.rs's sampler tries to calculate
        // sampling weights without a length constraint, leading to invalid/negative weights.
        //
        // Solution: Create SamplingParams with explicit max_len and our generation config.
        let sampling_params = SamplingParams {
            temperature: Some(gen_config.temperature as f64),
            top_k: Some(gen_config.top_k as usize),
            top_p: Some(gen_config.top_p as f64),
            min_p: None,
            top_n_logprobs: 0,
            frequency_penalty: Some(gen_config.repeat_penalty),
            presence_penalty: None,
            repetition_penalty: None,
            stop_toks: None,
            max_len: Some(gen_config.max_tokens), // REQUIRED - prevents weight calculation errors
            logits_bias: None,
            n_choices: 1,
            dry_params: None,
        };

        RequestBuilder::new().set_sampling(sampling_params)
    }

    /// Generate text completion (non-streaming).
    ///
    /// # Arguments
    /// * `prompt` - User prompt
    /// * `system` - Optional system message
    /// * `gen_config` - Generation configuration
    ///
    /// # Errors
    /// Returns `LLMError::GenerationFailed` on inference errors
    pub async fn generate(
        &self,
        prompt: &str,
        system: Option<&str>,
        gen_config: &GenerationConfig,
    ) -> Result<String, LLMError> {
        let mut request = self.create_request_builder(gen_config);

        // Add messages to request
        if let Some(sys) = system {
            request = request.add_message(TextMessageRole::System, sys);
        }
        request = request.add_message(TextMessageRole::User, prompt);

        // Send request with configured timeout
        let response = timeout(
            crate::shared::constants::LLM_INFERENCE_TIMEOUT,
            self.model.send_chat_request(request),
        )
        .await
        .map_err(|_| {
            LLMError::GenerationFailed(
                "Model inference timed out. \
                 The model may be incompatible or corrupted. \
                 Try using a different model or check the model file."
                    .to_string(),
            )
        })?
        .map_err(|e| {
            LLMError::GenerationFailed(format!(
                "Model inference failed: {}. \
                     This may indicate an incompatible model or internal error. \
                     Try using a different model or check the model file.",
                e
            ))
        })?;

        // Extract generated text
        let text = response
            .choices
            .first()
            .and_then(|choice| choice.message.content.as_ref())
            .ok_or_else(|| LLMError::GenerationFailed("No response generated".to_string()))?
            .clone();

        // Clean output before returning (BUG 2 fix)
        Ok(clean_llm_output(&text))
    }

    /// Generate text from a sequence of role-structured messages.
    ///
    /// This method constructs the prompt with proper role alternation
    /// required by chat templates (e.g., User/Assistant/User pattern).
    ///
    /// # Arguments
    /// * `messages` - Sequence of (role, content) pairs representing conversation
    /// * `gen_config` - Generation configuration
    ///
    /// # Returns
    /// Generated text or error if generation fails
    pub async fn generate_with_messages(
        &self,
        messages: &[(TextMessageRole, String)],
        gen_config: &GenerationConfig,
    ) -> Result<String, LLMError> {
        if messages.is_empty() {
            return Err(LLMError::InvalidConfig(
                "Messages cannot be empty".to_string(),
            ));
        }

        let mut request = self.create_request_builder(gen_config);

        // Add all messages in sequence
        for (role, content) in messages {
            request = request.add_message(role.clone(), content);
        }

        // Send request with 60-second timeout
        let response = timeout(
            Duration::from_secs(60),
            self.model.send_chat_request(request),
        )
        .await
        .map_err(|_| {
            LLMError::GenerationFailed(
                "Model inference timed out after 60 seconds. \
                 The model may be incompatible or corrupted. \
                 Try using a different model or check the model file."
                    .to_string(),
            )
        })?
        .map_err(|e| {
            LLMError::GenerationFailed(format!(
                "Model inference failed: {}. \
                     This may indicate an incompatible model or internal error. \
                     Try using a different model or check the model file.",
                e
            ))
        })?;

        // Extract generated text
        let text = response
            .choices
            .first()
            .and_then(|choice| choice.message.content.as_ref())
            .ok_or_else(|| LLMError::GenerationFailed("No response generated".to_string()))?
            .clone();

        // Clean output before returning (apply hex token fix)
        Ok(clean_llm_output(&text))
    }

    /// Generate text with streaming from a sequence of role-structured messages.
    pub async fn generate_stream_with_messages(
        &self,
        messages: &[(TextMessageRole, String)],
        gen_config: &GenerationConfig,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String, LLMError>> + Send + '_>>, LLMError> {
        if messages.is_empty() {
            return Err(LLMError::InvalidConfig(
                "Messages cannot be empty".to_string(),
            ));
        }

        let mut request = self.create_request_builder(gen_config);
        for (role, content) in messages {
            request = request.add_message(role.clone(), content);
        }

        let chunk_stream = timeout(
            Duration::from_secs(60),
            self.model.stream_chat_request(request),
        )
        .await
        .map_err(|_| {
            LLMError::GenerationFailed(
                "Model streaming initialization timed out after 60 seconds. \
                 The model may be incompatible or corrupted. \
                 Try using a different model or check the model file."
                    .to_string(),
            )
        })?
        .map_err(|e| {
            LLMError::GenerationFailed(format!(
                "Model streaming failed: {}. \
                     This may indicate an incompatible model or internal error. \
                     Try using a different model or check the model file.",
                e
            ))
        })?;

        Ok(Box::pin(stream! {
            tokio::pin!(chunk_stream);

            while let Some(chunk) = chunk_stream.next().await {
                if let mistralrs::Response::Chunk(chunk_response) = chunk {
                    if let Some(choice) = chunk_response.choices.first() {
                        if let Some(delta) = &choice.delta.content {
                            yield Ok(clean_llm_output(delta));
                        }
                    }
                }
            }
        }))
    }

    /// Generate text with streaming.
    ///
    /// # Arguments
    /// * `prompt` - User prompt
    /// * `system` - Optional system message
    /// * `gen_config` - Generation configuration
    ///
    /// # Returns
    /// Async stream of text chunks
    pub async fn generate_stream(
        &self,
        prompt: &str,
        system: Option<&str>,
        gen_config: &GenerationConfig,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String, LLMError>> + Send + '_>>, LLMError> {
        let mut request = self.create_request_builder(gen_config);

        if let Some(sys) = system {
            request = request.add_message(TextMessageRole::System, sys);
        }
        request = request.add_message(TextMessageRole::User, prompt);

        // Get streaming response with 60-second timeout
        let chunk_stream = timeout(
            Duration::from_secs(60),
            self.model.stream_chat_request(request),
        )
        .await
        .map_err(|_| {
            LLMError::GenerationFailed(
                "Model streaming initialization timed out after 60 seconds. \
                 The model may be incompatible or corrupted. \
                 Try using a different model or check the model file."
                    .to_string(),
            )
        })?
        .map_err(|e| {
            LLMError::GenerationFailed(format!(
                "Model streaming failed: {}. \
                     This may indicate an incompatible model or internal error. \
                     Try using a different model or check the model file.",
                e
            ))
        })?;

        // Transform chunk stream to string stream
        Ok(Box::pin(stream! {
            tokio::pin!(chunk_stream);

            while let Some(chunk) = chunk_stream.next().await {
                // chunk is a Response enum - match on Chunk variant
                if let mistralrs::Response::Chunk(chunk_response) = chunk {
                    // Extract delta content from chunk
                    if let Some(choice) = chunk_response.choices.first() {
                        if let Some(delta) = &choice.delta.content {
                            // Clean output before yielding (BUG 2 fix)
                            yield Ok(clean_llm_output(delta));
                        }
                    }
                }
            }
        }))
    }

    /// Check if engine is ready for inference.
    pub fn is_ready(&self) -> bool {
        true // Model is ready once loaded
    }
}
