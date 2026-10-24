//! LLM function calling commands.

use crate::interfaces::di::Container;
use lattice_core::application::contracts::tools::{FunctionCall, FunctionResult};
use lattice_core::shared::error::Result;
use tauri::State;
use tracing::{debug, error, info};

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

async fn execute_function_impl(
    container: &Container,
    call: FunctionCall,
) -> Result<FunctionResult> {
    info!("Executing function: name='{}', id='{}'", call.name, call.id);

    let validated_name = container
        .security_context()
        .input_validator()
        .validate_search_query(&call.name)?;

    let executor = container.function_executor();
    let result = executor.execute(call.clone()).await?;

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
