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
//!
//! # Use Cases
//!
//! - **LLM Agents**: Enable Claude/GPT to search and access data
//! - **Tool Use**: Agentic workflows with function calling
//! - **Research Assistant**: LLM-driven research with tool access

use crate::application::dtos::function_calling_dto::*;
use crate::domain::function_call::{FunctionCall, FunctionResult, ToolDefinition};
use crate::interfaces::di::Container;
use crate::shared::error::Result;
use tauri::State;
use tracing::{debug, error, info};

/// List all available tool definitions for LLM function calling
///
/// Returns complete metadata for all registered functions including names, descriptions, and
/// input schemas. Used by LLM integrations to discover available tools and by frontend to
/// display function registry.
///
/// # Arguments
///
/// * `container` - Service container with function registry
///
/// # Returns
///
/// * `Ok(Vec<Value>)` - Array of tool definitions with schemas
/// * `Err(AppError)` - Registry access failed
///
/// # Example
///
/// ```typescript
/// import { invoke } from '@tauri-apps/api/core';
///
/// interface ToolDefinition {
///   name: string;
///   description: string;
///   inputSchema: {
///     type: 'object';
///     properties: Record<string, any>;
///     required: string[];
///   };
/// }
///
/// // List available functions
/// const tools = await invoke<ToolDefinition[]>('list_available_functions');
/// console.log(`${tools.length} functions available:`);
/// tools.forEach(tool => console.log(`- ${tool.name}: ${tool.description}`));
///
/// // Initialize LLM with tools
/// const initializeLLM = async () => {
///   const tools = await invoke<ToolDefinition[]>('list_available_functions');
///
///   return new Claude({
///     apiKey: process.env.ANTHROPIC_API_KEY,
///     tools: tools.map(t => ({
///       name: t.name,
///       description: t.description,
///       input_schema: t.inputSchema
///     }))
///   });
/// };
///
/// // Display function registry in UI
/// const displayFunctions = async () => {
///   const tools = await invoke<ToolDefinition[]>('list_available_functions');
///
///   return (
///     <div className="function-registry">
///       <h2>{tools.length} Available Functions</h2>
///       {tools.map(tool => (
///         <div key={tool.name}>
///           <h3>{tool.name}</h3>
///           <p>{tool.description}</p>
///         </div>
///       ))}
///     </div>
///   );
/// };
/// ```
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
/// # Use Cases
///
/// - **LLM Initialization**: Provide tool definitions to Claude/GPT
/// - **Function Discovery**: Browse available functions in UI
/// - **API Documentation**: Generate function reference docs
///
/// # Performance
///
/// - **List Time**: ~1-5ms (in-memory registry)
/// - **No I/O**: Pure memory operation
///
/// # Architecture
///
/// Thin controller delegating to `FunctionRegistry::list_tools()`
async fn list_available_functions_impl(container: &Container) -> Result<Vec<serde_json::Value>> {
    debug!("Listing available functions");

    // Get registry
    let registry = container.function_registry();

    // Get tools as JSON
    let tools: Vec<serde_json::Value> = registry
        .list_tools()
        .iter()
        .map(|tool| {
            serde_json::json!({
                "name": tool.name,
                "description": tool.description,
                "input_schema": tool.input_schema,
            })
        })
        .collect();

    info!("Listed {} available functions", tools.len());

    Ok(tools)
}

pub async fn list_available_functions(
    container: State<'_, Container>,
) -> Result<Vec<serde_json::Value>> {
    list_available_functions_impl(&container).await
}

