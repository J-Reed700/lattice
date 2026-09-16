//! Execute Function Use Case
//!
//! Executes an LLM function call by routing to the appropriate handler.
//!
//! # Purpose
//!
//! Central orchestration point for function execution. Validates the function call,
//! routes to the appropriate implementation, and returns a standardized result.
//!
//! # Business Logic
//!
//! 1. Validate function name exists in registry
//! 2. Validate arguments against function's JSON schema
//! 3. Route to appropriate handler based on function name
//! 4. Execute the function
//! 5. Return standardized result (success or error)
//!
//! # Example
//!
//! ```rust
//! let request = ExecuteFunctionRequestDto {
//!     function_call: FunctionCallDto {
//!         id: "call_123".into(),
//!         name: "semantic_search".into(),
//!         arguments: json!({"query": "machine learning", "limit": 5}),
//!     },
//! };
//!
//! let use_case = ExecuteFunctionUseCase::new(registry, executor);
//! let response = use_case.execute(request).await?;
//!
//! assert!(response.result.success);
//! ```

use crate::features::function_calling::domain::{FunctionCall, FunctionResult};
use crate::features::function_calling::{FunctionExecutorTrait, FunctionRegistryTrait};
use crate::shared::result::Result;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Request to execute a function.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecuteFunctionRequestDto {
    /// The function call to execute
    pub function_call: FunctionCallDto,
}

/// Function call DTO.
///
/// Represents a request from the LLM to execute a specific function.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionCallDto {
    /// Unique identifier for this call
    pub id: String,

    /// Name of the function to execute
    pub name: String,

    /// Function arguments as JSON
    pub arguments: serde_json::Value,
}

impl FunctionCallDto {
    /// Convert DTO to domain FunctionCall
    pub fn to_domain(self) -> FunctionCall {
        FunctionCall::new(self.id, self.name, self.arguments)
    }
}

/// Response from function execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecuteFunctionResponseDto {
    /// The execution result
    pub result: FunctionResultDto,
}

/// Function execution result DTO.
///
/// Contains either success data or error information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionResultDto {
    /// Whether the function executed successfully
    pub success: bool,

    /// Result data (if success = true)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,

    /// Error message (if success = false)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
}

impl FunctionResultDto {
    /// Convert domain FunctionResult to DTO
    pub fn from_domain(result: FunctionResult) -> Self {
        Self {
            success: result.success,
            data: result.data,
            error_message: result.error_message,
        }
    }
}

/// Execute function use case.
///
/// Orchestrates function call validation and execution.
pub struct ExecuteFunctionUseCase {
    /// Function registry for validation
    function_registry: Arc<dyn FunctionRegistryTrait>,

    /// Function executor for execution
    function_executor: Arc<dyn FunctionExecutorTrait>,
}

impl ExecuteFunctionUseCase {
    /// Create a new instance of the use case.
    ///
    /// # Arguments
    ///
    /// * `function_registry` - Function registry for validation
    /// * `function_executor` - Function executor for execution
    pub fn new(
        function_registry: Arc<dyn FunctionRegistryTrait>,
        function_executor: Arc<dyn FunctionExecutorTrait>,
    ) -> Self {
        Self {
            function_registry,
            function_executor,
        }
    }

