//! Mock implementations for testing
//!
//! This module provides mock implementations of service traits.

#[cfg(test)]
use crate::application::dtos::settings::CustomToolSettingsDto;
#[cfg(test)]
use crate::domain::function_call::{FunctionCall, FunctionResult, RegistryStats, ToolDefinition};
#[cfg(test)]
use crate::infrastructure::services::traits::*;
#[cfg(test)]
use crate::shared::error::Result;
#[cfg(test)]
use async_trait::async_trait;
#[cfg(test)]
use std::collections::HashMap;
#[cfg(test)]
use std::sync::{Arc, RwLock};

#[cfg(test)]
/// Mock function registry for testing
pub struct MockFunctionRegistry {
    tools: Arc<RwLock<HashMap<String, ToolDefinition>>>,
    stats: Arc<RwLock<RegistryStats>>,
}

#[cfg(test)]
impl MockFunctionRegistry {
    pub fn new() -> Self {
        Self {
            tools: Arc::new(RwLock::new(HashMap::new())),
            stats: Arc::new(RwLock::new(RegistryStats::default())),
        }
    }

    pub fn with_tools(tools: Vec<ToolDefinition>) -> Result<Self> {
        let registry = Self::new();
        for tool in tools {
            registry.register(tool)?;
        }
        Ok(registry)
    }
}

#[cfg(test)]
impl Default for MockFunctionRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
impl FunctionRegistryTrait for MockFunctionRegistry {
    fn register(&self, tool: ToolDefinition) -> Result<()> {
        let mut tools = self.tools.write().unwrap();
        if tools.contains_key(&tool.name) {
            return Err(crate::error::AppError::InvalidData(format!(
                "Tool '{}' already registered",
                tool.name
            )));
        }
        tools.insert(tool.name.clone(), tool);

        let mut stats = self.stats.write().unwrap();
        stats.total_functions = tools.len();

        Ok(())
    }

    fn get_tool(&self, name: &str) -> Option<ToolDefinition> {
        self.tools.read().unwrap().get(name).cloned()
    }

    fn list_tools(&self) -> Vec<ToolDefinition> {
        self.tools.read().unwrap().values().cloned().collect()
    }

    fn stats(&self) -> RegistryStats {
        self.stats.read().unwrap().clone()
    }
}

#[cfg(test)]
/// Mock function executor for testing
pub struct MockFunctionExecutor {
    responses: Arc<RwLock<HashMap<String, FunctionResult>>>,
    validation_errors: Arc<RwLock<HashMap<String, String>>>,
}

#[cfg(test)]
impl MockFunctionExecutor {
    pub fn new() -> Self {
        Self {
            responses: Arc::new(RwLock::new(HashMap::new())),
            validation_errors: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Set a canned response for a function call
    pub fn set_response(&self, function_name: impl Into<String>, result: FunctionResult) {
        self.responses
            .write()
            .unwrap()
            .insert(function_name.into(), result);
    }

    /// Set a validation error for a function
    pub fn set_validation_error(&self, function_name: impl Into<String>, error: impl Into<String>) {
        self.validation_errors
            .write()
            .unwrap()
            .insert(function_name.into(), error.into());
    }
}

#[cfg(test)]
impl Default for MockFunctionExecutor {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
#[cfg(test)]
impl FunctionExecutorTrait for MockFunctionExecutor {
    async fn execute(&self, call: FunctionCall) -> Result<FunctionResult> {
        let responses = self.responses.read().unwrap();
        if let Some(result) = responses.get(&call.name) {
            Ok(result.clone())
        } else {
            Ok(FunctionResult::success(serde_json::json!({
                "mock_result": true,
                "function": call.name,
                "call_id": call.id
            })))
        }
    }

    fn set_custom_tools(&self, _custom_tools: HashMap<String, CustomToolSettingsDto>) {}

    fn validate_arguments(
        &self,
        function_name: &str,
        _arguments: &serde_json::Value,
    ) -> Result<()> {
        let validation_errors = self.validation_errors.read().unwrap();
        if let Some(error) = validation_errors.get(function_name) {
            Err(crate::shared::error::AppError::InvalidInput(error.clone()))
        } else {
            Ok(())
        }
    }
}
