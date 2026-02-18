/// Ollama client wrapper
///
/// Provides a high-level interface to the Ollama API for text generation.
use crate::infrastructure::qa::types::QAError;
use ollama_rs::{
    generation::{completion::request::GenerationRequest, options::GenerationOptions},
    Ollama,
};
use tokio_stream::{once, Stream};

/// Ollama client for LLM generation
pub struct OllamaClient {
    /// Ollama client instance
    client: Ollama,

    /// Model name (e.g., "llama3.1:8b", "mistral")
    model: String,

    /// Generation temperature (0.0 - 1.0)
    temperature: f32,

    /// Top-p sampling
    top_p: f32,
}

impl OllamaClient {
    /// Create a new Ollama client
    ///
    /// # Arguments
    /// * `url` - Ollama API base URL (e.g., "http://localhost:11434")
    /// * `model` - Model name (e.g., "llama3.1:8b")
    ///
    /// # Returns
    /// New OllamaClient instance
    ///
    /// # Examples
    /// ```no_run
    /// use vault_desktop::qa::OllamaClient;
    ///
    /// let client = OllamaClient::new(
    ///     "http://localhost:11434",
    ///     "llama3.1:8b".to_string()
    /// );
    /// ```
    pub fn new(url: &str, model: String) -> Self {
        // Parse URL to get host and port
        let url = url.trim_end_matches('/');
        let host = url
            .strip_prefix("http://")
            .or_else(|| url.strip_prefix("https://"))
            .unwrap_or(url);

        let (host, port) = if let Some((h, p)) = host.split_once(':') {
            (h.to_string(), p.parse::<u16>().unwrap_or(11434))
        } else {
            (host.to_string(), 11434)
        };

        let client = Ollama::new(host, port);

        Self {
            client,
            model,
            temperature: 0.7,
            top_p: 0.9,
        }
    }

    /// Set generation temperature
    ///
    /// # Arguments
    /// * `temperature` - Temperature value (0.0 - 1.0)
    ///
    /// # Returns
    /// Self for method chaining
    pub fn with_temperature(mut self, temperature: f32) -> Self {
        self.temperature = temperature.clamp(0.0, 1.0);
        self
    }

    /// Set top-p sampling
    ///
    /// # Arguments
    /// * `top_p` - Top-p value (0.0 - 1.0)
    ///
    /// # Returns
    /// Self for method chaining
    pub fn with_top_p(mut self, top_p: f32) -> Self {
        self.top_p = top_p.clamp(0.0, 1.0);
        self
    }

    /// Generate text (non-streaming)
    ///
    /// # Arguments
    /// * `prompt` - User prompt
    /// * `system` - Optional system prompt
    ///
    /// # Returns
    /// Generated text response
    ///
    /// # Errors
    /// Returns error if generation fails or Ollama is unavailable
    pub async fn generate(&self, prompt: &str, system: Option<&str>) -> Result<String, QAError> {
        let mut request = GenerationRequest::new(self.model.clone(), prompt.to_string());

        if let Some(sys) = system {
            request = request.system(sys.to_string());
        }

        let options = GenerationOptions::default()
            .temperature(self.temperature)
            .top_p(self.top_p);
        request = request.options(options);

        let response =
            self.client.generate(request).await.map_err(|e| {
                QAError::OllamaUnavailable(format!("Ollama generation failed: {}", e))
            })?;

        Ok(response.response)
    }

    /// Generate text with streaming
    ///
    /// # Arguments
    /// * `prompt` - User prompt
    /// * `system` - Optional system prompt
    ///
    /// # Returns
    /// Stream of text chunks
    ///
    /// # Errors
    /// Returns error if streaming fails or Ollama is unavailable
    pub async fn generate_stream(
        &self,
        prompt: &str,
        system: Option<&str>,
    ) -> Result<impl Stream<Item = Result<String, QAError>>, QAError> {
        let mut request = GenerationRequest::new(self.model.clone(), prompt.to_string());

        if let Some(sys) = system {
            request = request.system(sys.to_string());
        }

        let options = GenerationOptions::default()
            .temperature(self.temperature)
            .top_p(self.top_p);
        request = request.options(options);

        let response =
            self.client.generate(request).await.map_err(|e| {
                QAError::OllamaUnavailable(format!("Ollama streaming failed: {}", e))
            })?;

        Ok(once(Ok(response.response)))
    }

    /// Check if Ollama is available
    ///
    /// # Returns
    /// True if Ollama is reachable and the model is available
    pub async fn health_check(&self) -> bool {
        // Try to list models
        match self.client.list_local_models().await {
            Ok(models) => {
                // Check if our model is available
                models.iter().any(|m| m.name == self.model)
            }
            Err(_) => false,
        }
    }

    /// Get model name
    pub fn model_name(&self) -> &str {
        &self.model
    }
}
