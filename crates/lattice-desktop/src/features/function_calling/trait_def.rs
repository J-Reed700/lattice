use async_trait::async_trait;
use lattice_core::application::contracts::settings::CustomToolSettingsDto;
use lattice_core::application::contracts::tools::{
    FunctionCall, FunctionResult, RegistryStats, ToolDefinition,
};
use lattice_core::shared::error::Result;
use std::collections::HashMap;

#[async_trait]
pub trait FunctionRegistryTrait: Send + Sync {
    /// Register a new function/tool.
    fn register(&self, tool: ToolDefinition) -> Result<()>;

    /// Unregister a function/tool by name. Returns true if removed.
    fn unregister(&self, name: &str) -> bool;

    /// Record a function call for usage statistics.
    fn record_call(&self, function_name: &str);

    /// Get a tool definition by name.
    fn get_tool(&self, name: &str) -> Option<ToolDefinition>;

    /// List all registered tools.
    fn list_tools(&self) -> Vec<ToolDefinition>;

    /// Get registry statistics.
    fn stats(&self) -> RegistryStats;
}

/// Executes function calls from LLMs by routing to registered handlers.
#[async_trait]
pub trait FunctionExecutorTrait: Send + Sync {
    /// Execute a function call.
    async fn execute(&self, call: FunctionCall) -> Result<FunctionResult>;

    /// Replace currently configured custom tools at runtime so settings updates take effect without restart.
    fn set_custom_tools(&self, custom_tools: HashMap<String, CustomToolSettingsDto>);

    /// Validate function arguments against schema.
    fn validate_arguments(&self, function_name: &str, arguments: &serde_json::Value) -> Result<()>;
}
