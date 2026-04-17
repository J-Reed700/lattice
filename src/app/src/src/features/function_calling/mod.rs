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
//!
//! `trait_def` (FunctionServiceTrait) and `mocks` (MockFunctionService)
//! remain loaded via the shared `infrastructure::services::{traits,mocks}`
//! aggregators.

pub mod commands;
pub mod domain;
pub mod dto;
pub mod executor;
pub mod plugin;
pub mod registry;
pub mod use_cases;
