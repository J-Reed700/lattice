//! SearchProvider - Embedding and search service management
//!
//! This provider manages embedding generation services with support for:
//! - **Local ONNX**: Offline embedding generation using ONNX Runtime
//! - **Remote API**: Cloud-based embedding services (OpenAI, Anthropic, etc.)
//! - **Mock**: Deterministic mock embeddings for testing
//!
//! # Architecture
//!
//! The provider uses the builder pattern to conditionally initialize
//! services based on configuration:
//!
//! ```rust
//! // Local ONNX embeddings
//! let provider = SearchProviderBuilder::new()
//!     .with_pool(pool)
//!     .with_model_path(&format!("models/{}/model.onnx", DEFAULT_EMBEDDING_MODEL_NAME))
//!     .build()
//!     .await?;
//!
//! // Remote API embeddings
//! let provider = SearchProviderBuilder::new()
//!     .with_pool(pool)
//!     .with_remote_embeddings("sk-...")
//!     .build()
//!     .await?;
//!
//! // Mock embeddings (for tests)
//! let provider = SearchProviderBuilder::new()
//!     .with_pool(pool)
//!     .build()
//!     .await?;
//! ```
//!
//! # Graceful Degradation
//!
//! If no embedding configuration is provided, the provider uses mock
//! embeddings. This allows the application to start and run tests without
//! requiring model files or API keys.
//!
//! # Thread Safety
//!
//! All services are wrapped in `Arc<dyn Trait>` for safe concurrent access.

use crate::infrastructure::ml::{
    OnnxEmbeddingService, RemoteEmbeddingService, DEFAULT_REMOTE_EMBEDDING_MODEL,
    DEFAULT_REMOTE_EMBEDDING_URL,
};
use crate::infrastructure::services::mocks::mock_embedding::MockEmbeddingService;
use crate::infrastructure::services::traits::embedding::EmbeddingServiceTrait;
use crate::shared::error::{AppError, Result};
use sqlx::SqlitePool;
use std::path::PathBuf;
use std::sync::Arc;

/// SearchProvider manages embedding generation and vector search services.
///
/// This provider wraps embedding services and provides access through
/// a consistent trait interface. It supports multiple embedding backends
/// through dependency injection.
///
/// # Design Pattern
///
/// - Uses dependency injection to allow swapping implementations
/// - All services are trait objects (`Arc<dyn Trait>`)
/// - Builder pattern for flexible configuration
/// - Optional ONNX runtime (only initialized if local model requested)
///
/// # Example
///
/// ```rust
/// let provider = SearchProviderBuilder::new()
///     .with_pool(pool)
///     .with_model_path(&format!("models/{}/model.onnx", DEFAULT_EMBEDDING_MODEL_NAME))
///     .build()
///     .await?;
///
/// let embedding = provider.embedding_service().embed_single("hello").await?;
/// assert!(provider.is_local());
/// ```
pub struct SearchProvider {
    /// Embedding service (ONNX, remote API, or mock)
    embedding_service: Arc<dyn EmbeddingServiceTrait>,

    /// Optional ONNX runtime (only present for local embeddings)
    ///
    /// This is kept separately to allow runtime introspection (is_local check)
    /// and to avoid loading ONNX Runtime unnecessarily for remote/mock modes.
    onnx_runtime: Option<Arc<OnnxEmbeddingService>>,
}

impl SearchProvider {
    /// Get the embedding service.
    ///
    /// Returns a trait object that can be used for embedding generation.
    ///
    /// # Returns
    ///
    /// `Arc<dyn EmbeddingServiceTrait>` - Thread-safe embedding service
    ///
    /// # Example
    ///
    /// ```rust
    /// let service = provider.embedding_service();
    /// let embedding = service.embed_single("hello world").await?;
    /// ```
    pub fn embedding_service(&self) -> Arc<dyn EmbeddingServiceTrait> {
        Arc::clone(&self.embedding_service)
    }

