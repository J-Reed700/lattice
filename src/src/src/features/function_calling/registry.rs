//! Function registry service
//!
//! Central registry for LLM-callable functions with metadata management.
//!
//! # Architecture
//!
//! - **Thread-safe**: Uses RwLock for concurrent access
//! - **Validation**: Validates tool definitions on registration
//! - **Statistics**: Tracks usage metrics
//!
//! # Example
//! ```rust,no_run
//! use lattice::infrastructure::services::function_registry::FunctionRegistry;
//! use lattice::domain::function_call::ToolDefinition;
//! use serde_json::json;
//!
//! let registry = FunctionRegistry::new();
//!
//! let tool = ToolDefinition::new(
//!     "semantic_search",
//!     "Search documents using semantic similarity",
//!     json!({
//!         "type": "object",
//!         "properties": {
//!             "query": {"type": "string"},
//!             "limit": {"type": "integer", "default": 10}
//!         },
//!         "required": ["query"]
//!     })
//! )?;
//!
//! registry.register(tool)?;
//! let tools = registry.list_tools();
//! # Ok::<(), String>(())
//! ```

use crate::features::function_calling::domain::{RegistryStats, ToolDefinition};
use crate::features::function_calling::FunctionRegistryTrait;
use crate::shared::error::{AppError, Result};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;

/// Function registry implementation
///
/// Manages a collection of callable functions for LLMs.
/// Thread-safe with RwLock for concurrent read access.
pub struct FunctionRegistry {
    /// Registered tools by name
    tools: Arc<RwLock<HashMap<String, ToolDefinition>>>,

    /// Usage statistics
    stats: Arc<RwLock<RegistryStats>>,
}

impl FunctionRegistry {
    /// Create a new empty registry
    pub fn new() -> Self {
        Self {
            tools: Arc::new(RwLock::new(HashMap::new())),
            stats: Arc::new(RwLock::new(RegistryStats::default())),
        }
    }

    /// Create a registry pre-populated with tools
    ///
    /// # Arguments
    /// * `tools` - Initial tools to register
    ///
    /// # Returns
    /// Registry with all tools registered
    ///
    /// # Errors
    /// Returns error if any tool fails to register (duplicate names, validation errors)
    pub fn with_tools(tools: Vec<ToolDefinition>) -> Result<Self> {
        let registry = Self::new();
        for tool in tools {
            registry.register(tool)?;
        }
        Ok(registry)
    }

    /// Record a function call for statistics
    ///
    /// # Arguments
    /// * `function_name` - Name of function that was called
    pub fn record_call(&self, function_name: &str) {
        let mut stats = self.stats.write();
        stats.record_call(function_name);
    }
}

impl Default for FunctionRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl FunctionRegistryTrait for FunctionRegistry {
    fn register(&self, tool: ToolDefinition) -> Result<()> {
        let mut tools = self.tools.write();

        // Check for duplicates
        if tools.contains_key(&tool.name) {
            return Err(AppError::InvalidData(format!(
                "Function '{}' is already registered",
                tool.name
            )));
        }

        // Insert tool
        tools.insert(tool.name.clone(), tool);

        // Update stats
        let mut stats = self.stats.write();
        stats.total_functions = tools.len();

        Ok(())
    }

    fn get_tool(&self, name: &str) -> Option<ToolDefinition> {
        self.tools.read().get(name).cloned()
    }

    fn list_tools(&self) -> Vec<ToolDefinition> {
        self.tools.read().values().cloned().collect()
    }

    fn stats(&self) -> RegistryStats {
        self.stats.read().clone()
    }
}

