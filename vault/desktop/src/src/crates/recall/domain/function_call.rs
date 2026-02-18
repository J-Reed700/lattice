//! Function calling domain models
//!
//! Rich domain models for LLM function calling with business logic and invariants.
//!
//! # Architecture (Bricks and Studs)
//!
//! - **ToolDefinition**: Self-contained function metadata with validation
//! - **FunctionCall**: A request to execute a specific function
//! - **FunctionResult**: Execution outcome with success/error handling
//!
//! These models enforce business rules (e.g., valid tool names, parameter schemas).

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Tool (function) definition for LLM function calling
///
/// Represents a callable function with its metadata and parameter schema.
/// Enforces invariants:
/// - Tool name must be non-empty and alphanumeric + underscore
/// - Description must be clear and actionable
/// - Parameters must have valid JSON schema
///
/// # Example
/// ```rust
/// use serde_json::json;
///
/// let tool = ToolDefinition {
///     name: "semantic_search".to_string(),
///     description: "Search user's documents using semantic similarity".to_string(),
///     input_schema: json!({
///         "type": "object",
///         "properties": {
///             "query": {"type": "string"},
///             "limit": {"type": "integer", "default": 10}
///         },
///         "required": ["query"]
///     }),
/// };
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    /// Function name (alphanumeric + underscores)
    pub name: String,

    /// Human-readable description of what the function does
    pub description: String,

    /// JSON Schema for input parameters
    ///
    /// Should follow JSON Schema specification with:
    /// - type: "object"
    /// - properties: parameter definitions
    /// - required: list of required parameter names
    pub input_schema: serde_json::Value,
}

impl ToolDefinition {
    /// Create a new tool definition with validation
    ///
    /// # Arguments
    /// * `name` - Tool name (must be alphanumeric + underscores)
    /// * `description` - Clear description of functionality
    /// * `input_schema` - JSON Schema for parameters
    ///
    /// # Returns
    /// Validated ToolDefinition
    ///
    /// # Errors
    /// Returns error if name is invalid or schema is malformed
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        input_schema: serde_json::Value,
    ) -> Result<Self, String> {
        let name = name.into();
        let description = description.into();

        // Validate name (alphanumeric + underscores only)
        if name.is_empty() {
            return Err("Tool name cannot be empty".to_string());
        }

        if !name.chars().all(|c| c.is_alphanumeric() || c == '_') {
            return Err(format!(
                "Tool name '{}' must contain only alphanumeric characters and underscores",
                name
            ));
        }

        // Validate description
        if description.is_empty() {
            return Err("Tool description cannot be empty".to_string());
        }

        // Validate schema is an object
        if !input_schema.is_object() {
            return Err("Input schema must be a JSON object".to_string());
        }

        Ok(Self {
            name,
            description,
            input_schema,
        })
    }
}

/// A function call request from the LLM
///
/// Represents an LLM's intent to execute a specific function with arguments.
///
/// # Example
/// ```rust
/// use serde_json::json;
///
/// let call = FunctionCall {
///     id: "call_abc123".to_string(),
///     name: "semantic_search".to_string(),
///     arguments: json!({
///         "query": "machine learning papers",
///         "limit": 5
///     }),
/// };
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionCall {
    /// Unique identifier for this function call
    ///
    /// Used to correlate the call with its result in multi-turn conversations
    pub id: String,

    /// Name of the function to execute
    pub name: String,

    /// Function arguments as JSON value
    ///
    /// Structure should match the tool's input_schema
    pub arguments: serde_json::Value,
}

impl FunctionCall {
    /// Create a new function call
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        arguments: serde_json::Value,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            arguments,
        }
    }
}

/// Result of executing a function call
///
/// Encapsulates success or failure with detailed error information.
///
/// # Example (Success)
/// ```rust
/// use serde_json::json;
///
/// let result = FunctionResult::success(json!({
///     "results": ["doc1.pdf", "doc2.txt"],
///     "total_found": 2
/// }));
/// ```
///
/// # Example (Failure)
/// ```rust
/// let result = FunctionResult::error(
///     "DOCUMENT_NOT_FOUND",
///     "Document with ID abc123 not found"
/// );
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionResult {
    /// Whether the function executed successfully
    pub success: bool,

    /// Result data (if success = true)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,

    /// Error code (if success = false)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,

    /// Error message (if success = false)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
}

impl FunctionResult {
    /// Create a successful result
    pub fn success(data: serde_json::Value) -> Self {
        Self {
            success: true,
            data: Some(data),
            error_code: None,
            error_message: None,
        }
    }

    /// Create a failed result with error details
    pub fn error(error_code: impl Into<String>, error_message: impl Into<String>) -> Self {
        Self {
            success: false,
            data: None,
            error_code: Some(error_code.into()),
            error_message: Some(error_message.into()),
        }
    }

    /// Convert to JSON for returning to LLM
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or_else(|_| {
            serde_json::json!({
                "success": false,
                "error_code": "SERIALIZATION_ERROR",
                "error_message": "Failed to serialize function result"
            })
        })
    }
}

/// Function registry statistics
///
/// Tracks registered functions and usage metrics.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RegistryStats {
    /// Number of registered functions
    pub total_functions: usize,

    /// Function call counts (name -> count)
    pub call_counts: HashMap<String, u64>,
}

impl RegistryStats {
    /// Increment call count for a function
    pub fn record_call(&mut self, function_name: &str) {
        *self
            .call_counts
            .entry(function_name.to_string())
            .or_insert(0) += 1;
    }

    /// Get most frequently called function
    pub fn most_called_function(&self) -> Option<(&String, &u64)> {
        self.call_counts.iter().max_by_key(|(_, count)| *count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_tool_definition_validation() {
        // Valid tool
        let tool = ToolDefinition::new(
            "semantic_search",
            "Search documents",
            json!({"type": "object", "properties": {}}),
        );
        assert!(tool.is_ok());

        // Invalid: empty name
        let tool = ToolDefinition::new("", "Description", json!({"type": "object"}));
        assert!(tool.is_err());

        // Invalid: special characters in name
        let tool = ToolDefinition::new("search-docs", "Description", json!({"type": "object"}));
        assert!(tool.is_err());

        // Invalid: non-object schema
        let tool = ToolDefinition::new("search", "Description", json!("not an object"));
        assert!(tool.is_err());
    }

    #[test]
    fn test_function_result_success() {
        let result = FunctionResult::success(json!({"count": 5}));
        assert!(result.success);
        assert!(result.data.is_some());
        assert!(result.error_code.is_none());

        let json = result.to_json();
        assert_eq!(json["success"], true);
        assert_eq!(json["data"]["count"], 5);
    }

    #[test]
    fn test_function_result_error() {
        let result = FunctionResult::error("NOT_FOUND", "Resource not found");
        assert!(!result.success);
        assert!(result.data.is_none());
        assert_eq!(result.error_code.as_ref().unwrap(), "NOT_FOUND");

        let json = result.to_json();
        assert_eq!(json["success"], false);
        assert_eq!(json["error_code"], "NOT_FOUND");
    }

    #[test]
    fn test_registry_stats() {
        let mut stats = RegistryStats::default();
        stats.record_call("search");
        stats.record_call("search");
        stats.record_call("get_document");

        assert_eq!(stats.call_counts.get("search"), Some(&2));
        assert_eq!(stats.call_counts.get("get_document"), Some(&1));

        let (name, count) = stats.most_called_function().unwrap();
        assert_eq!(name, "search");
        assert_eq!(*count, 2);
    }
}
