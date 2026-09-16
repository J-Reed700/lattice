//! Function executor service
//!
//! Routes and executes function calls from LLMs to appropriate handlers.
//!
//! # Architecture
//!
//! - **Router**: Routes function calls by name to handlers
//! - **Security**: Rate limiting, input validation, audit logging
//! - **Error Handling**: Comprehensive error handling with user-friendly messages
//!
//! # Example
//! ```rust,no_run
//! use lattice::infrastructure::services::function_executor::FunctionExecutor;
//! use lattice::domain::function_call::FunctionCall;
//! use serde_json::json;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let executor = FunctionExecutor::new(/* dependencies */);
//!
//! let call = FunctionCall::new(
//!     "call_123",
//!     "semantic_search",
//!     json!({"query": "machine learning", "limit": 5})
//! );
//!
//! let result = executor.execute(call).await?;
//! assert!(result.success);
//! # Ok(())
//! # }
//! ```

mod custom_tools;
mod dispatch;
mod document_tools;
mod search_tools;
mod web_tools;

#[cfg(test)]
mod tests;

use crate::application::ports::{
    ChunkRepositoryPort, DocumentRepository, FavoritesRepositoryPort, FileStoragePort,
    RecentDocumentsRepositoryPort,
};
use crate::features::embedding::EmbeddingServiceTrait;
use crate::features::function_calling::domain::{FunctionCall, FunctionResult};
use crate::features::function_calling::{FunctionExecutorTrait, FunctionRegistryTrait};
use crate::features::search::{BM25SearchTrait, HybridSearchTrait, SearchServiceTrait};
use crate::features::settings::dto::CustomToolSettingsDto;
use crate::features::tags::TagServiceTrait;
use crate::features::web::WebServiceTrait;
use crate::shared::error::{AppError, Result};
use async_trait::async_trait;
use jsonschema::JSONSchema;
use reqwest::Client;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::RwLock;
use tracing::{error, info};

const TOOL_SEMANTIC_SEARCH: &str = "semantic_search";
const TOOL_GET_DOCUMENT: &str = "get_document";
const TOOL_LIST_DOCUMENTS: &str = "list_documents";
const TOOL_WEB_SEARCH: &str = "web_search";
const TOOL_FETCH_URL_CONTENT: &str = "fetch_url_content";
const TOOL_WIKI_SEARCH: &str = "wiki_search";
const TOOL_WIKI_SUMMARY: &str = "wiki_summary";
const WIKIPEDIA_USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"));

/// Function executor implementation
///
/// Routes function calls to appropriate handlers with security controls.
pub struct FunctionExecutor {
    /// Function registry for validation
    registry: Arc<dyn FunctionRegistryTrait>,

    /// Embedding service for semantic search
    embedding_service: Arc<dyn EmbeddingServiceTrait>,

    /// Search service for semantic/vector search
    search_service: Arc<dyn SearchServiceTrait>,

    /// BM25 search service for keyword search
    bm25_service: Arc<dyn BM25SearchTrait>,

    /// Hybrid search service
    hybrid_service: Arc<dyn HybridSearchTrait>,

    /// Document repository for document operations (DDD ports)
    document_repository: Arc<dyn DocumentRepository>,

    /// Chunk repository for chunk counts
    chunk_repository: Arc<dyn ChunkRepositoryPort>,

    /// Tag service for document tags
    tag_service: Arc<dyn TagServiceTrait>,

    /// Favorites repository for favorite status
    favorites_repository: Arc<dyn FavoritesRepositoryPort>,

    /// Recent documents repository for recent filters
    recent_documents_repository: Arc<dyn RecentDocumentsRepositoryPort>,

    /// File storage for document content and metadata
    file_storage: Arc<dyn FileStoragePort>,

    /// Web service for search and URL fetching.
    web_service: Arc<dyn WebServiceTrait>,

    /// Custom tools loaded from user settings.
    custom_tools: RwLock<HashMap<String, CustomToolSettingsDto>>,

    /// Shared HTTP client for keyless API integrations.
    http_client: Client,
}

