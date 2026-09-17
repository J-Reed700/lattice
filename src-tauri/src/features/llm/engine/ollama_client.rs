//! Ollama API client implementation.
//!
//! Provides async HTTP client for interacting with Ollama's REST API.
//! Supports both non-streaming and streaming generation requests.
//!
//! # Timeout policy
//!
//! Every bound is per request; the shared client deliberately has none.
//! `ClientBuilder::timeout` is a *total* deadline that includes the response
//! body, so a client-wide value silently truncates any generation that takes
//! longer than it — exactly what a long answer does. Probes and non-streaming
//! generation therefore pass `.timeout(..)` themselves, while streaming
//! bounds the wait for response headers with `stream_timeout` and then bounds
//! silence between chunks. Do not reintroduce a client-wide timeout.

use crate::features::llm::engine::circuit_breaker::{
    CircuitBreaker, CircuitBreakerConfig, CircuitBreakerError,
};
use crate::features::llm::engine::traits::{GenerationConfig, LLMClient};
use crate::features::llm::engine::types::*;
use crate::shared::error::{AppError, Result};
use crate::shared::utils::reqwest_client_builder;
use async_trait::async_trait;
use once_cell::sync::Lazy;
use reqwest::{
    header::{HeaderMap, HeaderName, HeaderValue},
    Client, StatusCode,
};
use serde_json;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tokio::time::timeout;
use tokio_stream::Stream;
use tracing::{debug, error, info, warn};

const DEFAULT_OLLAMA_MAX_CONCURRENCY: usize = 3;

fn ollama_max_concurrency() -> usize {
    std::env::var("RECALL_OLLAMA_MAX_CONCURRENCY")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_OLLAMA_MAX_CONCURRENCY)
}

static OLLAMA_REQUEST_SEMAPHORE: Lazy<Arc<Semaphore>> =
    Lazy::new(|| Arc::new(Semaphore::new(ollama_max_concurrency())));

/// Custom error types for Ollama client.
#[derive(Debug, thiserror::Error)]
pub enum OllamaClientError {
    #[error("Connection error: {0}")]
    Connection(String),

    #[error("API error: {0}")]
    Api(String),

    #[error("Timeout error: {0}")]
    Timeout(String),

    #[error("Invalid request: {0}")]
    InvalidRequest(String),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

impl From<OllamaClientError> for AppError {
    fn from(err: OllamaClientError) -> Self {
        AppError::Network(err.to_string())
    }
}

impl From<reqwest::Error> for OllamaClientError {
    fn from(err: reqwest::Error) -> Self {
        if err.is_timeout() {
            OllamaClientError::Timeout(err.to_string())
        } else if err.is_connect() {
            OllamaClientError::Connection(err.to_string())
        } else {
            OllamaClientError::Api(err.to_string())
        }
    }
}

impl From<OllamaClientError> for LLMError {
    fn from(err: OllamaClientError) -> Self {
        match err {
            OllamaClientError::Connection(msg) => LLMError::ClientUnavailable(msg),
            OllamaClientError::Api(msg) => LLMError::GenerationFailed(msg),
            OllamaClientError::Timeout(_msg) => LLMError::Timeout,
            OllamaClientError::InvalidRequest(msg) => LLMError::InvalidConfig(msg),
            OllamaClientError::Serialization(e) => LLMError::Other(e.to_string()),
        }
    }
}

impl From<AppError> for LLMError {
    fn from(err: AppError) -> Self {
        match err {
            AppError::Network(msg) => LLMError::Network(msg),
            AppError::InvalidInput(msg) => LLMError::InvalidConfig(msg),
            _ => LLMError::Other(err.to_string()),
        }
    }
}

/// Async HTTP client for Ollama API.
///
/// Provides methods for:
/// - Health checks
/// - Listing available models
/// - Non-streaming text generation
/// - Streaming text generation
///
/// # Example
///
/// ```no_run
/// use lattice::features::llm::engine::{OllamaClient, OllamaGenerateRequest};
///
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let client = OllamaClient::new("http://localhost:11434")?;
///
///     // Check health
///     if client.health_check().await? {
///         println!("Ollama is running");
///     }
///
///     // Generate text
///     let request = OllamaGenerateRequest::new("llama2", "Why is the sky blue?");
///     let response = client.generate(request).await?;
///     println!("Response: {}", response.response);
///
///     Ok(())
/// }
/// ```
pub struct OllamaClient {
    base_url: String,
    model_name: String,
    client: Client,
    timeout: Duration,
    stream_timeout: Duration,
    config: GenerationConfig,
    circuit_breaker: CircuitBreaker,
}

impl OllamaClient {
    async fn acquire_request_permit(&self, operation: &str) -> Result<OwnedSemaphorePermit> {
        let permit = OLLAMA_REQUEST_SEMAPHORE
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| {
                AppError::ServiceNotAvailable(
                    "Ollama concurrency gate is shutting down".to_string(),
                )
            })?;
        debug!(
            operation = operation,
            available_permits = OLLAMA_REQUEST_SEMAPHORE.available_permits(),
            "Acquired Ollama request permit"
        );
        Ok(permit)
    }

    /// Create a new Ollama client with default timeouts.
    ///
    /// # Arguments
    ///
    /// * `base_url` - Base URL of the Ollama server (e.g., "http://localhost:11434")
    ///
    /// # Errors
    ///
    /// Returns error if HTTP client cannot be created.
    ///
    /// # Note
    ///
    /// This method is for backward compatibility. For trait usage, use `with_model()`.
    pub fn new(base_url: impl Into<String>) -> Result<Self> {
        Self::with_model(base_url, "llama3.1:8b")
    }

    /// Create a new Ollama client with a specific model.
    ///
    /// # Arguments
    ///
    /// * `base_url` - Base URL of the Ollama server (e.g., "http://localhost:11434")
    /// * `model_name` - Model name (e.g., "llama3.1:8b", "mistral")
    ///
    /// # Errors
    ///
    /// Returns error if HTTP client cannot be created.
    pub fn with_model(base_url: impl Into<String>, model_name: impl Into<String>) -> Result<Self> {
        Self::with_model_and_timeouts(
            base_url,
            model_name,
            Duration::from_secs(120),
            Duration::from_secs(300),
        )
    }

    /// Create a new Ollama client with custom timeouts.
    ///
    /// # Arguments
    ///
    /// * `base_url` - Base URL of the Ollama server
    /// * `timeout` - Timeout for non-streaming requests
    /// * `stream_timeout` - Timeout for streaming requests
    ///
    /// # Note
    ///
    /// This method is for backward compatibility. For trait usage, use `with_model_and_timeouts()`.
    pub fn with_timeouts(
        base_url: impl Into<String>,
        timeout: Duration,
        stream_timeout: Duration,
    ) -> Result<Self> {
        Self::with_model_and_timeouts(base_url, "llama3.1:8b", timeout, stream_timeout)
    }