    /// Check if using local ONNX embeddings.
    ///
    /// # Returns
    ///
    /// - `true` if using local ONNX Runtime
    /// - `false` if using remote API or mock
    ///
    /// # Example
    ///
    /// ```rust
    /// if provider.is_local() {
    ///     println!("Using local ONNX embeddings (offline capable)");
    /// } else {
    ///     println!("Using remote or mock embeddings");
    /// }
    /// ```
    pub fn is_local(&self) -> bool {
        self.onnx_runtime.is_some()
    }

    /// Get the ONNX runtime if using local embeddings.
    ///
    /// # Returns
    ///
    /// - `Some(Arc<OnnxEmbeddingService>)` if using local ONNX
    /// - `None` if using remote API or mock
    ///
    /// # Example
    ///
    /// ```rust
    /// if let Some(onnx) = provider.onnx_runtime() {
    ///     println!("ONNX embedding dimension: {}", onnx.dimension());
    /// }
    /// ```
    pub fn onnx_runtime(&self) -> Option<Arc<OnnxEmbeddingService>> {
        self.onnx_runtime.as_ref().map(Arc::clone)
    }
}

/// Builder for SearchProvider with flexible configuration.
///
/// Supports three embedding modes:
/// 1. **Local ONNX** - Set with `with_model_path()`
/// 2. **Remote API** - Set with `with_remote_embeddings()` (configurable URL/model)
/// 3. **Mock** - Default if neither is set
///
/// # Required Fields
///
/// - `pool`: SQLite connection pool (required)
///
/// # Optional Fields
///
/// - `model_path`: Path to ONNX model file (enables local mode)
/// - `use_remote_embeddings`: Enable remote API mode
/// - `remote_api_key`: API key for remote service
/// - `remote_api_url`: Override API URL (optional)
/// - `remote_model`: Override model name (optional)
///
/// # Example
///
/// ```rust
/// // Local ONNX embeddings
/// let provider = SearchProviderBuilder::new()
///     .with_pool(pool)
///     .with_model_path(&format!("models/{}/model.onnx", DEFAULT_EMBEDDING_MODEL_DISPLAY_NAME))
///     .build()
///     .await?;
///
/// // Mock embeddings (for tests)
/// let provider = SearchProviderBuilder::new()
///     .with_pool(pool)
///     .build()
///     .await?;
/// ```
pub struct SearchProviderBuilder {
    pool: Option<SqlitePool>,
    model_path: Option<PathBuf>,
    use_remote_embeddings: bool,
    remote_api_key: Option<String>,
    remote_api_url: Option<String>,
    remote_model: Option<String>,
}

impl SearchProviderBuilder {
    /// Create a new builder with default values.
    ///
    /// # Returns
    ///
    /// A builder with no configuration set (will use mock embeddings)
    ///
    /// # Example
    ///
    /// ```rust
    /// let builder = SearchProviderBuilder::new();
    /// ```
    pub fn new() -> Self {
        Self {
            pool: None,
            model_path: None,
            use_remote_embeddings: false,
            remote_api_key: None,
            remote_api_url: None,
            remote_model: None,
        }
    }

    /// Set the SQLite connection pool (required).
    ///
    /// # Arguments
    ///
    /// * `pool` - SQLite connection pool
    ///
    /// # Returns
    ///
    /// Self for method chaining
    ///
    /// # Example
    ///
    /// ```rust
    /// builder.with_pool(pool)
    /// ```
    pub fn with_pool(mut self, pool: SqlitePool) -> Self {
        self.pool = Some(pool);
        self
    }