/// Initialize registry with built-in functions
///
/// Registers built-ins:
/// - semantic_search
/// - get_document
/// - list_documents
/// - web_search
/// - fetch_url_content
/// - wiki_search
/// - wiki_summary
///
/// # Returns
/// Fully initialized registry
pub fn init_function_registry() -> Result<FunctionRegistry> {
    use serde_json::json;

    let registry = FunctionRegistry::new();

    // Phase 1: Core Retrieval Functions

    registry.register(ToolDefinition::new(
        "semantic_search",
        "Search the user's document lattice using semantic similarity. Use this when the user asks to find, search, or retrieve information from their documents.",
        json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Search query describing what to find"
                },
                "limit": {
                    "type": "integer",
                    "description": "Maximum number of results to return (1-50)",
                    "default": 10,
                    "minimum": 1,
                    "maximum": 50
                },
                "threshold": {
                    "type": "number",
                    "description": "Minimum similarity score (0.0-1.0). Lower = more results",
                    "default": 0.3,
                    "minimum": 0.0,
                    "maximum": 1.0
                },
                "search_mode": {
                    "type": "string",
                    "enum": ["semantic", "keyword", "hybrid"],
                    "description": "Search algorithm: semantic (embeddings), keyword (BM25), or hybrid (both)",
                    "default": "hybrid"
                },
                "file_types": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "Filter by file extensions (e.g., ['pdf', 'txt'])"
                },
                "date_from": {
                    "type": "string",
                    "format": "date-time",
                    "description": "Only return documents modified after this date (ISO 8601)"
                },
                "date_to": {
                    "type": "string",
                    "format": "date-time",
                    "description": "Only return documents modified before this date (ISO 8601)"
                }
            },
            "required": ["query"]
        })
    )?)?;

    registry.register(ToolDefinition::new(
        "get_document",
        "Retrieve content from a specific document by ID. Use this after semantic_search to inspect document text, and request additional pages for large documents.",
        json!({
            "type": "object",
            "properties": {
                "document_id": {
                    "type": "string",
                    "description": "Document ID from search results"
                },
                "include_metadata": {
                    "type": "boolean",
                    "description": "Include file metadata (size, dates, tags)",
                    "default": true
                },
                "max_content_length": {
                    "type": "integer",
                    "description": "Maximum characters to return per page",
                    "default": 50000,
                    "minimum": 1000,
                    "maximum": 100000
                },
                "page": {
                    "type": "integer",
                    "description": "1-based page number for internal pagination",
                    "default": 1,
                    "minimum": 1
                }
            },
            "required": ["document_id"]
        })
    )?)?;

    registry.register(ToolDefinition::new(
        "list_documents",
        "Browse and filter documents in the lattice. Use this to explore what documents exist, filter by type/date/tags, or get recent/favorite documents.",
        json!({
            "type": "object",
            "properties": {
                "filter_mode": {
                    "type": "string",
                    "enum": ["all", "recent", "favorites", "by_tag", "by_type"],
                    "description": "Filtering mode for documents",
                    "default": "all"
                },
                "file_types": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "Filter by file extensions (for 'by_type' mode)"
                },
                "tags": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "Filter by tags (for 'by_tag' mode)"
                },
                "date_from": {
                    "type": "string",
                    "format": "date-time",
                    "description": "Only return documents modified after this date"
                },
                "date_to": {
                    "type": "string",
                    "format": "date-time",
                    "description": "Only return documents modified before this date"
                },
                "limit": {
                    "type": "integer",
                    "description": "Maximum number of documents to return (1-500)",
                    "default": 50,
                    "minimum": 1,
                    "maximum": 500
                },
                "offset": {
                    "type": "integer",
                    "description": "Number of documents to skip (for pagination)",
                    "default": 0,
                    "minimum": 0
                },
                "sort_by": {
                    "type": "string",
                    "enum": ["modified", "created", "name", "size"],
                    "description": "Sort field",
                    "default": "modified"
                },
                "sort_order": {
                    "type": "string",
                    "enum": ["asc", "desc"],
                    "description": "Sort direction",
                    "default": "desc"
                }
            },
            "required": []
        })
    )?)?;

    // Phase 2: Web Integration Functions

    registry.register(ToolDefinition::new(
        "web_search",
        "Search the web with pagination and provider enrichment when information isn't in the lattice. Supports DuckDuckGo, Bing, and optional Wikipedia blending.",
        json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Web search query"
                },
                "max_results": {
                    "type": "integer",
                    "description": "Maximum number of web results to return for this page (1-50)",
                    "default": 5,
                    "minimum": 1,
                    "maximum": 50
                },
                "page": {
                    "type": "integer",
                    "description": "1-based page number for paginated search",
                    "default": 1,
                    "minimum": 1
                },
                "offset": {
                    "type": "integer",
                    "description": "Absolute result offset before pagination",
                    "default": 0,
                    "minimum": 0
                },
                "providers": {
                    "type": "array",
                    "description": "Preferred provider order",
                    "items": {
                        "type": "string",
                        "enum": ["duckduckgo", "bing", "wikipedia"]
                    }
                },
                "include_wikipedia": {
                    "type": "boolean",
                    "description": "Include Wikipedia enrichment in web results",
                    "default": false
                },
                "depth": {
                    "type": "integer",
                    "description": "Recursive deep-research depth (1-4)",
                    "default": 1,
                    "minimum": 1,
                    "maximum": 4
                },
                "branch_queries": {
                    "type": "integer",
                    "description": "Maximum follow-up branches per depth step (1-4)",
                    "default": 2,
                    "minimum": 1,
                    "maximum": 4
                }
            },
            "required": ["query"]
        })
    )?)?;

    registry.register(ToolDefinition::new(
        "fetch_url_content",
        "Fetch and extract readable text from a web URL. Use this after web_search to get full content from interesting results, or when the user provides a URL to read/analyze.",
        json!({
            "type": "object",
            "properties": {
                "url": {
                    "type": "string",
                    "description": "URL to fetch and extract content from"
                }
            },
            "required": ["url"]
        })
    )?)?;

    // Phase 2.5: Wikipedia Functions (keyless enrichment)

    registry.register(ToolDefinition::new(
        "wiki_search",
        "Search Wikipedia titles and snippets. Use this for factual background when broad web search is noisy or blocked.",
        json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Wikipedia search query"
                },
                "max_results": {
                    "type": "integer",
                    "description": "Maximum number of results to return (1-10)",
                    "default": 5,
                    "minimum": 1,
                    "maximum": 10
                }
            },
            "required": ["query"]
        })
    )?)?;

    registry.register(ToolDefinition::new(
        "wiki_summary",
        "Fetch the lead summary for a specific Wikipedia page title. Use this after wiki_search when you need concise, sourced overview text.",
        json!({
            "type": "object",
            "properties": {
                "title": {
                    "type": "string",
                    "description": "Exact or near-exact Wikipedia article title"
                }
            },
            "required": ["title"]
        })
    )?)?;

    Ok(registry)
}

