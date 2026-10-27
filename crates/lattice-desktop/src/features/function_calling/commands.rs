//! LLM Function Calling Commands
//!
//! Thin command controllers for LLM function calling operations following DDD pattern.
//! Enables LLMs to call registered functions (search, web access, etc.) with comprehensive
//! input validation and audit logging.
//!
//! # Commands (3 total)
//!
//! - `list_available_functions` - Get all registered tool definitions
//! - `execute_function` - Execute LLM function call with security checks
//! - `get_function_stats` - Get function registry usage statistics
//!
//! # Function Registry
//!
//! Available functions for LLM use:
//! - **semantic_search**: Vector similarity search across documents
//! - **get_document**: Retrieve specific document by ID
//! - **list_documents**: List available documents
//! - **web_search**: Search the web
//! - **fetch_url_content**: Fetch content from URL
//!
//! # Security Features
//!
//! - **Input Validation**: Function names and arguments validated
//! - **Audit Logging**: All executions logged with success/failure
//!
//! # Architecture
//!
//! Commands delegate to:
//! - `FunctionRegistry` - Function metadata and tool definitions
//! - `FunctionExecutor` - Function execution with validation

use crate::interfaces::di::Container;
use lattice_core::application::contracts::tools::{FunctionCall, FunctionResult};
use lattice_core::shared::error::Result;
use tauri::State;
use tracing::{debug, error, info};

/// List all available tool definitions for LLM function calling
///
/// Returns complete metadata for all registered functions including names, descriptions, and
/// input schemas. Used by LLM integrations to discover available tools and by frontend to
/// display function registry.
///
/// # Tool Schema Format
///
/// Each tool includes:
/// - **name**: Function identifier (e.g., "semantic_search")
/// - **description**: Human-readable function description
/// - **input_schema**: JSON Schema for function arguments
///
/// # Security
///
/// **Input Validation**: Function schemas are validated before execution
///
/// # Architecture
///
/// Thin controller delegating to `FunctionRegistry::list_tools()`
async fn list_available_functions_impl(
    container: &Container,
) -> Result<Vec<lattice_core::application::contracts::tools::ToolDefinition>> {
    debug!("Listing available functions");

    let registry = container.function_registry();

    let tools = registry.list_tools();

    info!("Listed {} available functions", tools.len());

    Ok(tools)
}

pub async fn list_available_functions(
    container: State<'_, Container>,
) -> Result<Vec<lattice_core::application::contracts::tools::ToolDefinition>> {
    list_available_functions_impl(&container).await
}

/// Execute LLM function call with validation
///
/// Executes a function call from LLM with input validation and audit logging.
/// Returns success/failure result with data or error message.
///
/// # Security
///
/// **Input Validation**: Function names and arguments validated
/// **Audit Logging**: All executions logged with success/failure
///
/// # Architecture
///
/// Thin controller delegating to `FunctionExecutor`
async fn execute_function_impl(
    container: &Container,
    call: FunctionCall,
) -> Result<FunctionResult> {
    info!("Executing function: name='{}', id='{}'", call.name, call.id);

    // Input validation - check function name
    let validated_name = container
        .security_context()
        .input_validator()
        .validate_search_query(&call.name)?; // Reuse search validator for function names

    let executor = container.function_executor();

    let result = executor.execute(call.clone()).await?;

    // Audit logging
    if result.success {
        info!("Function '{}' executed successfully", validated_name);
    } else {
        error!(
            "Function '{}' failed: {:?}",
            validated_name, result.error_message
        );
    }

    Ok(result)
}

pub async fn execute_function(
    container: State<'_, Container>,
    call: FunctionCall,
) -> Result<FunctionResult> {
    execute_function_impl(&container, call).await
}

/// Get function registry usage statistics with metadata
///
/// Returns usage statistics for the function registry including total registered functions,
/// execution counts, success/failure rates, and performance metrics. Useful for monitoring,
/// debugging, and displaying function registry health in admin dashboards.
///
/// # Statistics Included
///
/// - **totalFunctions**: Number of registered functions in registry
/// - **executionCount**: Total number of function executions
/// - **successCount**: Number of successful executions
/// - **failureCount**: Number of failed executions
/// - **averageExecutionTimeMs**: Mean execution time across all functions
///
/// # Security
///
/// **Read-only stats lookup**: in-memory registry counters only
///
/// # Architecture
///
/// Thin controller delegating to `FunctionRegistry::stats()`
pub async fn get_function_stats(
    container: State<'_, Container>,
) -> Result<lattice_core::application::contracts::tools::RegistryStats> {
    debug!("Getting function registry statistics");

    let registry = container.function_registry();

    let stats = registry.stats();

    info!(
        "Retrieved function stats: {} functions",
        stats.total_functions
    );

    Ok(stats)
}

#[cfg(all(test, feature = "test-utils"))]
mod tests {
    use super::*;
    use crate::tests::common::setup_test_container;

    async fn create_test_container() -> Result<Container> {
        setup_test_container().await
    }

    #[tokio::test]
    async fn test_list_available_functions() -> Result<()> {
        let container = create_test_container().await?;

        let result = list_available_functions_impl(&container).await?;

        assert!(result.iter().any(|tool| tool.name == "semantic_search"));
        assert!(result.iter().any(|tool| tool.name == "fetch_url_content"));
        Ok(())
    }

    #[tokio::test]
    async fn test_execute_function() -> Result<()> {
        let container = create_test_container().await?;

        let call = FunctionCall::new("call_123", "list_documents", serde_json::json!({}));

        let function_result = execute_function_impl(&container, call).await?;

        assert!(function_result.success); // Mock executor always succeeds
        Ok(())
    }
}
