//! Health feature dependency injection.

use std::sync::Arc;

use sqlx::SqlitePool;

use crate::features::health::use_cases::HealthCheckUseCase;
use crate::interfaces::di::Container;
use lattice_core::application::ports::{
    DegradedLLMPort, EmbeddingPort, LLMPort, MockEmbeddingPort,
};

#[derive(Clone)]
pub struct HealthDi {
    pub health_check_use_case: Arc<HealthCheckUseCase>,
}

pub fn build(db_pool: SqlitePool) -> HealthDi {
    // Health uses degraded ports — the real check is that DB is reachable.
    // Neither model is wired here, so both report not ready.
    let embedding = Arc::new(MockEmbeddingPort::new_degraded()) as Arc<dyn EmbeddingPort>;
    let llm = Arc::new(DegradedLLMPort) as Arc<dyn LLMPort>;

    HealthDi {
        health_check_use_case: Arc::new(HealthCheckUseCase::new(db_pool, embedding, llm)),
    }
}

/// Health's registrar surface on `Container`.
impl Container {
    // Health (from SystemModule)
    pub fn health_check_use_case(&self) -> Arc<HealthCheckUseCase> {
        Arc::clone(self.system.health_check_use_case())
    }
}
