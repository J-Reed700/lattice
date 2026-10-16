//! Service trait definitions
//!
//! This module defines trait interfaces for dependency injection.

use crate::features::function_calling::domain::{
    FunctionCall, FunctionResult, RegistryStats, ToolDefinition,
};
use crate::features::settings::dto::CustomToolSettingsDto;
use async_trait::async_trait;
use lattice_core::shared::error::Result;
use std::collections::HashMap;

#[async_trait]
pub trait FunctionRegistryTrait: Send + Sync {
    /// Register a new function/tool
    ///
    /// # Arguments
    /// * `tool` - Tool definition with metadata and parameter schema
    ///
    /// # Errors
    /// Returns error if tool name already exists or tool is invalid
    fn register(&self, tool: ToolDefinition) -> Result<()>;

    /// Unregister a function/tool by name
    ///
    /// # Arguments
    /// * `name` - Tool name to remove
    ///
    /// # Returns
    /// True if the tool was present and removed, false otherwise
    fn unregister(&self, name: &str) -> bool;

    /// Record a function call for usage statistics
    ///
    /// # Arguments
    /// * `function_name` - Name of function that was called
    fn record_call(&self, function_name: &str);

    /// Get a tool definition by name
    ///
    /// # Arguments
    /// * `name` - Tool name to look up
    ///
    /// # Returns
    /// Tool definition if found, None otherwise
    fn get_tool(&self, name: &str) -> Option<ToolDefinition>;

    /// List all registered tools
    ///
    /// # Returns
    /// Vector of all tool definitions
    fn list_tools(&self) -> Vec<ToolDefinition>;

    /// Get registry statistics
    ///
    /// # Returns
    /// Registry stats with function counts and usage
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
    /// # Arguments
    /// * `call` - Function call with name and arguments
    ///
    /// # Returns
    /// Function result with data or error
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
    ///
    /// # Arguments
    /// * `function_name` - Name of function to validate
    /// * `arguments` - Arguments to validate
    ///
    /// # Returns
    /// Ok if valid, Err with validation message if invalid
    fn validate_arguments(&self, function_name: &str, arguments: &serde_json::Value) -> Result<()>;
}
