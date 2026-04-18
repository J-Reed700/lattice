//! Health feature dependency injection.

use std::sync::Arc;

use sqlx::SqlitePool;

use crate::application::ports::{EmbeddingPort, LLMPort, MockEmbeddingPort};
use crate::features::health::use_cases::HealthCheckUseCase;
use crate::infrastructure::llm::factory::MockLLMPort;

#[derive(Clone)]
pub struct HealthDi {
    pub health_check_use_case: Arc<HealthCheckUseCase>,
}

pub fn build(db_pool: SqlitePool) -> HealthDi {
    // Health uses degraded mocks — the real check is that DB is reachable.
    let embedding = Arc::new(MockEmbeddingPort::new_degraded()) as Arc<dyn EmbeddingPort>;
    let llm = Arc::new(MockLLMPort::default()) as Arc<dyn LLMPort>;

    HealthDi {
        health_check_use_case: Arc::new(HealthCheckUseCase::new(db_pool, embedding, llm)),
    }
}
