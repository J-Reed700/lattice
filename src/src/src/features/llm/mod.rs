//! # LLM feature
//!
//! Local + remote LLM inference: clients (Ollama HTTP, llama.cpp via
//! mistral.rs local), circuit breaker, model catalog, system
//! capabilities (GPU/CPU detection), and model download orchestration.
//!
//! ## Public surface
//!
//! - `crate::features::llm::dto` — LLM DTOs (DownloadModelRequestDto, etc.)
//! - `crate::features::llm::use_cases` — LLM use cases
//! - `crate::features::llm::commands` — Tauri command handlers
//!
//! ## Engine
//!
//! - `crate::features::llm::engine` — clients, catalog, inference, and
//!   system capabilities. Owned by this feature; consumed by
//!   model_management, DI, and others.
//!
//! No port (`LLMPort` stays in `application/ports/`). No plugin.

pub mod commands;
pub mod di;
pub mod dto;
pub mod use_cases;

pub mod cloud;
pub mod engine;
pub mod llama_cpp;
