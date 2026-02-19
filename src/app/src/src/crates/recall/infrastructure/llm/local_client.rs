//! Local LLM client implementation using mistral.rs.

use async_trait::async_trait;
use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;
use tokio_stream::Stream;

use crate::llm::inference::{InferenceConfig, InferenceEngine};
use crate::llm::models::ModelInfo;
use crate::llm::traits::{GenerationConfig, LLMClient};
use crate::llm::types::LLMError;
use mistralrs::TextMessageRole;

/// Local LLM client using GPU-accelerated mistral.rs inference.
pub struct LocalLLMClient {
    engine: Arc<InferenceEngine>,
    model_info: ModelInfo,
    config: GenerationConfig,
}

impl LocalLLMClient {
    /// Create a new local LLM client.
    ///
    /// # Arguments
    /// * `model_path` - Path to GGUF model file
    /// * `model_info` - Model metadata
    /// * `inference_config` - Inference configuration (GPU settings, context size, etc.)
    /// * `generation_config` - Text generation configuration (temperature, etc.)
    pub async fn new<P: AsRef<Path>>(
        model_path: P,
        model_info: ModelInfo,
        inference_config: InferenceConfig,
        generation_config: GenerationConfig,
    ) -> Result<Self, LLMError> {
        let engine = InferenceEngine::from_path(model_path, inference_config).await?;

        Ok(Self {
            engine: Arc::new(engine),
            model_info,
            config: generation_config,
        })
    }

    /// Create a client with default configurations.
    ///
    /// # Arguments
    /// * `model_path` - Path to GGUF model file
    /// * `model_info` - Model metadata
    pub async fn with_defaults<P: AsRef<Path>>(
        model_path: P,
        model_info: ModelInfo,
    ) -> Result<Self, LLMError> {
        Self::new(
            model_path,
            model_info,
            InferenceConfig::default(),
            GenerationConfig::default(),
        )
        .await
    }

    /// Create a CPU-only client (no GPU acceleration).
    ///
    /// # Arguments
    /// * `model_path` - Path to GGUF model file
    /// * `model_info` - Model metadata
    pub async fn cpu_only<P: AsRef<Path>>(
        model_path: P,
        model_info: ModelInfo,
    ) -> Result<Self, LLMError> {
        Self::new(
            model_path,
            model_info,
            InferenceConfig::cpu_only(),
            GenerationConfig::default(),
        )
        .await
    }

    /// Create a GPU-accelerated client.
    ///
    /// # Arguments
    /// * `model_path` - Path to GGUF model file
    /// * `model_info` - Model metadata
    /// * `n_gpu_layers` - Number of layers to offload to GPU (-1 for all)
    pub async fn with_gpu<P: AsRef<Path>>(
        model_path: P,
        model_info: ModelInfo,
        n_gpu_layers: i32,
    ) -> Result<Self, LLMError> {
        Self::new(
            model_path,
            model_info,
            InferenceConfig::with_gpu(n_gpu_layers),
            GenerationConfig::default(),
        )
        .await
    }

    /// Generate with structured message history (for chat templates).
    pub async fn generate_with_messages(
        &self,
        messages: &[(TextMessageRole, String)],
    ) -> Result<String, LLMError> {
        self.engine
            .generate_with_messages(messages, &self.config)
            .await
    }

    /// Stream generation with structured message history (for chat templates).
    pub async fn generate_stream_with_messages(
        &self,
        messages: &[(TextMessageRole, String)],
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String, LLMError>> + Send + '_>>, LLMError> {
        self.engine
            .generate_stream_with_messages(messages, &self.config)
            .await
    }
}

#[async_trait]
impl LLMClient for LocalLLMClient {
    async fn generate(
        &self,
        prompt: &str,
        system: Option<&str>,
        _images: Option<Vec<String>>,
    ) -> Result<String, LLMError> {
        self.engine.generate(prompt, system, &self.config).await
    }

    async fn generate_stream(
        &self,
        prompt: &str,
        system: Option<&str>,
        _images: Option<Vec<String>>,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<String, LLMError>> + Send + '_>>, LLMError> {
        let stream = self
            .engine
            .generate_stream(prompt, system, &self.config)
            .await?;
        Ok(Box::pin(stream))
    }

    async fn health_check(&self) -> bool {
        self.engine.is_ready()
    }

    fn model_name(&self) -> &str {
        &self.model_info.name
    }

    fn generation_config(&self) -> &GenerationConfig {
        &self.config
    }

    fn generation_config_mut(&mut self) -> &mut GenerationConfig {
        &mut self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::models::{ModelFamily, Quantization};

    fn create_test_model_info() -> ModelInfo {
        ModelInfo {
            name: "Test Model".to_string(),
            family: ModelFamily::Llama,
            quantization: Some(Quantization::Q4),
            size_mb: 4096,
        }
    }

    #[tokio::test]
    async fn test_client_creation_fails_with_invalid_path() {
        let model_info = create_test_model_info();
        let result = LocalLLMClient::with_defaults("/nonexistent/model.gguf", model_info).await;
        assert!(result.is_err());
    }

    #[test]
    fn test_model_name() {
        // This test would require a real model file, so we'll skip the actual creation
        let model_info = create_test_model_info();
        assert_eq!(model_info.name, "Test Model");
    }
}