    /// Set the path to the ONNX model file (enables local mode).
    ///
    /// # Arguments
    ///
    /// * `path` - Path to ONNX model file (e.g., "models/{DEFAULT_EMBEDDING_MODEL_DISPLAY_NAME}/model.onnx")
    ///
    /// # Returns
    ///
    /// Self for method chaining
    ///
    /// # Example
    ///
    /// ```rust
    /// builder.with_model_path(&format!(
    ///     "models/{}/model.onnx",
    ///     DEFAULT_EMBEDDING_MODEL_DISPLAY_NAME
    /// ))
    /// ```
    pub fn with_model_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.model_path = Some(path.into());
        self
    }

    /// Enable remote API embeddings.
    ///
    /// # Arguments
    ///
    /// * `api_key` - API key for remote embedding service
    ///
    /// # Returns
    ///
    /// Self for method chaining
    ///
    /// # Example
    ///
    /// ```rust
    /// builder.with_remote_embeddings("sk-...")
    /// ```
    pub fn with_remote_embeddings(mut self, api_key: impl Into<String>) -> Self {
        self.use_remote_embeddings = true;
        self.remote_api_key = Some(api_key.into());
        self
    }

    /// Override the remote embedding API URL.
    ///
    /// Defaults to `DEFAULT_REMOTE_EMBEDDING_URL` when not provided.
    pub fn with_remote_embedding_url(mut self, api_url: impl Into<String>) -> Self {
        self.remote_api_url = Some(api_url.into());
        self
    }

    /// Override the remote embedding model name.
    ///
    /// Defaults to `DEFAULT_REMOTE_EMBEDDING_MODEL` when not provided.
    pub fn with_remote_embedding_model(mut self, model: impl Into<String>) -> Self {
        self.remote_model = Some(model.into());
        self
    }

    /// Build the SearchProvider.
    ///
    /// # Returns
    ///
    /// - `Ok(SearchProvider)` if successfully built
    /// - `Err(AppError)` if configuration is invalid
    ///
    /// # Errors
    ///
    /// - `AppError::Configuration` if pool is not set
    /// - `AppError::NotFound` if model file doesn't exist (local mode)
    /// - `AppError::EmbeddingFailed` if model fails to load (local mode)
    /// - `AppError::Configuration` if remote mode is requested (not yet implemented)
    ///
    /// # Embedding Mode Selection
    ///
    /// 1. If `model_path` is set → **Local ONNX** mode
    /// 2. If `use_remote_embeddings` is true → **Remote API** mode
    /// 3. Otherwise → **Mock** mode (graceful degradation)
    ///
    /// # Example
    ///
    /// ```rust
    /// let provider = SearchProviderBuilder::new()
    ///     .with_pool(pool)
    ///     .with_model_path(&format!(
    ///         "models/{}/model.onnx",
    ///         DEFAULT_EMBEDDING_MODEL_DISPLAY_NAME
    ///     ))
    ///     .build()
    ///     .await?;
    /// ```
    pub async fn build(self) -> Result<SearchProvider> {
        // Validate required fields
        let _pool = self.pool.ok_or_else(|| {
            AppError::Configuration("SearchProvider requires a SQLite pool".to_string())
        })?;

        // Determine embedding mode and create service
        let (embedding_service, onnx_runtime): (Arc<dyn EmbeddingServiceTrait>, Option<Arc<OnnxEmbeddingService>>) =
            if let Some(model_path) = self.model_path {
                // Local ONNX mode
                tracing::info!(
                    "Initializing SearchProvider with local ONNX embeddings: {}",
                    model_path.display()
                );

                let onnx_service = OnnxEmbeddingService::new(&model_path)?;
                let onnx_arc = Arc::new(onnx_service);

                (
                    onnx_arc.clone() as Arc<dyn EmbeddingServiceTrait>,
                    Some(onnx_arc),
                )
            } else if self.use_remote_embeddings {
                let api_key = self.remote_api_key.ok_or_else(|| {
                    AppError::Configuration("Remote embedding API key is required".to_string())
                })?;
                let api_url = self
                    .remote_api_url
                    .unwrap_or_else(|| DEFAULT_REMOTE_EMBEDDING_URL.to_string());
                let model = self
                    .remote_model
                    .unwrap_or_else(|| DEFAULT_REMOTE_EMBEDDING_MODEL.to_string());

                tracing::info!(
                    "Initializing SearchProvider with remote embeddings: {}",
                    api_url
                );

                let remote_service = RemoteEmbeddingService::new(api_key, api_url, model)?;

                (
                    Arc::new(remote_service) as Arc<dyn EmbeddingServiceTrait>,
                    None,
                )
            } else {
                // Mock mode (graceful degradation)
                tracing::warn!(
                    "No embedding configuration provided - using mock embeddings. \
                     This is suitable for testing but not production use."
                );

                (
                    Arc::new(MockEmbeddingService::default()) as Arc<dyn EmbeddingServiceTrait>,
                    None,
                )
            };

        Ok(SearchProvider {
            embedding_service,
            onnx_runtime,
        })
    }
}