    /// Create a new Ollama client with model and custom timeouts.
    ///
    /// # Arguments
    ///
    /// * `base_url` - Base URL of the Ollama server
    /// * `model_name` - Model name (e.g., "llama3.1:8b", "mistral")
    /// * `timeout` - Timeout for non-streaming requests
    /// * `stream_timeout` - Timeout for streaming requests
    pub fn with_model_and_timeouts(
        base_url: impl Into<String>,
        model_name: impl Into<String>,
        timeout: Duration,
        stream_timeout: Duration,
    ) -> Result<Self> {
        Self::with_model_and_timeouts_and_header(
            base_url,
            model_name,
            timeout,
            stream_timeout,
            None,
        )
    }

    /// Create a new Ollama client with model, custom timeouts, and an optional auth header.
    ///
    /// # Arguments
    ///
    /// * `base_url` - Base URL of the Ollama server
    /// * `model_name` - Model name (e.g., "llama3.1:8b", "mistral")
    /// * `timeout` - Timeout for non-streaming requests
    /// * `stream_timeout` - Timeout for streaming requests
    /// * `auth_header` - Optional header (name, value) to include with every request
    pub fn with_model_and_timeouts_and_header(
        base_url: impl Into<String>,
        model_name: impl Into<String>,
        timeout: Duration,
        stream_timeout: Duration,
        auth_header: Option<(String, String)>,
    ) -> Result<Self> {
        let mut builder = reqwest_client_builder();
        if let Some((name, value)) = auth_header {
            let header_name = HeaderName::from_bytes(name.trim().as_bytes()).map_err(|e| {
                AppError::InvalidInput(format!("Invalid Ollama header name: {}", e))
            })?;
            let header_value = HeaderValue::from_str(value.trim()).map_err(|e| {
                AppError::InvalidInput(format!("Invalid Ollama header value: {}", e))
            })?;
            let mut headers = HeaderMap::new();
            headers.insert(header_name, header_value);
            builder = builder.default_headers(headers);
        }

        let client = builder
            .build()
            .map_err(|e| AppError::Network(format!("Failed to create HTTP client: {}", e)))?;

        let model_name = model_name.into();
        if model_name.trim().is_empty() {
            return Err(AppError::InvalidInput(
                "Model name cannot be empty".to_string(),
            ));
        }

        let mut url = base_url.into();
        if url.ends_with('/') {
            url.pop();
        }

        Ok(Self {
            base_url: url,
            model_name,
            client,
            timeout,
            stream_timeout,
            config: GenerationConfig::default(),
            circuit_breaker: CircuitBreaker::new(CircuitBreakerConfig::default()),
        })
    }

    /// Check if an Ollama server is reachable.
    ///
    /// Probes `/api/tags`, an Ollama-only route, rather than the bare base
    /// URL: a llama.cpp server answers the base URL with its web UI and would
    /// otherwise pass as Ollama, only to 404 on the first generation call.
    ///
    /// # Returns
    ///
    /// Returns `Ok(true)` if server responds to health check, `Ok(false)` otherwise.
    pub async fn health_check_with_result(&self) -> Result<bool> {
        let url = format!("{}/api/tags", self.base_url);
        debug!("Performing Ollama health check at {}", url);

        match self.client.get(&url).timeout(self.timeout).send().await {
            Ok(response) => {
                let is_healthy = response.status() == StatusCode::OK;
                if is_healthy {
                    info!("Ollama health check passed");
                } else {
                    warn!(
                        "Ollama health check failed with status: {}",
                        response.status()
                    );
                }
                Ok(is_healthy)
            }
            Err(e) => {
                error!("Ollama health check failed: {}", e);
                Ok(false)
            }
        }
    }

