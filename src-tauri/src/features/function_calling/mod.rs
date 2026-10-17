//! # Function calling feature
//!
//! LLM tool/function calling: define available tools, route LLM function
//! requests to Rust implementations, return results back to the LLM.
//! Self-contained vertical slice.
//!
//! ## Public surface
//!
//! The tool vocabulary (`ToolDefinition`, `FunctionCall`, `FunctionResult`,
//! `RegistryStats` and the built-in tools' input and output records) lives in
//! `lattice_core::application::contracts::tools`; the web tools' records in
//! `lattice_core::application::contracts::web`.
//!
//! - `crate::features::function_calling::use_cases` — function-calling use cases
//! - `crate::features::function_calling::executor` — FunctionExecutor
//! - `crate::features::function_calling::registry` — FunctionRegistry
//! - `crate::features::function_calling::commands` — Tauri command handlers
//! - `crate::features::function_calling::plugin::init()` — Tauri plugin
//! - `crate::features::function_calling::{FunctionRegistryTrait, FunctionExecutorTrait}` — service traits

pub mod commands;
pub mod di;
pub mod executor;
pub mod plugin;
pub mod registry;
pub mod trait_def;
pub mod use_cases;

pub use trait_def::{FunctionExecutorTrait, FunctionRegistryTrait};

#[cfg(test)]
pub mod mocks;

/// Temporary: chat's test runtime still names the tool vocabulary at this path.
#[cfg(test)]
pub(crate) mod domain {
    pub(crate) use lattice_core::application::contracts::tools::{
        FunctionCall, FunctionResult, ToolDefinition,
    };
}