impl Default for SearchProviderBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::embedding_constants::DEFAULT_EMBEDDING_DIM;

    /// Helper to create an in-memory SQLite pool for testing
    async fn create_test_pool() -> SqlitePool {
        SqlitePool::connect("sqlite::memory:")
            .await
            .expect("Failed to create test pool")
    }

    #[tokio::test]
    async fn test_search_provider_no_embeddings() {
        let pool = create_test_pool().await;

        let provider = SearchProviderBuilder::new()
            .with_pool(pool)
            .build()
            .await
            .expect("Failed to build provider");

        // Should use mock embeddings
        assert!(!provider.is_local());
        assert!(provider.onnx_runtime().is_none());

        // Should be able to generate embeddings
        let service = provider.embedding_service();
        let embedding = service
            .embed_single("test")
            .await
            .expect("Failed to generate embedding");

        assert_eq!(embedding.len(), DEFAULT_EMBEDDING_DIM);
    }

    #[tokio::test]
    async fn test_search_provider_builder_missing_pool() {
        let result = SearchProviderBuilder::new().build().await;

        assert!(result.is_err());
        match result {
            Err(AppError::Configuration(msg)) => {
                assert!(msg.contains("pool"));
            }
            _ => panic!("Expected Configuration error"),
        }
    }

    #[tokio::test]
    async fn test_search_provider_local_onnx_missing_file() {
        let pool = create_test_pool().await;

        let result = SearchProviderBuilder::new()
            .with_pool(pool)
            .with_model_path("nonexistent.onnx")
            .build()
            .await;

        assert!(result.is_err());
        match result {
            Err(AppError::NotFound(msg)) => {
                assert!(msg.contains("ONNX model file not found"));
            }
            _ => panic!("Expected NotFound error"),
        }
    }

    #[tokio::test]
    async fn test_search_provider_remote_builder() {
        let pool = create_test_pool().await;

        let provider = SearchProviderBuilder::new()
            .with_pool(pool)
            .with_remote_embeddings("sk-test-key")
            .build()
            .await
            .expect("Failed to build provider");

        assert!(!provider.is_local());
        assert!(provider.onnx_runtime().is_none());
    }

    #[tokio::test]
    async fn test_mock_embedding_service_generates_embeddings() {
        let pool = create_test_pool().await;

        let provider = SearchProviderBuilder::new()
            .with_pool(pool)
            .build()
            .await
            .expect("Failed to build provider");

        let service = provider.embedding_service();

        // Test single embedding
        let embedding = service
            .embed_single("hello world")
            .await
            .expect("Failed to generate embedding");

        assert_eq!(embedding.len(), DEFAULT_EMBEDDING_DIM);
        assert!(embedding.iter().any(|&x| x != 0.0), "Embedding should be non-zero");

        // Test batch embedding
        let texts = vec!["first".to_string(), "second".to_string()];
        let embeddings = service
            .embed_batch(&texts)
            .await
            .expect("Failed to generate batch embeddings");

        assert_eq!(embeddings.len(), 2);
        assert_eq!(embeddings[0].len(), DEFAULT_EMBEDDING_DIM);
        assert_eq!(embeddings[1].len(), DEFAULT_EMBEDDING_DIM);

        // Mock embeddings should be deterministic
        let embedding2 = service
            .embed_single("hello world")
            .await
            .expect("Failed to generate embedding");
        assert_eq!(embedding, embedding2, "Mock embeddings should be deterministic");
    }

    #[tokio::test]
    async fn test_builder_default() {
        let builder = SearchProviderBuilder::default();
        assert!(builder.pool.is_none());
        assert!(builder.model_path.is_none());
        assert!(!builder.use_remote_embeddings);
        assert!(builder.remote_api_url.is_none());
        assert!(builder.remote_model.is_none());
    }

    // Note: Cannot test local ONNX without actual model file in test environment
    // Note: Cannot test remote embeddings without actual API key and service
}