    /// List available models.
    ///
    /// # Returns
    ///
    /// Returns list of available models with their metadata.
    ///
    /// # Errors
    ///
    /// Returns error if request fails or response is invalid.
    pub async fn list_models(&self) -> Result<OllamaListResponse> {
        debug!("Listing Ollama models");

        let url = format!("{}/api/tags", self.base_url);
        let response = self
            .client
            .get(&url)
            .timeout(self.timeout)
            .send()
            .await
            .map_err(|e| {
                error!("Failed to list models: {}", e);
                OllamaClientError::from(e)
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            error!("HTTP error listing models: {} - {}", status, error_text);

            let ollama_error: OllamaError =
                serde_json::from_str(&error_text).unwrap_or_else(|_| OllamaError {
                    error: format!("HTTP {}", status),
                });

            return Err(OllamaClientError::Api(ollama_error.error).into());
        }

        let list_response = response.json::<OllamaListResponse>().await.map_err(|e| {
            error!("Failed to parse models list: {}", e);
            OllamaClientError::from(e)
        })?;

        info!("Found {} models", list_response.models.len());
        Ok(list_response)
    }

    /// Generate text (non-streaming).
    ///
    /// # Arguments
    ///
    /// * `request` - Generation request with model, prompt, and options
    ///
    /// # Returns
    ///
    /// Returns complete generated response.
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Request has `stream` enabled (use `generate_stream_raw` instead)
    /// - HTTP request fails after all retries
    /// - Response is invalid
    /// - Request times out after all retries
    /// - Circuit breaker is open
    pub async fn generate_raw(
        &self,
        mut request: OllamaGenerateRequest,
    ) -> Result<OllamaGenerateResponse> {
        use crate::shared::utils::{retry_with_backoff, RetryConfig};

        if request.stream {
            return Err(AppError::InvalidInput(
                "Use generate_stream() for streaming requests".to_string(),
            ));
        }

        request.stream = false;
        let _permit = self.acquire_request_permit("generate_raw").await?;

        debug!("Generating with model: {}", request.model);

        let base_url = self.base_url.clone();
        let client = self.client.clone();
        let timeout = self.timeout;

        // Circuit breaker wraps retry logic (policy protects resilience)
        let result = self
            .circuit_breaker
            .call(async {
                // Retry wrapper with exponential backoff for transient failures
                retry_with_backoff(
                    RetryConfig::aggressive(), // More retries for local LLM calls
                    || async {
                        // HTTP request (no circuit breaker here)
                        let url = format!("{}/api/generate", base_url);
                        let response = client
                            .post(&url)
                            .json(&request)
                            .timeout(timeout)
                            .send()
                            .await
                            .map_err(|e| {
                                error!("Generation request failed: {}", e);
                                OllamaClientError::from(e)
                            })?;

                        if !response.status().is_success() {
                            let status = response.status();
                            let error_text = response.text().await.unwrap_or_default();
                            error!("HTTP error during generation: {} - {}", status, error_text);

                            let ollama_error: OllamaError = serde_json::from_str(&error_text)
                                .unwrap_or_else(|_| OllamaError {
                                    error: format!("HTTP {}", status),
                                });

                            return Err(OllamaClientError::Api(ollama_error.error));
                        }

                        let gen_response = response
                            .json::<OllamaGenerateResponse>()
                            .await
                            .map_err(|e| {
                                error!("Failed to parse generation response: {}", e);
                                OllamaClientError::from(e)
                            })?;

                        Ok::<OllamaGenerateResponse, OllamaClientError>(gen_response)
                    },
                    |e: &OllamaClientError| {
                        // Retry on transient failures only
                        match e {
                            OllamaClientError::Connection(_) => true,
                            OllamaClientError::Timeout(_) => true,
                            OllamaClientError::Api(msg) => {
                                // Retry on server errors (Ollama might be busy)
                                msg.contains("500")
                                    || msg.contains("502")
                                    || msg.contains("503")
                                    || msg.contains("504")
                            }
                            _ => false,
                        }
                    },
                )
                .await
            })
            .await;

        // Map circuit breaker errors to AppError
        let gen_response = match result {
            Ok(response) => response,
            Err(CircuitBreakerError::Open) => {
                error!("Ollama API circuit breaker is open - service unavailable");
                return Err(AppError::Network(
                    "Ollama API unavailable (circuit breaker open)".to_string(),
                ));
            }
            Err(CircuitBreakerError::CallFailed(e)) => {
                return Err(e.into());
            }
        };

        info!(
            "Generation complete, {} tokens generated",
            gen_response.eval_count.unwrap_or(0)
        );

        Ok(gen_response)
    }

    /// Generate text with streaming.
    ///
    /// Yields chunks of generated text as they become available.
    ///
    /// # Arguments
    ///
    /// * `request` - Generation request (stream will be enabled automatically)
    ///
    /// # Returns
    ///
    /// Returns async stream of response chunks.
    ///
    /// # Errors
    ///
    /// Returns error if HTTP request fails or response is invalid.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use lattice::features::llm::engine::{OllamaClient, OllamaGenerateRequest};
    /// use futures::StreamExt;
    ///
    /// async fn stream_example() -> Result<(), Box<dyn std::error::Error>> {
    ///     let client = OllamaClient::new("http://localhost:11434")?;
    ///     let request = OllamaGenerateRequest::new("llama2", "Tell me a story");
    ///
    ///     let mut stream = client.generate_stream_raw(request).await?;
    ///     while let Some(chunk) = stream.next().await {
    ///         let chunk = chunk?;
    ///         print!("{}", chunk.response);
    ///         if chunk.done {
    ///             break;
    ///         }
    ///     }
    ///
    ///     Ok(())
    /// }
    /// ```
    pub async fn generate_stream_raw(
        &self,
        mut request: OllamaGenerateRequest,
    ) -> Result<impl futures::Stream<Item = Result<OllamaStreamResponse>> + '_> {
        request.stream = true;
        let request_permit = self.acquire_request_permit("generate_stream_raw").await?;

        debug!(
            "Starting streaming generation with model: {}",
            request.model
        );

        // Wrap the initial HTTP request in circuit breaker protection
        let result = self
            .circuit_breaker
            .call(async {
                let url = format!("{}/api/generate", self.base_url);
                let send = self.client.post(&url).json(&request).send();
                // No total timeout: it would cut off long answers. Bound the wait for
                // headers here; the stream below bounds silence between chunks.
                let response = timeout(self.stream_timeout, send)
                    .await
                    .map_err(|_| OllamaClientError::Timeout("Ollama did not respond".into()))?
                    .map_err(|e| {
                        error!("Streaming request failed: {}", e);
                        OllamaClientError::from(e)
                    })?;

                if !response.status().is_success() {
                    let status = response.status();
                    let error_text = response.text().await.unwrap_or_default();
                    error!("HTTP error during streaming: {} - {}", status, error_text);

                    let ollama_error: OllamaError = serde_json::from_str(&error_text)
                        .unwrap_or_else(|_| OllamaError {
                            error: format!("HTTP {}", status),
                        });

                    return Err(OllamaClientError::Api(ollama_error.error));
                }

                Ok::<reqwest::Response, OllamaClientError>(response)
            })
            .await;

        // Map circuit breaker errors to AppError
        let response = match result {
            Ok(resp) => resp,
            Err(CircuitBreakerError::Open) => {
                error!("Ollama API circuit breaker is open - service unavailable");
                return Err(AppError::Network(
                    "Ollama API unavailable (circuit breaker open)".to_string(),
                ));
            }
            Err(CircuitBreakerError::CallFailed(e)) => {
                return Err(e.into());
            }
        };

        let stream = async_stream::stream! {
            let _request_permit = request_permit;
            let mut bytes_stream = response.bytes_stream();
            let mut buffer = String::new();
            let mut has_content = false;

            use futures::StreamExt;

            let chunk_timeout = self.stream_timeout;
            loop {
                let next_chunk = match timeout(chunk_timeout, bytes_stream.next()).await {
                    Ok(next_chunk) => next_chunk,
                    Err(_) => {
                        error!("Stream idle timeout after {:?}", chunk_timeout);
                        yield Err(AppError::Other("LLM stream timed out".to_string()));
                        return;
                    }
                };

                let chunk = match next_chunk {
                    Some(chunk) => chunk,
                    None => break,
                };

                match chunk {
                    Ok(bytes) => {
                        let text = String::from_utf8_lossy(&bytes);
                        buffer.push_str(&text);

                        while let Some(newline_idx) = buffer.find('\n') {
                            let mut line = buffer[..newline_idx].trim().to_string();
                            buffer.drain(..=newline_idx);

                            if line.is_empty() {
                                continue;
                            }

                            if let Some(stripped) = line.strip_prefix("data:") {
                                line = stripped.trim().to_string();
                            }

                            match serde_json::from_str::<OllamaStreamResponse>(&line) {
                                Ok(chunk) => {
                                    let done = chunk.done;
                                    if !chunk.response.is_empty() {
                                        has_content = true;
                                    }
                                    yield Ok(chunk);
                                    if done {
                                        if !has_content {
                                            warn!("Generate stream completed with zero content tokens");
                                        }
                                        return;
                                    }
                                }
                                Err(e) => {
                                    if e.is_eof() {
                                        buffer = format!("{}{}", line, buffer);
                                        break;
                                    }
                                    warn!("Failed to parse stream chunk: {}", e);
                                }
                            }
                        }
                    }
                    Err(e) => {
                        error!("Stream error: {}", e);
                        yield Err(AppError::Network(format!("Stream error: {}", e)));
                        return;
                    }
                }
            }
            let remaining = buffer.trim();
            if !remaining.is_empty() {
                let mut line = remaining.to_string();
                if let Some(stripped) = line.strip_prefix("data:") {
                    line = stripped.trim().to_string();
                }
                match serde_json::from_str::<OllamaStreamResponse>(&line) {
                    Ok(chunk) => {
                        if !chunk.response.is_empty() {
                            has_content = true;
                        }
                        yield Ok(chunk);
                    }
                    Err(e) => {
                        if !e.is_eof() {
                            warn!("Failed to parse stream chunk: {}", e);
                        }
                    }
                }
            }

            if !has_content {
                warn!("Generate stream ended without yielding any content");
            }
        };

        Ok(stream)
    }

