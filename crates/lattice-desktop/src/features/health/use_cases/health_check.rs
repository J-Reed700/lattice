use chrono::Utc;
use sqlx::SqlitePool;
use std::sync::Arc;

use crate::features::health::dto::HealthCheckResponseDto;
use lattice_core::application::ports::EmbeddingPort;
use lattice_core::shared::error::Result;

/// Coordinates system health checks by verifying the availability of core services:
/// - Database connection
/// - Embedding service
/// - LLM readiness (models load lazily on first use)
pub struct HealthCheckUseCase {
    db_pool: SqlitePool,
    embedding_service: Arc<dyn EmbeddingPort>,
}

impl HealthCheckUseCase {
    pub fn new(db_pool: SqlitePool, embedding_service: Arc<dyn EmbeddingPort>) -> Self {
        Self {
            db_pool,
            embedding_service,
        }
    }

    /// Execute health check.
    /// `llm_ready` is whether a language model is loaded and not degraded.
    pub async fn execute(&self, llm_ready: bool) -> Result<HealthCheckResponseDto> {
        let db_healthy = self.check_database().await;
        let embedding_healthy = self.check_embedding_service().await;
        let llm_healthy = llm_ready;
        let status = self.determine_status(db_healthy, embedding_healthy, llm_healthy);

        Ok(HealthCheckResponseDto {
            status,
            database: db_healthy,
            embedding_model: embedding_healthy,
            llm: llm_healthy,
            timestamp: Utc::now().to_rfc3339(),
        })
    }

    async fn check_database(&self) -> bool {
        self.db_pool.acquire().await.is_ok()
    }

    async fn check_embedding_service(&self) -> bool {
        self.embedding_service.is_ready().await.unwrap_or(false)
    }

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

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;

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

    async fn create_test_pool() -> SqlitePool {
        SqlitePool::connect(":memory:").await.unwrap()
    }

    #[tokio::test]
    async fn test_health_check_all_healthy() {
        let pool = create_test_pool().await;
        let embedder = Arc::new(MockEmbedder { is_ready: true });

        let use_case = HealthCheckUseCase::new(pool, embedder);
        let response = use_case.execute(true).await.unwrap();

        assert_eq!(response.status, "healthy");
        assert!(response.database);
        assert!(response.embedding_model);
        assert!(response.llm);
    }

    #[tokio::test]
    async fn test_health_check_embedding_down() {
        let pool = create_test_pool().await;
        let embedder = Arc::new(MockEmbedder { is_ready: false });

        let use_case = HealthCheckUseCase::new(pool, embedder);
        let response = use_case.execute(true).await.unwrap();

        assert_eq!(response.status, "degraded");
        assert!(response.database);
        assert!(!response.embedding_model);
    }

    #[tokio::test]
    async fn test_health_check_llm_optional() {
        let pool = create_test_pool().await;
        let embedder = Arc::new(MockEmbedder { is_ready: true });

        let use_case = HealthCheckUseCase::new(pool, embedder);
        let response = use_case.execute(false).await.unwrap();

        assert_eq!(response.status, "healthy");
        assert!(response.database);
        assert!(response.embedding_model);
        assert!(!response.llm);
    }

    #[tokio::test]
    async fn test_health_check_timestamp() {
        let pool = create_test_pool().await;
        let embedder = Arc::new(MockEmbedder { is_ready: true });

        let use_case = HealthCheckUseCase::new(pool, embedder);
        let response = use_case.execute(true).await.unwrap();

        chrono::DateTime::parse_from_rfc3339(&response.timestamp).unwrap();
    }
}
