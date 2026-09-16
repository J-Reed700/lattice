pub mod engine;
pub mod ollama;
pub mod prompts;
pub mod tokenizer;
/// Q&A Module - RAG-based question answering with Ollama
///
/// This module provides a complete RAG (Retrieval-Augmented Generation) pipeline
/// for answering questions using a local knowledge base and Ollama LLM.
pub mod types;

pub use engine::QAEngine;
pub use ollama::OllamaClient;
pub use prompts::{build_user_prompt, SYSTEM_PROMPT};
pub use tokenizer::{count_tokens, truncate_to_tokens};
pub use types::{QAError, SourceReference, StreamChunk};