    /// Generate text using the configured model and generation config.
    ///
    /// This is a helper method for trait implementation that uses the client's
    /// configured model and generation settings.
    async fn generate_with_config(
        &self,
        prompt: &str,
        system: Option<&str>,
        images: Option<Vec<String>>,
    ) -> Result<String, LLMError> {
        let mut request = OllamaGenerateRequest::new(&self.model_name, prompt);

        if let Some(sys) = system {
            request = request.with_system(sys);
        }

        if let Some(imgs) = images {
            request = request.with_images(imgs);
        }

        let mut options = std::collections::HashMap::new();
        options.insert(
            "temperature".to_string(),
            serde_json::json!(self.config.temperature),
        );
        options.insert("top_p".to_string(), serde_json::json!(self.config.top_p));
        options.insert("top_k".to_string(), serde_json::json!(self.config.top_k));
        options.insert(
            "repeat_penalty".to_string(),
            serde_json::json!(self.config.repeat_penalty),
        );
        options.insert(
            "num_predict".to_string(),
            serde_json::json!(self.config.max_tokens),
        );
        request.options = Some(options);

        let response = self.generate_raw(request).await.map_err(LLMError::from)?;
        Ok(response.response)
    }

    /// Generate streaming text using the configured model and generation config.
    ///
    /// This is a helper method for trait implementation that uses the client's
    /// configured model and generation settings.
    async fn generate_stream_with_config(
        &self,
        prompt: &str,
        system: Option<&str>,
        images: Option<Vec<String>>,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String, LLMError>> + Send + '_>>, LLMError> {
        let mut request = OllamaGenerateRequest::new(&self.model_name, prompt);

        if let Some(sys) = system {
            request = request.with_system(sys);
        }

        if let Some(imgs) = images {
            request = request.with_images(imgs);
        }

        let mut options = std::collections::HashMap::new();
        options.insert(
            "temperature".to_string(),
            serde_json::json!(self.config.temperature),
        );
        options.insert("top_p".to_string(), serde_json::json!(self.config.top_p));
        options.insert("top_k".to_string(), serde_json::json!(self.config.top_k));
        options.insert(
            "repeat_penalty".to_string(),
            serde_json::json!(self.config.repeat_penalty),
        );
        options.insert(
            "num_predict".to_string(),
            serde_json::json!(self.config.max_tokens),
        );
        request.options = Some(options);

        let stream = self
            .generate_stream_raw(request)
            .await
            .map_err(LLMError::from)?;

        use futures::StreamExt;
        let text_stream =
            stream.map(|result| result.map(|chunk| chunk.response).map_err(LLMError::from));

        Ok(Box::pin(text_stream))
    }

    /// Generate chat response using Ollama Chat API (non-streaming).
    ///
    /// # Arguments
    /// * `messages` - Conversation messages (system, user, assistant)
    ///
    /// # Returns
    /// Generated text response
    ///
    /// # Errors
    /// Returns error if HTTP request fails or response is invalid
    pub async fn chat(
        &self,
        messages: Vec<crate::features::llm::engine::traits::ChatMessage>,
    ) -> Result<String, LLMError> {
        use crate::features::llm::engine::types::{
            OllamaChatMessage, OllamaChatRequest, OllamaChatResponse,
        };
        use crate::shared::utils::{retry_with_backoff, RetryConfig};

        debug!("Generating chat with model: {}", self.model_name);
        let _permit = self
            .acquire_request_permit("chat")
            .await
            .map_err(LLMError::from)?;

        let ollama_messages: Vec<OllamaChatMessage> = messages
            .into_iter()
            .map(|m| OllamaChatMessage {
                role: m.role,
                content: m.content,
                images: None,
                tool_calls: None,
                tool_name: None,
            })
            .collect();

        let mut request = OllamaChatRequest {
            model: self.model_name.clone(),
            messages: ollama_messages,
            stream: false,
            options: None,
            keep_alive: None,
            tools: None,
        };

        let mut options = std::collections::HashMap::new();
        options.insert(
            "temperature".to_string(),
            serde_json::json!(self.config.temperature),
        );
        options.insert("top_p".to_string(), serde_json::json!(self.config.top_p));
        options.insert("top_k".to_string(), serde_json::json!(self.config.top_k));
        options.insert(
            "repeat_penalty".to_string(),
            serde_json::json!(self.config.repeat_penalty),
        );
        options.insert(
            "num_predict".to_string(),
            serde_json::json!(self.config.max_tokens),
        );
        request.options = Some(options);

        let base_url = self.base_url.clone();
        let client = self.client.clone();
        let timeout = self.timeout;

        // Circuit breaker wraps retry logic
        let result = self
            .circuit_breaker
            .call(async {
                retry_with_backoff(
                    RetryConfig::aggressive(),
                    || async {
                        let url = format!("{}/api/chat", base_url);
                        let response = client
                            .post(&url)
                            .json(&request)
                            .timeout(timeout)
                            .send()
                            .await
                            .map_err(|e| {
                                error!("Chat request failed: {}", e);
                                OllamaClientError::from(e)
                            })?;

                        if !response.status().is_success() {
                            let status = response.status();
                            let error_text = response.text().await.unwrap_or_default();
                            error!("HTTP error during chat: {} - {}", status, error_text);

                            let ollama_error: OllamaError = serde_json::from_str(&error_text)
                                .unwrap_or_else(|_| OllamaError {
                                    error: format!("HTTP {}", status),
                                });

                            return Err(OllamaClientError::Api(ollama_error.error));
                        }

                        let chat_response =
                            response.json::<OllamaChatResponse>().await.map_err(|e| {
                                error!("Failed to parse chat response: {}", e);
                                OllamaClientError::from(e)
                            })?;

                        Ok::<OllamaChatResponse, OllamaClientError>(chat_response)
                    },
                    |e: &OllamaClientError| match e {
                        OllamaClientError::Connection(_) => true,
                        OllamaClientError::Timeout(_) => true,
                        OllamaClientError::Api(msg) => {
                            msg.contains("500")
                                || msg.contains("502")
                                || msg.contains("503")
                                || msg.contains("504")
                        }
                        _ => false,
                    },
                )
                .await
            })
            .await;

        let chat_response = match result {
            Ok(response) => response,
            Err(crate::features::llm::engine::circuit_breaker::CircuitBreakerError::Open) => {
                error!("Ollama API circuit breaker is open - service unavailable");
                return Err(LLMError::Network(
                    "Ollama API unavailable (circuit breaker open)".to_string(),
                ));
            }
            Err(
                crate::features::llm::engine::circuit_breaker::CircuitBreakerError::CallFailed(e),
            ) => {
                return Err(e.into());
            }
        };

        info!(
            "Chat complete, {} tokens generated",
            chat_response.eval_count.unwrap_or(0)
        );

        Ok(chat_response.message.content)
    }

