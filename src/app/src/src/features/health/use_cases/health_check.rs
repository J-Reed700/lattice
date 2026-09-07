//! # Health Check Use Case
//!
//! Performs comprehensive system health checks.
//!
//! This use case orchestrates:
//! 1. Database connectivity check
//! 2. Embedding service availability check
//! 3. LLM service availability check
//! 4. Overall system status determination
//!
//! ## Example
//!
//! ```rust,no_run
//! use lattice::application::use_cases::health::HealthCheckUseCase;
//!
//! # async fn example(use_case: HealthCheckUseCase) -> Result<(), Box<dyn std::error::Error>> {
//! let response = use_case.execute().await?;
//! println!("System status: {}", response.status);
//! println!("Database: {}", response.database);
//! println!("Embedding: {}", response.embedding_model);
//! println!("LLM: {}", response.llm);
//! # Ok(())
//! # }
//! ```

use chrono::Utc;
use sqlx::SqlitePool;
use std::sync::Arc;

use crate::application::ports::{EmbeddingPort, LLMPort};
use crate::features::health::dto::HealthCheckResponseDto;
use crate::shared::error::Result;

/// Health check use case.
///
/// Coordinates system health checks by verifying the availability of core services:
/// - Database connection
/// - Embedding service
/// - LLM service
///
/// ## Dependencies
///
/// - `db_pool` - Database connection pool for health checks
/// - `embedding_service` - Embedding service to verify availability
/// - `llm_service` - LLM service to verify availability
pub struct HealthCheckUseCase {
    db_pool: SqlitePool,
    embedding_service: Arc<dyn EmbeddingPort>,
    llm_service: Arc<dyn LLMPort>,
}

impl HealthCheckUseCase {
    /// Create a new health check use case.
    ///
    /// # Arguments
    ///
    /// * `db_pool` - SQLite connection pool
    /// * `embedding_service` - Service for generating embeddings
    /// * `llm_service` - Service for LLM interactions
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use lattice::application::use_cases::health::HealthCheckUseCase;
    /// # use std::sync::Arc;
    /// # use sqlx::SqlitePool;
    /// # use lattice::application::ports::{EmbeddingPort, LLMPort};
    ///
    /// # async fn example(
    /// #     pool: SqlitePool,
    /// #     embedder: Arc<dyn EmbeddingPort>,
    /// #     llm: Arc<dyn LLMPort>
    /// # ) {
    /// let use_case = HealthCheckUseCase::new(pool, embedder, llm);
    /// # }
    /// ```
    pub fn new(
        db_pool: SqlitePool,
        embedding_service: Arc<dyn EmbeddingPort>,
        llm_service: Arc<dyn LLMPort>,
    ) -> Self {
        Self {
            db_pool,
            embedding_service,
            llm_service,
        }
    }

    /// Execute health check.
    ///
    /// # Returns
    ///
    /// Health check response with status of all services
    ///
    /// # Errors
    ///
    /// This operation is designed to never fail. Individual service failures
    /// are captured in the response as `false` values, but the overall
    /// health check always returns a response.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use lattice::application::use_cases::health::HealthCheckUseCase;
    /// # async fn example(use_case: HealthCheckUseCase) -> Result<(), Box<dyn std::error::Error>> {
    /// let response = use_case.execute().await?;
    ///
    /// if response.status == "healthy" {
    ///     println!("All systems operational");
    /// } else if response.status == "degraded" {
    ///     println!("Some systems unavailable");
    /// } else {
    ///     println!("Critical systems down");
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn execute(&self) -> Result<HealthCheckResponseDto> {
        // 1. Check database health
        let db_healthy = self.check_database().await;

        // 2. Check embedding service health
        let embedding_healthy = self.check_embedding_service().await;

        // 3. Check LLM service health
        let llm_healthy = self.check_llm_service().await;

        // 4. Determine overall system status
        let status = self.determine_status(db_healthy, embedding_healthy, llm_healthy);

        // 5. Build response
        Ok(HealthCheckResponseDto {
            status,
            database: db_healthy,
            embedding_model: embedding_healthy,
            llm: llm_healthy,
            timestamp: Utc::now().to_rfc3339(),
        })
    }

    /// Check database connectivity.
    ///
    /// Attempts to acquire a connection from the pool. If successful,
    /// the database is considered healthy.
    async fn check_database(&self) -> bool {
        self.db_pool.acquire().await.is_ok()
    }

    /// Check embedding service availability.
    ///
    /// Calls the service's `is_ready()` method to verify availability.
    /// Returns false if the service is not ready or if the check fails.
    async fn check_embedding_service(&self) -> bool {
        self.embedding_service.is_ready().await.unwrap_or(false)
    }

