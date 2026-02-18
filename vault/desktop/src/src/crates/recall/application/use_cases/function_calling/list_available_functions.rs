//! List Available Functions Use Case
//!
//! Returns all registered LLM-callable functions with their schemas.
//!
//! # Purpose
//!
//! Provides the LLM with a complete catalog of available tools it can invoke.
//! Functions are returned in OpenAI-compatible format for tool calling.
//!
//! # Business Logic
//!
//! 1. Retrieve all registered tools from function registry
//! 2. Transform domain models (ToolDefinition) to DTOs
//! 3. Return function definitions in OpenAI tool format
//!
//! # Example
//!
//! ```rust
//! let use_case = ListAvailableFunctionsUseCase::new(registry);
//! let response = use_case.execute().await?;
//!
//! // response.functions contains all 5 registered tools:
//! // - semantic_search
//! // - get_document
//! // - list_documents
//! // - web_search
//! // - fetch_url_content
//! ```

use crate::domain::function_call::ToolDefinition;
use crate::infrastructure::services::traits::FunctionRegistryTrait;
use crate::shared::result::Result;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

// =============================================================================
// DTOs
// =============================================================================

/// Available functions response DTO.
///
/// Contains all registered functions with their metadata and schemas.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AvailableFunctionsDto {
    /// List of available functions
    pub functions: Vec<FunctionDefinitionDto>,

    /// Total number of functions
    pub total: usize,
}

/// Function definition DTO (OpenAI tools format).
///
/// Represents a single callable function with its schema.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionDefinitionDto {
    /// Function name (e.g., "semantic_search")
    pub name: String,

    /// Human-readable description of what the function does
    pub description: String,

    /// JSON Schema for input parameters
    pub input_schema: serde_json::Value,
}

impl FunctionDefinitionDto {
    /// Convert domain ToolDefinition to DTO
    pub fn from_domain(tool: ToolDefinition) -> Self {
        Self {
            name: tool.name,
            description: tool.description,
            input_schema: tool.input_schema,
        }
    }
}

// =============================================================================
// Use Case
// =============================================================================

/// List available functions use case.
///
/// Returns all registered LLM-callable functions with their schemas.
pub struct ListAvailableFunctionsUseCase {
    /// Function registry port
    function_registry: Arc<dyn FunctionRegistryTrait>,
}

impl ListAvailableFunctionsUseCase {
    /// Create a new instance of the use case.
    ///
    /// # Arguments
    ///
    /// * `function_registry` - Function registry port for retrieving registered tools
    pub fn new(function_registry: Arc<dyn FunctionRegistryTrait>) -> Self {
        Self { function_registry }
    }

    /// Execute the use case.
    ///
    /// # Returns
    ///
    /// Available functions DTO with all registered tools.
    ///
    /// # Errors
    ///
    /// This use case should not fail under normal circumstances.
    /// Returns empty list if no functions are registered.
    pub async fn execute(&self) -> Result<AvailableFunctionsDto> {
        // 1. Get all registered functions from registry
        let tools = self.function_registry.list_tools();

        // 2. Transform to DTOs (OpenAI format)
        let function_dtos: Vec<FunctionDefinitionDto> = tools
            .into_iter()
            .map(FunctionDefinitionDto::from_domain)
            .collect();

        // 3. Return response
        let total = function_dtos.len();
        Ok(AvailableFunctionsDto {
            functions: function_dtos,
            total,
        })
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::services::function_registry::FunctionRegistry;
    use serde_json::json;

    #[tokio::test]
    async fn test_list_functions_returns_all_registered() {
        // Arrange
        let registry = FunctionRegistry::new();

        // Register test functions
        let tool1 = ToolDefinition::new(
            "search",
            "Semantic search",
            json!({"type": "object", "properties": {"query": {"type": "string"}}, "required": ["query"]}),
        )
        .unwrap();

        let tool2 = ToolDefinition::new(
            "fetch_url",
            "Fetch URL content",
            json!({"type": "object", "properties": {"url": {"type": "string"}}, "required": ["url"]}),
        )
        .unwrap();

        registry.register(tool1).unwrap();
        registry.register(tool2).unwrap();

        let use_case = ListAvailableFunctionsUseCase::new(Arc::new(registry));

        // Act
        let result = use_case.execute().await;

        // Assert
        assert!(result.is_ok());
        let response = result.unwrap();
        assert_eq!(response.total, 2);
        assert_eq!(response.functions.len(), 2);

        // Verify function names
        let names: Vec<&str> = response.functions.iter().map(|f| f.name.as_str()).collect();
        assert!(names.contains(&"search"));
        assert!(names.contains(&"fetch_url"));
    }

    #[tokio::test]
    async fn test_list_functions_correct_schema_format() {
        // Arrange
        let registry = FunctionRegistry::new();

        let tool = ToolDefinition::new(
            "semantic_search",
            "Search documents using semantic similarity",
            json!({
                "type": "object",
                "properties": {
                    "query": {"type": "string"},
                    "limit": {"type": "integer", "default": 10}
                },
                "required": ["query"]
            }),
        )
        .unwrap();

        registry.register(tool).unwrap();
        let use_case = ListAvailableFunctionsUseCase::new(Arc::new(registry));

        // Act
        let result = use_case.execute().await.unwrap();

        // Assert
        assert_eq!(result.functions.len(), 1);
        let func = &result.functions[0];

        assert_eq!(func.name, "semantic_search");
        assert_eq!(
            func.description,
            "Search documents using semantic similarity"
        );

        // Verify schema structure
        assert_eq!(func.input_schema["type"], "object");
        assert!(func.input_schema["properties"].is_object());
        assert!(func.input_schema["required"].is_array());
    }

    #[tokio::test]
    async fn test_list_functions_empty_registry() {
        // Arrange
        let registry = FunctionRegistry::new();
        let use_case = ListAvailableFunctionsUseCase::new(Arc::new(registry));

        // Act
        let result = use_case.execute().await;

        // Assert
        assert!(result.is_ok());
        let response = result.unwrap();
        assert_eq!(response.total, 0);
        assert!(response.functions.is_empty());
    }

    #[tokio::test]
    async fn test_function_dto_from_domain() {
        // Arrange
        let tool = ToolDefinition::new(
            "test_function",
            "A test function",
            json!({"type": "object", "properties": {}}),
        )
        .unwrap();

        // Act
        let dto = FunctionDefinitionDto::from_domain(tool);

        // Assert
        assert_eq!(dto.name, "test_function");
        assert_eq!(dto.description, "A test function");
        assert_eq!(dto.input_schema["type"], "object");
    }
}