    /// Generate chat response with streaming using Ollama Chat API.
    ///
    /// # Arguments
    /// * `messages` - Conversation messages (system, user, assistant)
    ///
    /// # Returns
    /// Stream of text chunks
    ///
    /// # Errors
    /// Returns error if HTTP request fails or response is invalid
    pub async fn chat_stream(
        &self,
        messages: Vec<crate::features::llm::engine::traits::ChatMessage>,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String, LLMError>> + Send + '_>>, LLMError> {
        use crate::features::llm::engine::types::{
            OllamaChatMessage, OllamaChatRequest, OllamaChatStreamResponse,
        };

        debug!("Starting streaming chat with model: {}", self.model_name);
        let request_permit = self
            .acquire_request_permit("chat_stream")
            .await
            .map_err(LLMError::from)?;

        let ollama_messages: Vec<OllamaChatMessage> = messages
            .into_iter()
            .map(|m| OllamaChatMessage {
                role: m.role,
                content: m.content,
                images: None,
                tool_calls: None,
                tool_name: None,
            })
            .collect();

        let mut request = OllamaChatRequest {
            model: self.model_name.clone(),
            messages: ollama_messages,
            stream: true,
            options: None,
            keep_alive: None,
            tools: None,
        };

        let mut options = std::collections::HashMap::new();
        options.insert(
            "temperature".to_string(),
            serde_json::json!(self.config.temperature),
        );
        options.insert("top_p".to_string(), serde_json::json!(self.config.top_p));
        options.insert("top_k".to_string(), serde_json::json!(self.config.top_k));
        options.insert(
            "repeat_penalty".to_string(),
            serde_json::json!(self.config.repeat_penalty),
        );
        options.insert(
            "num_predict".to_string(),
            serde_json::json!(self.config.max_tokens),
        );
        request.options = Some(options);

        // Wrap the initial HTTP request in circuit breaker protection
        let result = self
            .circuit_breaker
            .call(async {
                let url = format!("{}/api/chat", self.base_url);
                let send = self.client.post(&url).json(&request).send();
                // No total timeout: it would cut off long answers. Bound the wait for
                // headers here; the stream below bounds silence between chunks.
                let response = timeout(self.stream_timeout, send)
                    .await
                    .map_err(|_| OllamaClientError::Timeout("Ollama did not respond".into()))?
                    .map_err(|e| {
                        error!("Streaming chat request failed: {}", e);
                        OllamaClientError::from(e)
                    })?;

                if !response.status().is_success() {
                    let status = response.status();
                    let error_text = response.text().await.unwrap_or_default();
                    error!(
                        "HTTP error during streaming chat: {} - {}",
                        status, error_text
                    );

                    let ollama_error: OllamaError = serde_json::from_str(&error_text)
                        .unwrap_or_else(|_| OllamaError {
                            error: format!("HTTP {}", status),
                        });

                    return Err(OllamaClientError::Api(ollama_error.error));
                }

                Ok::<reqwest::Response, OllamaClientError>(response)
            })
            .await;

        let response = match result {
            Ok(resp) => resp,
            Err(crate::features::llm::engine::circuit_breaker::CircuitBreakerError::Open) => {
                error!("Ollama API circuit breaker is open - service unavailable");
                return Err(LLMError::Network(
                    "Ollama API unavailable (circuit breaker open)".to_string(),
                ));
            }
            Err(
                crate::features::llm::engine::circuit_breaker::CircuitBreakerError::CallFailed(e),
            ) => {
                return Err(e.into());
            }
        };

        let stream = async_stream::stream! {
            let _request_permit = request_permit;
            let mut bytes_stream = response.bytes_stream();
            let mut buffer = String::new();
            let mut has_content = false;

            use futures::StreamExt;

            let chunk_timeout = self.stream_timeout;
            loop {
                let next_chunk = match timeout(chunk_timeout, bytes_stream.next()).await {
                    Ok(next_chunk) => next_chunk,
                    Err(_) => {
                        error!("Chat stream idle timeout after {:?}", chunk_timeout);
                        yield Err(LLMError::Timeout);
                        return;
                    }
                };

                let chunk = match next_chunk {
                    Some(chunk) => chunk,
                    None => break,
                };

                match chunk {
                    Ok(bytes) => {
                        let text = String::from_utf8_lossy(&bytes);
                        buffer.push_str(&text);

                        while let Some(newline_idx) = buffer.find('\n') {
                            let mut line = buffer[..newline_idx].trim().to_string();
                            buffer.drain(..=newline_idx);

                            if line.is_empty() {
                                continue;
                            }

                            if let Some(stripped) = line.strip_prefix("data:") {
                                line = stripped.trim().to_string();
                            }

                            match serde_json::from_str::<OllamaChatStreamResponse>(&line) {
                                Ok(chunk) => {
                                    let done = chunk.done;
                                    // Filter empty content chunks to prevent empty response accumulation
                                    if !chunk.message.content.is_empty() {
                                        has_content = true;
                                        yield Ok(chunk.message.content);
                                    }
                                    if done {
                                        if !has_content {
                                            warn!("Chat stream completed with zero content tokens from model");
                                            yield Err(LLMError::GenerationFailed(
                                                "Model returned empty response (zero content tokens)".to_string()
                                            ));
                                        }
                                        return;
                                    }
                                }
                                Err(e) => {
                                    if e.is_eof() {
                                        buffer = format!("{}{}", line, buffer);
                                        break;
                                    }
                                    warn!("Failed to parse chat stream chunk: {}", e);
                                }
                            }
                        }
                    }
                    Err(e) => {
                        error!("Chat stream error: {}", e);
                        yield Err(LLMError::Network(format!("Stream error: {}", e)));
                        return;
                    }
                }
            }
            let remaining = buffer.trim();
            if !remaining.is_empty() {
                let mut line = remaining.to_string();
                if let Some(stripped) = line.strip_prefix("data:") {
                    line = stripped.trim().to_string();
                }
                match serde_json::from_str::<OllamaChatStreamResponse>(&line) {
                    Ok(chunk) => {
                        if !chunk.message.content.is_empty() {
                            has_content = true;
                            yield Ok(chunk.message.content);
                        }
                    }
                    Err(e) => {
                        if !e.is_eof() {
                            warn!("Failed to parse chat stream chunk: {}", e);
                        }
                    }
                }
            }

            if !has_content {
                warn!("Chat stream ended without yielding any content");
                yield Err(LLMError::GenerationFailed(
                    "Model returned empty response (stream ended without content)".to_string()
                ));
            }
        };

        Ok(Box::pin(stream))
    }
}

