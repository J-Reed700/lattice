//! Local inference engine module.
//!
//! Provides GPU-accelerated text generation using llama.cpp with GGUF models.
//! Supports both streaming and non-streaming generation with configurable
//! GPU offloading.

pub mod config;
pub mod engine;
pub mod loader;

pub use config::InferenceConfig;
pub use engine::InferenceEngine;
pub use loader::ModelLoader;
