//! LLM tool and function calling registry and executor.

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
