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
//! ## Kept as shared (redirects retained)
//!
//! - `crate::infrastructure::llm` (entire engine — clients, catalog,
//!   inference, system) registered via `infrastructure/mod.rs` #[path]
//!   redirect. Consumed by model_management, DI, and others.
//!
//! No port (`LLMPort` stays in `application/ports/`). No plugin.

pub mod commands;
pub mod di;
pub mod dto;
pub mod use_cases;