    /// Execute a function call.
    ///
    /// # Arguments
    ///
    /// * `request` - Function call request with name and arguments
    ///
    /// # Returns
    ///
    /// Function execution result (always returns Ok, errors are in the result DTO)
    ///
    /// # Errors
    ///
    /// Only returns Err for critical system failures. Function-level errors
    /// are returned as success=false in the response DTO.
    pub async fn execute(
        &self,
        request: ExecuteFunctionRequestDto,
    ) -> Result<ExecuteFunctionResponseDto> {
        // 1. Validate function exists
        let function_exists = self
            .function_registry
            .get_tool(&request.function_call.name)
            .is_some();

        if !function_exists {
            return Ok(ExecuteFunctionResponseDto {
                result: FunctionResultDto {
                    success: false,
                    data: None,
                    error_message: Some(format!(
                        "Unknown function: {}. Available functions can be listed using list_available_functions.",
                        request.function_call.name
                    )),
                },
            });
        }

        // 2. Validate arguments against schema
        if let Err(validation_error) = self.function_executor.validate_arguments(
            &request.function_call.name,
            &request.function_call.arguments,
        ) {
            return Ok(ExecuteFunctionResponseDto {
                result: FunctionResultDto {
                    success: false,
                    data: None,
                    error_message: Some(format!("Invalid arguments: {}", validation_error)),
                },
            });
        }

        // 3. Convert DTO to domain and execute
        let function_call = request.function_call.to_domain();

        match self.function_executor.execute(function_call).await {
            Ok(result) => Ok(ExecuteFunctionResponseDto {
                result: FunctionResultDto::from_domain(result),
            }),
            Err(e) => Ok(ExecuteFunctionResponseDto {
                result: FunctionResultDto {
                    success: false,
                    data: None,
                    error_message: Some(format!("Execution failed: {}", e)),
                },
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::function_calling::domain::ToolDefinition;
    use crate::features::function_calling::mocks::MockFunctionExecutor;
    use crate::features::function_calling::registry::FunctionRegistry;
    use serde_json::json;

    #[tokio::test]
    async fn test_execute_function_success() {
        let registry = FunctionRegistry::new();
        let tool = ToolDefinition::new(
            "test_function",
            "A test function",
            json!({"type": "object", "properties": {"param": {"type": "string"}}, "required": ["param"]}),
        )
        .unwrap();
        registry.register(tool).unwrap();

        let executor = MockFunctionExecutor::new();
        executor.set_response(
            "test_function",
            FunctionResult::success(json!({"result": "success"})),
        );

        let use_case = ExecuteFunctionUseCase::new(Arc::new(registry), Arc::new(executor));

        let request = ExecuteFunctionRequestDto {
            function_call: FunctionCallDto {
                id: "call_123".into(),
                name: "test_function".into(),
                arguments: json!({"param": "value"}),
            },
        };

        let response = use_case.execute(request).await.unwrap();

        assert!(response.result.success);
        assert!(response.result.data.is_some());
        assert_eq!(response.result.data.unwrap()["result"], "success");
    }

    #[tokio::test]
    async fn test_execute_function_unknown_function() {
        let registry = FunctionRegistry::new();
        let executor = MockFunctionExecutor::new();
        let use_case = ExecuteFunctionUseCase::new(Arc::new(registry), Arc::new(executor));

        let request = ExecuteFunctionRequestDto {
            function_call: FunctionCallDto {
                id: "call_123".into(),
                name: "nonexistent_function".into(),
                arguments: json!({}),
            },
        };

        let response = use_case.execute(request).await.unwrap();

        assert!(!response.result.success);
        assert!(response.result.error_message.is_some());
        assert!(response
            .result
            .error_message
            .unwrap()
            .contains("Unknown function"));
    }

    #[tokio::test]
    async fn test_execute_function_invalid_arguments() {
        let registry = FunctionRegistry::new();
        let tool = ToolDefinition::new(
            "test_function",
            "A test function",
            json!({"type": "object", "properties": {"required_param": {"type": "string"}}, "required": ["required_param"]}),
        )
        .unwrap();
        registry.register(tool).unwrap();

        let executor = MockFunctionExecutor::new();
        executor.set_validation_error(
            "test_function",
            "Missing required parameter: required_param",
        );

        let use_case = ExecuteFunctionUseCase::new(Arc::new(registry), Arc::new(executor));

        let request = ExecuteFunctionRequestDto {
            function_call: FunctionCallDto {
                id: "call_123".into(),
                name: "test_function".into(),
                arguments: json!({}), // Missing required_param
            },
        };

        let response = use_case.execute(request).await.unwrap();

        assert!(!response.result.success);
        assert!(response.result.error_message.is_some());
        assert!(response
            .result
            .error_message
            .unwrap()
            .contains("Invalid arguments"));
    }

    #[tokio::test]
    async fn test_execute_function_execution_error() {
        let registry = FunctionRegistry::new();
        let tool = ToolDefinition::new(
            "test_function",
            "A test function",
            json!({"type": "object", "properties": {}}),
        )
        .unwrap();
        registry.register(tool).unwrap();

        let executor = MockFunctionExecutor::new();
        executor.set_response(
            "test_function",
            FunctionResult::error("EXECUTION_ERROR", "Function execution failed"),
        );

        let use_case = ExecuteFunctionUseCase::new(Arc::new(registry), Arc::new(executor));

        let request = ExecuteFunctionRequestDto {
            function_call: FunctionCallDto {
                id: "call_123".into(),
                name: "test_function".into(),
                arguments: json!({}),
            },
        };

        let response = use_case.execute(request).await.unwrap();

        assert!(!response.result.success);
        assert!(response.result.error_message.is_some());
    }
}
