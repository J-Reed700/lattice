//! Service trait definitions
//!
//! This module defines trait interfaces for dependency injection.

use async_trait::async_trait;
use lattice_core::application::contracts::settings::CustomToolSettingsDto;
use lattice_core::application::contracts::tools::{
    FunctionCall, FunctionResult, RegistryStats, ToolDefinition,
};
use lattice_core::shared::error::Result;
use std::collections::HashMap;

#[async_trait]
pub trait FunctionRegistryTrait: Send + Sync {
    /// Register a new function/tool
    ///
    /// # Errors
    /// Returns error if tool name already exists or tool is invalid
    fn register(&self, tool: ToolDefinition) -> Result<()>;

    /// Unregister a function/tool by name
    fn unregister(&self, name: &str) -> bool;

    /// Record a function call for usage statistics
    fn record_call(&self, function_name: &str);

    /// Get a tool definition by name
    fn get_tool(&self, name: &str) -> Option<ToolDefinition>;

    /// List all registered tools
    fn list_tools(&self) -> Vec<ToolDefinition>;

    /// Get registry statistics
    fn stats(&self) -> RegistryStats;
}

/// Trait for function execution operations
///
/// Executes function calls from LLMs by routing to appropriate handlers.
/// Handles validation, execution, error handling, and result formatting.
///
/// # Implementations
/// - `FunctionExecutor`: Production implementation
/// - `MockFunctionExecutor`: Mock for testing
#[async_trait]
#[async_trait]
pub trait FunctionExecutorTrait: Send + Sync {
    /// Execute a function call
    ///
    /// # Errors
    /// Returns error if function not found or execution fails
    async fn execute(&self, call: FunctionCall) -> Result<FunctionResult>;

    /// Replace currently configured custom tools at runtime.
    ///
    /// Used when settings are updated so conversation tool-calling reflects
    /// newly-added or removed custom tools without restart.
    fn set_custom_tools(&self, custom_tools: HashMap<String, CustomToolSettingsDto>);

    /// Validate function arguments against schema
    fn validate_arguments(&self, function_name: &str, arguments: &serde_json::Value) -> Result<()>;
}
