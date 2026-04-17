//! # Function calling feature
//!
//! LLM tool/function calling: define available tools, route LLM function
//! requests to Rust implementations, return results back to the LLM.
//!
//! ## File layout
//!
//! | File            | Canonical module path                                              |
//! |-----------------|--------------------------------------------------------------------|
//! | `domain.rs`     | `crate::domain::function_call` (FunctionCall, FunctionResult, etc.) |
//! | `dto.rs`        | `crate::application::dtos::function_calling_dto`                   |
//! | `use_cases/`    | `crate::application::use_cases::function_calling`                  |
//! | `executor.rs`   | `crate::infrastructure::services::function_executor`               |
//! | `registry.rs`   | `crate::infrastructure::services::function_registry`               |
//! | `trait_def.rs`  | `crate::infrastructure::services::traits` (merged re-exports)      |
//! | `mocks.rs`      | `crate::infrastructure::services::mocks` (merged re-exports)       |
//! | `commands.rs`   | `crate::interfaces::commands::function_calling_commands`           |
//! | `plugin.rs`     | `crate::plugins::functions_plugin`                                 |
//!
//! No port today — the function registry is a concrete singleton.
