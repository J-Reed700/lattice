//! # LLM feature
//!
//! Local and remote LLM inference: the engine (clients, catalog, system
//! capabilities), the scheduler in front of each backend, the instance pool
//! of every loaded model, and model download orchestration. `LLMPort` itself
//! lives in `application/ports/`.

pub(crate) mod activity;
pub mod commands;
pub mod di;
pub mod dto;
pub(crate) mod instances;
pub(crate) mod loading;
pub mod use_cases;

pub mod cloud;
pub mod engine;
pub mod llama_cpp;
pub(crate) mod scheduler;