    /// Check LLM service availability.
    ///
    /// Calls the service's `is_ready()` method to verify availability.
    /// Returns false if the service is not ready or if the check fails.
    async fn check_llm_service(&self) -> bool {
        self.llm_service.is_ready().await.unwrap_or(false)
    }

    /// Determine overall system status based on individual component health.
    ///
    /// Status logic:
    /// - "healthy": Database and embedding service are both healthy (LLM is optional)
    /// - "degraded": Database is healthy but embedding service is not
    /// - "unhealthy": Database is not healthy (critical failure)
    ///
    /// # Arguments
    ///
    /// * `db_healthy` - Database health status
    /// * `embedding_healthy` - Embedding service health status
    /// * `llm_healthy` - LLM service health status (currently unused in logic)
    fn determine_status(
        &self,
        db_healthy: bool,
        embedding_healthy: bool,
        _llm_healthy: bool,
    ) -> String {
        if db_healthy && embedding_healthy {
            "healthy".to_string()
        } else if db_healthy {
            "degraded".to_string()
        } else {
            "unhealthy".to_string()
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;

    // Mock embedding service for testing
    struct MockEmbedder {
        is_ready: bool,
    }

    #[async_trait]
    impl EmbeddingPort for MockEmbedder {
        async fn embed_single(&self, _text: &str) -> Result<Vec<f32>> {
            Ok(vec![0.1, 0.2, 0.3])
        }

        async fn embed_batch(&self, _texts: &[String]) -> Result<Vec<Vec<f32>>> {
            Ok(vec![vec![0.1, 0.2, 0.3]])
        }

        fn dimension(&self) -> usize {
            3
        }

        async fn is_ready(&self) -> Result<bool> {
            Ok(self.is_ready)
        }
    }

    // Mock LLM service for testing
    struct MockLLM {
        is_ready: bool,
    }

    #[async_trait]
    impl LLMPort for MockLLM {
        async fn generate(
            &self,
            _prompt: &str,
            _context: &[String],
            _images: Option<Vec<String>>,
        ) -> Result<String> {
            Ok("response".to_string())
        }

        async fn generate_streaming(
            &self,
            _prompt: &str,
            _context: &[String],
            _images: Option<Vec<String>>,
        ) -> Result<Box<dyn futures::Stream<Item = Result<String>> + Send + Unpin + '_>> {
            unimplemented!()
        }

        fn model_name(&self) -> &str {
            "mock-model"
        }

        fn max_context_tokens(&self) -> usize {
            4096
        }

        fn count_tokens(&self, text: &str) -> usize {
            text.len() / 4
        }

        async fn is_ready(&self) -> Result<bool> {
            Ok(self.is_ready)
        }
    }

    async fn create_test_pool() -> SqlitePool {
        SqlitePool::connect(":memory:").await.unwrap()
    }

    #[tokio::test]
    async fn test_health_check_all_healthy() {
        let pool = create_test_pool().await;
        let embedder = Arc::new(MockEmbedder { is_ready: true });
        let llm = Arc::new(MockLLM { is_ready: true });

        let use_case = HealthCheckUseCase::new(pool, embedder, llm);
        let response = use_case.execute().await.unwrap();

        assert_eq!(response.status, "healthy");
        assert!(response.database);
        assert!(response.embedding_model);
        assert!(response.llm);
    }

    #[tokio::test]
    async fn test_health_check_embedding_down() {
        let pool = create_test_pool().await;
        let embedder = Arc::new(MockEmbedder { is_ready: false });
        let llm = Arc::new(MockLLM { is_ready: true });

        let use_case = HealthCheckUseCase::new(pool, embedder, llm);
        let response = use_case.execute().await.unwrap();

        assert_eq!(response.status, "degraded");
        assert!(response.database);
        assert!(!response.embedding_model);
    }

    #[tokio::test]
    async fn test_health_check_llm_optional() {
        let pool = create_test_pool().await;
        let embedder = Arc::new(MockEmbedder { is_ready: true });
        let llm = Arc::new(MockLLM { is_ready: false });

        let use_case = HealthCheckUseCase::new(pool, embedder, llm);
        let response = use_case.execute().await.unwrap();

        // LLM is optional, so status should still be healthy if db and embedding are ok
        assert_eq!(response.status, "healthy");
        assert!(response.database);
        assert!(response.embedding_model);
        assert!(!response.llm);
    }

    #[tokio::test]
    async fn test_health_check_timestamp() {
        let pool = create_test_pool().await;
        let embedder = Arc::new(MockEmbedder { is_ready: true });
        let llm = Arc::new(MockLLM { is_ready: true });

        let use_case = HealthCheckUseCase::new(pool, embedder, llm);
        let response = use_case.execute().await.unwrap();

        // Verify timestamp is a valid RFC3339 string
        chrono::DateTime::parse_from_rfc3339(&response.timestamp).unwrap();
    }
}
