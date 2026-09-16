//! # Function calling feature
//!
//! LLM tool/function calling: define available tools, route LLM function
//! requests to Rust implementations, return results back to the LLM.
//! Self-contained vertical slice.
//!
//! ## Public surface
//!
//! - `crate::features::function_calling::domain` — FunctionCall, FunctionResult, etc.
//! - `crate::features::function_calling::dto` — function-calling DTOs
//!   (CleanArticle, UrlPreview, WebSearchOutput, etc.)
//! - `crate::features::function_calling::use_cases` — function-calling use cases
//! - `crate::features::function_calling::executor` — FunctionExecutor
//! - `crate::features::function_calling::registry` — FunctionRegistry
//! - `crate::features::function_calling::commands` — Tauri command handlers
//! - `crate::features::function_calling::plugin::init()` — Tauri plugin
//! - `crate::features::function_calling::{FunctionRegistryTrait, FunctionExecutorTrait}` — service traits

pub mod commands;
pub mod di;
pub mod domain;
pub mod dto;
pub mod executor;
pub mod plugin;
pub mod registry;
pub mod trait_def;
pub mod use_cases;

#[cfg(test)]
pub mod mocks;

pub use trait_def::{FunctionExecutorTrait, FunctionRegistryTrait};