#[async_trait]
impl LLMClient for OllamaClient {
    async fn generate(
        &self,
        prompt: &str,
        system: Option<&str>,
        images: Option<Vec<String>>,
    ) -> Result<String, LLMError> {
        self.generate_with_config(prompt, system, images).await
    }

    async fn generate_stream(
        &self,
        prompt: &str,
        system: Option<&str>,
        images: Option<Vec<String>>,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String, LLMError>> + Send + '_>>, LLMError> {
        self.generate_stream_with_config(prompt, system, images)
            .await
    }

    async fn health_check(&self) -> bool {
        self.health_check_with_result().await.unwrap_or(false)
    }

    fn model_name(&self) -> &str {
        &self.model_name
    }

    fn generation_config(&self) -> &GenerationConfig {
        &self.config
    }

    fn generation_config_mut(&mut self) -> &mut GenerationConfig {
        &mut self.config
    }

    async fn generate_chat(
        &self,
        messages: Vec<crate::features::llm::engine::traits::ChatMessage>,
    ) -> Result<String, LLMError> {
        self.chat(messages).await
    }

    async fn generate_chat_stream(
        &self,
        messages: Vec<crate::features::llm::engine::traits::ChatMessage>,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String, LLMError>> + Send + '_>>, LLMError> {
        self.chat_stream(messages).await
    }

    fn supports_chat(&self) -> bool {
        true
    }
}

use crate::application::ports::LLMPort;

#[async_trait]
impl LLMPort for OllamaClient {
    async fn generate(
        &self,
        prompt: &str,
        context: &[String],
        images: Option<Vec<String>>,
    ) -> crate::shared::result::Result<String> {
        let system = if context.is_empty() {
            None
        } else {
            Some(format!(
                "Use the following context to answer the question:\n\n{}",
                context.join("\n\n")
            ))
        };

        // Delegate to LLMClient::generate
        <Self as LLMClient>::generate(self, prompt, system.as_deref(), images)
            .await
            .map_err(|e| {
                crate::shared::error::AppError::Other(format!("LLM generation failed: {}", e))
            })
    }

    async fn generate_streaming(
        &self,
        prompt: &str,
        context: &[String],
        images: Option<Vec<String>>,
    ) -> crate::shared::result::Result<
        Box<dyn Stream<Item = crate::shared::result::Result<String>> + Send + Unpin + '_>,
    > {
        fn parse_context_to_chat_message(
            context_entry: &str,
        ) -> Option<crate::features::llm::engine::traits::ChatMessage> {
            let trimmed = context_entry.trim();
            let prefixes = [
                ("system:", "system"),
                ("user:", "user"),
                ("assistant:", "assistant"),
            ];

            for (prefix, role) in prefixes {
                if let (Some(head), Some(tail)) =
                    (trimmed.get(..prefix.len()), trimmed.get(prefix.len()..))
                {
                    if !head.eq_ignore_ascii_case(prefix) {
                        continue;
                    }

                    let content = tail.trim();
                    if !content.is_empty() {
                        return Some(crate::features::llm::engine::traits::ChatMessage {
                            role: role.to_string(),
                            content: content.to_string(),
                        });
                    }
                }
            }

            None
        }

        let parsed_messages: Vec<crate::features::llm::engine::traits::ChatMessage> = context
            .iter()
            .filter_map(|entry| parse_context_to_chat_message(entry))
            .collect();

        let stream = if !parsed_messages.is_empty() {
            let mut messages = parsed_messages;
            messages.push(crate::features::llm::engine::traits::ChatMessage {
                role: "user".to_string(),
                content: prompt.to_string(),
            });

            <Self as LLMClient>::generate_chat_stream(self, messages)
                .await
                .map_err(|e| {
                    crate::shared::error::AppError::Other(format!("LLM streaming failed: {}", e))
                })?
        } else {
            // Fallback to flattened context for backward compatibility.
            let system = if context.is_empty() {
                None
            } else {
                Some(format!(
                    "Use the following context to answer the question:\n\n{}",
                    context.join("\n\n")
                ))
            };

            <Self as LLMClient>::generate_stream(self, prompt, system.as_deref(), images)
                .await
                .map_err(|e| {
                    crate::shared::error::AppError::Other(format!("LLM streaming failed: {}", e))
                })?
        };

        use futures::StreamExt;
        let mapped_stream = stream.map(|result| {
            result
                .map_err(|e| crate::shared::error::AppError::Other(format!("Stream error: {}", e)))
        });

        Ok(Box::new(Box::pin(mapped_stream)))
    }

    fn model_name(&self) -> &str {
        &self.model_name
    }

    fn max_context_tokens(&self) -> usize {
        // Default Ollama context window (varies by model)
        // llama3.1:8b has 128K context window
        128_000
    }

    fn count_tokens(&self, text: &str) -> usize {
        // Rough approximation: 1 token ≈ 4 characters
        // This is a simple heuristic and should be replaced with proper tokenization
        text.len().div_ceil(4)
    }

    async fn is_ready(&self) -> crate::shared::result::Result<bool> {
        self.health_check_with_result().await
    }

    fn supports_tool_calling(&self) -> bool {
        true
    }

    fn provider_name(&self) -> &str {
        "ollama"
    }