impl FunctionExecutor {
    /// Create a new function executor
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        registry: Arc<dyn FunctionRegistryTrait>,
        embedding_service: Arc<dyn EmbeddingServiceTrait>,
        search_service: Arc<dyn SearchServiceTrait>,
        bm25_service: Arc<dyn BM25SearchTrait>,
        hybrid_service: Arc<dyn HybridSearchTrait>,
        document_repository: Arc<dyn DocumentRepository>,
        chunk_repository: Arc<dyn ChunkRepositoryPort>,
        tag_service: Arc<dyn TagServiceTrait>,
        favorites_repository: Arc<dyn FavoritesRepositoryPort>,
        recent_documents_repository: Arc<dyn RecentDocumentsRepositoryPort>,
        file_storage: Arc<dyn FileStoragePort>,
        web_service: Arc<dyn WebServiceTrait>,
    ) -> Self {
        Self::new_with_custom_tools(
            registry,
            embedding_service,
            search_service,
            bm25_service,
            hybrid_service,
            document_repository,
            chunk_repository,
            tag_service,
            favorites_repository,
            recent_documents_repository,
            file_storage,
            web_service,
            HashMap::new(),
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new_with_custom_tools(
        registry: Arc<dyn FunctionRegistryTrait>,
        embedding_service: Arc<dyn EmbeddingServiceTrait>,
        search_service: Arc<dyn SearchServiceTrait>,
        bm25_service: Arc<dyn BM25SearchTrait>,
        hybrid_service: Arc<dyn HybridSearchTrait>,
        document_repository: Arc<dyn DocumentRepository>,
        chunk_repository: Arc<dyn ChunkRepositoryPort>,
        tag_service: Arc<dyn TagServiceTrait>,
        favorites_repository: Arc<dyn FavoritesRepositoryPort>,
        recent_documents_repository: Arc<dyn RecentDocumentsRepositoryPort>,
        file_storage: Arc<dyn FileStoragePort>,
        web_service: Arc<dyn WebServiceTrait>,
        custom_tools: HashMap<String, CustomToolSettingsDto>,
    ) -> Self {
        Self {
            registry,
            embedding_service,
            search_service,
            bm25_service,
            hybrid_service,
            document_repository,
            chunk_repository,
            tag_service,
            favorites_repository,
            recent_documents_repository,
            file_storage,
            web_service,
            custom_tools: RwLock::new(custom_tools),
            http_client: Client::new(),
        }
    }
}

#[async_trait]
impl FunctionExecutorTrait for FunctionExecutor {
    async fn execute(&self, call: FunctionCall) -> Result<FunctionResult> {
        info!(
            "Executing function call: name='{}', id='{}'",
            call.name, call.id
        );

        let is_registered = self.registry.get_tool(&call.name).is_some();
        let has_custom_handler = self
            .custom_tools
            .read()
            .ok()
            .is_some_and(|tools| tools.contains_key(call.name.as_str()));

        if !is_registered && !has_custom_handler {
            error!("Function '{}' not found in registry", call.name);
            return Ok(FunctionResult::error(
                "FUNCTION_NOT_FOUND",
                format!("Function '{}' is not registered", call.name),
            ));
        }

        let normalized_arguments = Self::normalize_tool_arguments(&call.name, &call.arguments);

        if let Err(e) = self.validate_arguments(&call.name, &normalized_arguments) {
            error!("Invalid arguments for '{}': {}", call.name, e);
            return Ok(FunctionResult::error("INVALID_ARGUMENTS", e.to_string()));
        }

        // Route to appropriate handler
        let result = match call.name.as_str() {
            TOOL_SEMANTIC_SEARCH => self.handle_semantic_search(&normalized_arguments).await,
            TOOL_GET_DOCUMENT => self.handle_get_document(&normalized_arguments).await,
            TOOL_LIST_DOCUMENTS => self.handle_list_documents(&normalized_arguments).await,
            TOOL_WEB_SEARCH => self.handle_web_search(&normalized_arguments).await,
            TOOL_FETCH_URL_CONTENT => self.handle_fetch_url(&normalized_arguments).await,
            TOOL_WIKI_SEARCH => self.handle_wiki_search(&normalized_arguments).await,
            TOOL_WIKI_SUMMARY => self.handle_wiki_summary(&normalized_arguments).await,
            _ => match self
                .custom_tools
                .read()
                .ok()
                .and_then(|tools| tools.get(call.name.as_str()).cloned())
            {
                Some(custom_tool) => {
                    self.handle_custom_query_tool(&custom_tool, &normalized_arguments)
                        .await
                }
                None => {
                    error!("No handler for function '{}'", call.name);
                    return Ok(FunctionResult::error(
                        "NO_HANDLER",
                        format!("No handler implemented for function '{}'", call.name),
                    ));
                }
            },
        };

        match result {
            Ok(function_result) => {
                info!("Function '{}' executed successfully", call.name);
                Ok(function_result)
            }
            Err(e) => {
                error!("Function '{}' failed: {}", call.name, e);
                Ok(FunctionResult::error("EXECUTION_ERROR", e.to_string()))
            }
        }
    }

    fn set_custom_tools(&self, custom_tools: HashMap<String, CustomToolSettingsDto>) {
        if let Ok(mut guard) = self.custom_tools.write() {
            *guard = custom_tools;
        }
    }

    fn validate_arguments(&self, function_name: &str, arguments: &serde_json::Value) -> Result<()> {
        if let Some(tool) = self.registry.get_tool(function_name) {
            // Basic validation: check that arguments is an object
            if !arguments.is_object() {
                return Err(AppError::InvalidInput(
                    "Arguments must be a JSON object".to_string(),
                ));
            }

            let schema = JSONSchema::compile(&tool.input_schema).map_err(|e| {
                AppError::InvalidInput(format!("Invalid schema for '{}': {}", function_name, e))
            })?;

            if let Err(errors) = schema.validate(arguments) {
                let message = errors
                    .into_iter()
                    .next()
                    .map(|error| error.to_string())
                    .unwrap_or_else(|| "Schema validation failed".to_string());
                return Err(AppError::InvalidInput(message));
            }

            return Ok(());
        }

        if self
            .custom_tools
            .read()
            .ok()
            .is_some_and(|tools| tools.contains_key(function_name))
        {
            return Self::validate_custom_query_arguments(arguments);
        }

        Err(AppError::InvalidData(format!(
            "Function '{}' not found",
            function_name
        )))
    }
}