/// Register user-defined custom query tools from settings.
///
/// Custom tools are exposed as regular LLM functions and routed at runtime by
/// the function executor.
pub fn register_custom_query_tools(
    registry: &dyn FunctionRegistryTrait,
    custom_tools: &[crate::features::settings::dto::CustomToolSettingsDto],
) -> Result<()> {
    use serde_json::json;

    for tool in custom_tools {
        if !tool.enabled {
            continue;
        }

        let definition = ToolDefinition::new(
            tool.name.clone(),
            tool.description.clone(),
            json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Query text passed to this custom endpoint"
                    },
                    "max_results": {
                        "type": "integer",
                        "description": "Requested max result count",
                        "default": tool.default_max_results,
                        "minimum": 1,
                        "maximum": 100
                    }
                },
                "required": ["query"]
            }),
        )
        .map_err(AppError::InvalidData)?;

        registry.register(definition)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_registry_creation() {
        let registry = FunctionRegistry::new();
        assert_eq!(registry.list_tools().len(), 0);
    }

    #[test]
    fn test_register_tool() {
        let registry = FunctionRegistry::new();
        let tool = ToolDefinition::new(
            "test_function",
            "A test function",
            json!({"type": "object", "properties": {}}),
        )
        .unwrap();

        let result = registry.register(tool.clone());
        assert!(result.is_ok());

        assert_eq!(registry.list_tools().len(), 1);
        assert!(registry.get_tool("test_function").is_some());
    }

    #[test]
    fn test_duplicate_registration() {
        let registry = FunctionRegistry::new();
        let tool = ToolDefinition::new(
            "test_function",
            "A test function",
            json!({"type": "object", "properties": {}}),
        )
        .unwrap();

        registry.register(tool.clone()).unwrap();
        let result = registry.register(tool);

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("already registered"));
    }

    #[test]
    fn test_init_function_registry() {
        let registry = init_function_registry().unwrap();
        let tools = registry.list_tools();

        // Should have all 7 built-in functions
        assert_eq!(tools.len(), 7);

        // Verify each function exists
        assert!(registry.get_tool("semantic_search").is_some());
        assert!(registry.get_tool("get_document").is_some());
        assert!(registry.get_tool("list_documents").is_some());
        assert!(registry.get_tool("web_search").is_some());
        assert!(registry.get_tool("fetch_url_content").is_some());
        assert!(registry.get_tool("wiki_search").is_some());
        assert!(registry.get_tool("wiki_summary").is_some());
    }

    #[test]
    fn test_stats() {
        let registry = FunctionRegistry::new();
        let tool = ToolDefinition::new("test", "test", json!({"type": "object", "properties": {}}))
            .unwrap();
        registry.register(tool).unwrap();

        registry.record_call("test");
        registry.record_call("test");

        let stats = registry.stats();
        assert_eq!(stats.total_functions, 1);
        assert_eq!(stats.call_counts.get("test"), Some(&2));
    }

    #[test]
    fn test_register_custom_query_tools() {
        let registry = FunctionRegistry::new();
        let tools = vec![
            crate::features::settings::dto::CustomToolSettingsDto {
                enabled: true,
                name: "pubmed_search".to_string(),
                description: "Search PubMed".to_string(),
                endpoint: "https://example.org/search".to_string(),
                query_param: "q".to_string(),
                max_results_param: Some("limit".to_string()),
                default_max_results: 5,
            },
            crate::features::settings::dto::CustomToolSettingsDto {
                enabled: false,
                name: "disabled_tool".to_string(),
                description: "Disabled".to_string(),
                endpoint: "https://example.org/disabled".to_string(),
                query_param: "q".to_string(),
                max_results_param: Some("limit".to_string()),
                default_max_results: 5,
            },
        ];

        register_custom_query_tools(&registry, &tools).expect("custom tool registration");

        assert!(registry.get_tool("pubmed_search").is_some());
        assert!(registry.get_tool("disabled_tool").is_none());
    }
}