    async fn generate_streaming_with_tools(
        &self,
        prompt: &str,
        context: &[String],
        images: Option<Vec<String>>,
        tools: Option<&[crate::application::ports::ToolDefinition]>,
    ) -> crate::shared::result::Result<
        Box<
            dyn Stream<Item = crate::shared::result::Result<crate::application::ports::StreamChunk>>
                + Send
                + Unpin
                + '_,
        >,
    > {
        use crate::application::ports::{StreamChunk, ToolCall};
        use crate::features::llm::engine::types::{
            OllamaChatMessage, OllamaChatRequest, OllamaChatStreamResponse, OllamaTool,
        };
        let request_permit = self
            .acquire_request_permit("generate_streaming_with_tools")
            .await?;

        fn parse_context_to_chat_message(
            entry: &str,
        ) -> Option<crate::features::llm::engine::traits::ChatMessage> {
            let trimmed = entry.trim();
            for (prefix, role) in [
                ("system:", "system"),
                ("user:", "user"),
                ("assistant:", "assistant"),
            ] {
                if let (Some(head), Some(tail)) =
                    (trimmed.get(..prefix.len()), trimmed.get(prefix.len()..))
                {
                    if head.eq_ignore_ascii_case(prefix) {
                        let content = tail.trim();
                        if !content.is_empty() {
                            return Some(crate::features::llm::engine::traits::ChatMessage {
                                role: role.to_string(),
                                content: content.to_string(),
                            });
                        }
                    }
                }
            }
            None
        }

        let mut ollama_messages: Vec<OllamaChatMessage> = context
            .iter()
            .filter_map(|entry| parse_context_to_chat_message(entry))
            .map(|m| OllamaChatMessage {
                role: m.role,
                content: m.content,
                images: None,
                tool_calls: None,
                tool_name: None,
            })
            .collect();

        ollama_messages.push(OllamaChatMessage {
            role: "user".to_string(),
            content: prompt.to_string(),
            images: images.clone(),
            tool_calls: None,
            tool_name: None,
        });

        // Convert application-layer tool definitions to Ollama format (consume eagerly
        // so the `tools` borrow is released before the stream is constructed)
        let ollama_tools: Option<Vec<OllamaTool>> = tools.map(|defs| {
            defs.iter()
                .map(|td| OllamaTool::new(&td.name, &td.description, td.parameters.clone()))
                .collect()
        });
        // Track whether we had tools for the diagnostic at end of stream
        let had_tools = tools.is_some();

        let mut options = std::collections::HashMap::new();
        options.insert(
            "temperature".to_string(),
            serde_json::json!(self.config.temperature),
        );
        options.insert("top_p".to_string(), serde_json::json!(self.config.top_p));
        options.insert("top_k".to_string(), serde_json::json!(self.config.top_k));
        options.insert(
            "num_predict".to_string(),
            serde_json::json!(self.config.max_tokens),
        );

        let request = OllamaChatRequest {
            model: self.model_name.clone(),
            messages: ollama_messages,
            stream: true,
            options: Some(options),
            keep_alive: None,
            tools: ollama_tools,
        };

        // Make the HTTP request with circuit breaker
        let result = self
            .circuit_breaker
            .call(async {
                let url = format!("{}/api/chat", self.base_url);
                let send = self.client.post(&url).json(&request).send();
                // No total timeout: it would cut off long answers. Bound the wait for
                // headers here; the stream below bounds silence between chunks.
                let response = timeout(self.stream_timeout, send)
                    .await
                    .map_err(|_| OllamaClientError::Timeout("Ollama did not respond".into()))?
                    .map_err(|e| {
                        error!("Tool-enabled chat request failed: {}", e);
                        OllamaClientError::from(e)
                    })?;

                if !response.status().is_success() {
                    let status = response.status();
                    let error_text = response.text().await.unwrap_or_default();
                    error!(
                        "HTTP error during tool-enabled chat: {} - {}",
                        status, error_text
                    );
                    let ollama_error: OllamaError = serde_json::from_str(&error_text)
                        .unwrap_or_else(|_| OllamaError {
                            error: format!("HTTP {}", status),
                        });
                    return Err(OllamaClientError::Api(ollama_error.error));
                }

                Ok::<reqwest::Response, OllamaClientError>(response)
            })
            .await;

        let response = match result {
            Ok(resp) => resp,
            Err(crate::features::llm::engine::circuit_breaker::CircuitBreakerError::Open) => {
                return Err(crate::shared::error::AppError::Network(
                    "Ollama API unavailable (circuit breaker open)".to_string(),
                ));
            }
            Err(
                crate::features::llm::engine::circuit_breaker::CircuitBreakerError::CallFailed(e),
            ) => {
                return Err(e.into());
            }
        };

        let chunk_timeout = self.stream_timeout;
        let stream = async_stream::stream! {
            let _request_permit = request_permit;
            let mut bytes_stream = response.bytes_stream();
            let mut buffer = String::new();
            let mut has_content = false;
            let mut accumulated_tool_calls: Vec<ToolCall> = Vec::new();

            use futures::StreamExt;

            loop {
                let next_chunk = match timeout(chunk_timeout, bytes_stream.next()).await {
                    Ok(next_chunk) => next_chunk,
                    Err(_) => {
                        error!("Tool chat stream idle timeout after {:?}", chunk_timeout);
                        yield Err(crate::shared::error::AppError::Other(
                            "LLM stream timed out".to_string(),
                        ));
                        return;
                    }
                };

                let chunk = match next_chunk {
                    Some(chunk) => chunk,
                    None => break,
                };

                match chunk {
                    Ok(bytes) => {
                        let text = String::from_utf8_lossy(&bytes);
                        buffer.push_str(&text);

                        while let Some(newline_idx) = buffer.find('\n') {
                            let mut line = buffer[..newline_idx].trim().to_string();
                            buffer.drain(..=newline_idx);

                            if line.is_empty() {
                                continue;
                            }
                            if let Some(stripped) = line.strip_prefix("data:") {
                                line = stripped.trim().to_string();
                            }

                            match serde_json::from_str::<OllamaChatStreamResponse>(&line) {
                                Ok(chunk) => {
                                    let done = chunk.done;

                                    if let Some(ref tc) = chunk.message.tool_calls {
                                        for call in tc {
                                            accumulated_tool_calls.push(ToolCall {
                                                id: None,
                                                name: call.function.name.clone(),
                                                arguments: call.function.arguments.clone(),
                                            });
                                        }
                                    }

                                    // Yield content if non-empty
                                    if !chunk.message.content.is_empty() {
                                        has_content = true;
                                        yield Ok(StreamChunk::Content(chunk.message.content));
                                    }

                                    if done {
                                        // If we accumulated tool calls, yield them
                                        if !accumulated_tool_calls.is_empty() {
                                            yield Ok(StreamChunk::ToolCalls(
                                                std::mem::take(&mut accumulated_tool_calls),
                                            ));
                                        }
                                        yield Ok(StreamChunk::Done);
                                        return;
                                    }
                                }
                                Err(e) => {
                                    if e.is_eof() {
                                        buffer = format!("{}{}", line, buffer);
                                        break;
                                    }
                                    warn!("Failed to parse tool chat stream chunk: {}", e);
                                }
                            }
                        }
                    }
                    Err(e) => {
                        error!("Tool chat stream error: {}", e);
                        yield Err(crate::shared::error::AppError::Network(
                            format!("Stream error: {}", e),
                        ));
                        return;
                    }
                }
            }

            let remaining = buffer.trim();
            if !remaining.is_empty() {
                let mut line = remaining.to_string();
                if let Some(stripped) = line.strip_prefix("data:") {
                    line = stripped.trim().to_string();
                }
                if let Ok(chunk) = serde_json::from_str::<OllamaChatStreamResponse>(&line) {
                    if let Some(ref tc) = chunk.message.tool_calls {
                        for call in tc {
                            accumulated_tool_calls.push(ToolCall {
                                                id: None,
                                name: call.function.name.clone(),
                                arguments: call.function.arguments.clone(),
                            });
                        }
                    }
                    if !chunk.message.content.is_empty() {
                        has_content = true;
                        yield Ok(StreamChunk::Content(chunk.message.content));
                    }
                }
            }

            // Yield accumulated tool calls if any
            if !accumulated_tool_calls.is_empty() {
                yield Ok(StreamChunk::ToolCalls(accumulated_tool_calls));
            }

            // Always end with Done
            yield Ok(StreamChunk::Done);

            if !has_content && !had_tools {
                warn!("Tool chat stream ended without content or tool calls");
            }
        };

        Ok(Box::new(Box::pin(stream)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_creation() {
        let client = OllamaClient::new("http://localhost:11434");
        assert!(client.is_ok());
    }

    #[test]
    fn test_client_url_normalization() {
        let client = OllamaClient::new("http://localhost:11434/").unwrap();
        assert!(!client.base_url.ends_with('/'));
    }

    #[test]
    fn test_generate_request_validation() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime.block_on(async {
            let client = OllamaClient::new("http://localhost:11434").unwrap();
            let request = OllamaGenerateRequest::new("llama2", "test").with_stream(true);

            let result = client.generate_raw(request).await;
            assert!(result.is_err());
            assert!(result.unwrap_err().to_string().contains("streaming"));
        });
    }

    #[test]
    fn test_client_with_model() {
        let client = OllamaClient::with_model("http://localhost:11434", "mistral").unwrap();
        use crate::features::llm::engine::traits::LLMClient;
        assert_eq!(LLMClient::model_name(&client), "mistral");
    }

    #[test]
    fn test_generation_config() {
        let mut client = OllamaClient::with_model("http://localhost:11434", "llama3.1:8b").unwrap();

        assert_eq!(client.generation_config().temperature, 0.7);

        // Modify config
        client.generation_config_mut().temperature = 0.5;
        assert_eq!(client.generation_config().temperature, 0.5);
    }

    #[tokio::test]
    #[ignore = "Requires Ollama running"]
    async fn test_trait_generate() {
        use crate::features::llm::engine::traits::LLMClient;

        let client = OllamaClient::with_model("http://localhost:11434", "llama3.1:8b").unwrap();
        let result = LLMClient::generate(
            &client,
            "Say hello",
            Some("You are a helpful assistant"),
            None,
        )
        .await;

        // This will fail if Ollama is not running
        if let Ok(text) = result {
            assert!(!text.is_empty());
        }
    }

    #[tokio::test]
    async fn health_check_requires_an_ollama_route_not_just_a_listening_port() {
        use wiremock::{
            matchers::{method, path},
            Mock, MockServer, ResponseTemplate,
        };

        // A llama.cpp server answers "/" with its web UI but has no /api/tags.
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/"))
            .respond_with(ResponseTemplate::new(200).set_body_string("<html>llama.cpp</html>"))
            .mount(&server)
            .await;
        let client = OllamaClient::with_model(server.uri(), "any-model").unwrap();
        assert!(!client.health_check_with_result().await.unwrap());

        Mock::given(method("GET"))
            .and(path("/api/tags"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({"models":[]})),
            )
            .mount(&server)
            .await;
        assert!(client.health_check_with_result().await.unwrap());
    }

    /// A client-wide `timeout` covers the response body, so it would truncate a
    /// generation that is still producing bytes.
    #[tokio::test]
    async fn a_streamed_body_outlives_the_non_streaming_timeout() {
        use futures::StreamExt;
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            // Drain the request headers; the body is irrelevant to this test.
            let mut buffer = [0u8; 4096];
            let read = socket.read(&mut buffer).await.unwrap();
            assert!(read > 0, "client sent no request");
            let chunks = [
                "{\"model\":\"m\",\"created_at\":\"\",\"response\":\"Slow \",\"done\":false}\n",
                "{\"model\":\"m\",\"created_at\":\"\",\"response\":\"answer\",\"done\":true}\n",
            ];
            let size: usize = chunks.iter().map(|chunk| chunk.len()).sum();
            socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/x-ndjson\r\nContent-Length: {size}\r\n\r\n").as_bytes()).await.unwrap();
            for chunk in chunks {
                tokio::time::sleep(Duration::from_millis(250)).await;
                socket.write_all(chunk.as_bytes()).await.unwrap();
            }
        });

        let client = OllamaClient::with_model_and_timeouts(
            format!("http://{address}"),
            "any-model",
            Duration::from_millis(150),
            Duration::from_secs(5),
        )
        .unwrap();
        let mut stream = Box::pin(
            client
                .generate_stream_raw(OllamaGenerateRequest::new("any-model", "Question"))
                .await
                .unwrap(),
        );
        let mut answer = String::new();
        while let Some(chunk) = stream.next().await {
            answer.push_str(&chunk.unwrap().response);
        }
        assert_eq!(answer, "Slow answer");
        server.await.unwrap();
    }

    #[tokio::test]
    #[ignore = "Requires Ollama running"]
    async fn test_trait_health_check() {
        use crate::features::llm::engine::traits::LLMClient;

        let client = OllamaClient::with_model("http://localhost:11434", "llama3.1:8b").unwrap();
        let is_healthy = client.health_check().await;

        // This will be true if Ollama is running
        println!("Health check result: {}", is_healthy);
    }
}