/// Execute LLM function call with validation
///
/// Executes a function call from LLM with input validation and audit logging.
/// Returns success/failure result with data or error message.
///
/// # Arguments
///
/// * `call` - Function call with id, name, and arguments
/// * `container` - Service container with executor and security context
///
/// # Returns
///
/// * `Ok(FunctionResult)` - Execution result with success flag and data/error
/// * `Err(AppError)` - Execution failed (invalid function, validation error)
///
/// # Example
///
/// ```typescript
/// import { invoke } from '@tauri-apps/api/core';
///
/// interface FunctionCall {
///   id: string;
///   name: string;
///   arguments: Record<string, any>;
/// }
///
/// interface FunctionResult {
///   success: boolean;
///   data?: any;
///   errorMessage?: string;
/// }
///
/// // Execute semantic search function
/// const result = await invoke<FunctionResult>('execute_function', {
///   call: {
///     id: 'call_123',
///     name: 'semantic_search',
///     arguments: { query: 'machine learning', limit: 5 }
///   }
/// });
///
/// if (result.success) {
///   console.log('Search results:', result.data);
/// } else {
///   console.error('Function failed:', result.errorMessage);
/// }
///
/// // LLM integration example
/// const executeLLMToolCall = async (toolUse: any) => {
///   const result = await invoke<FunctionResult>('execute_function', {
///     call: {
///       id: toolUse.id,
///       name: toolUse.name,
///       arguments: toolUse.input
///     }
///   });
///
///   return {
///     type: 'tool_result',
///     tool_use_id: toolUse.id,
///     content: result.success ? result.data : result.errorMessage
///   };
/// };
///
/// // Agentic loop with function calling
/// const runAgenticLoop = async (userMessage: string) => {
///   let messages = [{ role: 'user', content: userMessage }];
///
///   while (true) {
///     const response = await callClaude(messages);
///
///     if (response.stop_reason === 'end_turn') break;
///
///     if (response.stop_reason === 'tool_use') {
///       const toolResults = await Promise.all(
///         response.content
///           .filter(c => c.type === 'tool_use')
///           .map(tool => executeLLMToolCall(tool))
///       );
///
///       messages.push(...toolResults);
///     }
///   }
/// };
/// ```
///
/// # Security
///
/// **Input Validation**: Function names and arguments validated
/// **Audit Logging**: All executions logged with success/failure
///
/// # Use Cases
///
/// - **LLM Tool Use**: Execute Claude/GPT function calls
/// - **Agentic Workflows**: Multi-step LLM agents with tool access
/// - **Research Assistant**: LLM-driven research with search/web access
///
/// # Performance
///
/// - **Execution Time**: Varies by function (search ~50-200ms, web ~1-5s)
/// - **Async**: Non-blocking execution
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

    // Get executor
    let executor = container.function_executor();

    // Execute function
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
/// # Arguments
///
/// * `container` - Service container with function registry
///
/// # Returns
///
/// * `Ok(RegistryStats)` - Registry statistics with function counts and metadata
/// * `Err(AppError)` - Registry access failed
///
/// # Example
///
/// ```typescript
/// import { invoke } from '@tauri-apps/api/core';
///
/// interface RegistryStats {
///   totalFunctions: number;
///   executionCount: number;
///   successCount: number;
///   failureCount: number;
///   averageExecutionTimeMs: number;
/// }
///
/// // Get current stats
/// const stats = await invoke<RegistryStats>('get_function_stats');
/// console.log(`${stats.totalFunctions} functions available`);
/// console.log(`Success rate: ${(stats.successCount / stats.executionCount * 100).toFixed(1)}%`);
///
/// // Display in admin dashboard
/// const FunctionStatsPanel = () => {
///   const [stats, setStats] = useState<RegistryStats | null>(null);
///
///   useEffect(() => {
///     const loadStats = async () => {
///       const data = await invoke<RegistryStats>('get_function_stats');
///       setStats(data);
///     };
///
///     loadStats();
///     const interval = setInterval(loadStats, 30000); // Refresh every 30s
///     return () => clearInterval(interval);
///   }, []);
///
///   if (!stats) return <div>Loading...</div>;
///
///   return (
///     <div className="stats-panel">
///       <h3>Function Registry</h3>
///       <div>Total Functions: {stats.totalFunctions}</div>
///       <div>Execution Count: {stats.executionCount}</div>
///       <div>Success Rate: {(stats.successCount / stats.executionCount * 100).toFixed(1)}%</div>
///       <div>Avg Response Time: {stats.averageExecutionTimeMs.toFixed(2)}ms</div>
///     </div>
///   );
/// };
///
/// // Monitoring and alerting
/// const monitorFunctionHealth = async () => {
///   const stats = await invoke<RegistryStats>('get_function_stats');
///
///   const successRate = stats.successCount / stats.executionCount;
///   if (successRate < 0.95) {
///     console.warn(`Function success rate dropped to ${(successRate * 100).toFixed(1)}%`);
///     // Send alert
///   }
///
///   if (stats.averageExecutionTimeMs > 1000) {
///     console.warn(`Function execution time increased to ${stats.averageExecutionTimeMs}ms`);
///     // Send alert
///   }
/// };
///
/// // Debug function registry issues
/// const debugFunctionRegistry = async () => {
///   const stats = await invoke<RegistryStats>('get_function_stats');
///
///   console.log('Function Registry Debug Info:');
///   console.log(`- Total Functions: ${stats.totalFunctions}`);
///   console.log(`- Total Executions: ${stats.executionCount}`);
///   console.log(`- Failures: ${stats.failureCount}`);
///   console.log(`- Avg Execution: ${stats.averageExecutionTimeMs}ms`);
///
///   if (stats.totalFunctions === 0) {
///     console.error('Function registry is empty - initialization failed?');
///   }
/// };
/// ```
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
/// # Use Cases
///
/// - **Admin Dashboard**: Display function registry health and usage
/// - **Monitoring**: Track function execution metrics over time
/// - **Debugging**: Diagnose function registry issues
/// - **Performance Tuning**: Identify slow functions needing optimization
/// - **Alerting**: Trigger alerts on degraded success rates or response times
///
/// # Performance
///
/// - **Query Time**: ~1-5ms (in-memory statistics)
/// - **No I/O**: Pure memory operation (no database access)
/// - **Constant Time**: O(1) lookup regardless of function count
///
/// # Architecture
///
/// Thin controller delegating to `FunctionRegistry::stats()`
pub async fn get_function_stats(
    container: State<'_, Container>,
) -> Result<crate::domain::function_call::RegistryStats> {
    debug!("Getting function registry statistics");

    // Get registry
    let registry = container.function_registry();

    // Get stats
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
    use crate::application::ports::{DocumentRepositoryPort, RepositoryPort};
    use crate::domain::entities::Document;
    use crate::infrastructure::persistence::repositories::mocks::MockDocumentRepository as DddMockDocRepo;
    use crate::infrastructure::services::traits::*;
    use crate::observability::Metrics;
    use crate::security::SecurityContext;
    use sqlx::SqlitePool;
    use std::sync::Arc;

    async fn create_test_container() -> Result<Container> {
        let pool = SqlitePool::connect(":memory:").await.map_err(|e| {
            crate::shared::error::AppError::database(format!("Test pool creation failed: {}", e))
        })?;
        let security_context = Arc::new(SecurityContext::new());
        let metrics = Arc::new(Metrics::new());

        let embedding_service = Arc::new(MockEmbeddingService::new(
            crate::domain::embedding_constants::DEFAULT_EMBEDDING_DIM,
        )) as Arc<dyn EmbeddingServiceTrait>;
        let search_service = Arc::new(MockSearchService::new()) as Arc<dyn SearchServiceTrait>;
        let bm25_search = Arc::new(MockBM25Search::new()) as Arc<dyn BM25SearchTrait>;
        let hybrid_search = Arc::new(MockHybridSearch::new()) as Arc<dyn HybridSearchTrait>;
        let tag_service = Arc::new(MockTagService::new()) as Arc<dyn TagServiceTrait>;
        let file_storage =
            Arc::new(MockFileStorageService::new()) as Arc<dyn FileStorageServiceTrait>;
        let model_manager = Arc::new(MockModelManager::new()) as Arc<dyn ModelManagerTrait>;
        let web_ingestion =
            Arc::new(MockWebIngestionService::new()) as Arc<dyn WebIngestionServiceTrait>;
        let search_enrichment =
            Arc::new(MockSearchEnrichmentService::new()) as Arc<dyn SearchEnrichmentServiceTrait>;
        let conversation =
            Arc::new(MockConversationService::new()) as Arc<dyn ConversationServiceTrait>;
        let context_manager =
            Arc::new(MockContextManager::new(4000)) as Arc<dyn ContextManagerTrait>;
        let mock = Arc::new(DddMockDocRepo::new());
        let doc_repo = mock as Arc<dyn RepositoryPort<Document> + DocumentRepositoryPort>;
        let chunk_repo = Arc::new(MockChunkRepository::new()) as Arc<dyn ChunkRepositoryTrait>;
        let tag_repo = Arc::new(MockTagRepository::new()) as Arc<dyn TagRepositoryTrait>;
        let mention_repo = Arc::new(MockMentionRepository::new())
            as Arc<dyn crate::application::ports::mention_repository_port::MentionRepositoryPort>;
        let index_storage = Arc::new(MockIndexStorage::new()) as Arc<dyn IndexStorageTrait>;

        // Function calling services
        let function_registry =
            Arc::new(MockFunctionRegistry::new()) as Arc<dyn FunctionRegistryTrait>;
        let function_executor =
            Arc::new(MockFunctionExecutor::new()) as Arc<dyn FunctionExecutorTrait>;
        let web_service = Arc::new(MockWebService::new()) as Arc<dyn WebServiceTrait>;

        Ok(Container::new(
            pool,
            security_context,
            metrics,
            embedding_service,
            search_service,
            bm25_search,
            hybrid_search,
            tag_service,
            file_storage,
            model_manager,
            web_ingestion,
            search_enrichment,
            conversation,
            context_manager,
            None,
            doc_repo,
            chunk_repo,
            tag_repo,
            mention_repo,
            index_storage,
            None,
            None,
            function_registry,
            function_executor,
            web_service,
        ))
    }

    #[tokio::test]
    async fn test_list_available_functions() -> Result<()> {
        let container = create_test_container().await?;

        let result = list_available_functions_impl(&container).await?;

        assert_eq!(result.len(), 0); // Mock registry starts empty
        Ok(())
    }

    #[tokio::test]
    async fn test_execute_function() -> Result<()> {
        let container = create_test_container().await?;

        let call = FunctionCall::new(
            "call_123",
            "test_function",
            serde_json::json!({"param": "value"}),
        );

        let function_result = execute_function_impl(&container, call).await?;

        assert!(function_result.success); // Mock executor always succeeds
        Ok(())
    }
}
