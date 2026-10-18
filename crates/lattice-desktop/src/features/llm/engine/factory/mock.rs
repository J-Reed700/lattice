//! The fallback port that answers without a model, for tests and for an app
//! with no backend configured.

use async_trait::async_trait;
use tracing::warn;

use lattice_core::application::ports::llm_port::{
    CompletionInput, CompletionRequest, CompletionResponse,
};
use lattice_core::application::ports::LLMPort;
use lattice_core::shared::error::Result;

/// Mock LLM port for testing and fallback.
///
/// Returns fixed responses without actual LLM inference.
pub struct MockLLMPort {
    model_name: String,
    vision: Option<lattice_core::domain::models::vision::VisionGeometry>,
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
            vision: None,
        }
    }

    /// A mock declared to see with `vision`.
    pub fn with_vision(
        mut self,
        vision: lattice_core::domain::models::vision::VisionGeometry,
    ) -> Self {
        self.vision = Some(vision);
        self
    }
}

#[async_trait]
impl LLMPort for MockLLMPort {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        warn!("Using mock LLM - returning intelligent response");

        let prompt = match request.input.last() {
            Some(CompletionInput::Message { content, .. }) => content.as_str(),
            _ => "",
        };
        let context = request.input.len().saturating_sub(1);
        let response = if context > 0 {
            // If context is provided, acknowledge it
            format!(
                "Based on the provided context, I can help answer your question about: {}. \
                However, I'm currently running in mock mode as no LLM backend is available. \
                To enable full AI capabilities, please either:\n\n\
                1. Start Ollama with a model (e.g., `ollama run llama3.1:8b`), or\n\
                2. Download a local model from Settings → Models\n\n\
                Your question: {}\n\nContext references: {} document(s) available",
                prompt, prompt, context
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

        Ok(CompletionResponse::from_text(response))
    }

    fn model_name(&self) -> &str {
        &self.model_name
    }

    fn vision(&self) -> Option<lattice_core::domain::models::vision::VisionGeometry> {
        self.vision
    }

    fn max_context_tokens(&self) -> usize {
        4096
    }

    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }

    fn provider_name(&self) -> &str {
        "mock"
    }
}
