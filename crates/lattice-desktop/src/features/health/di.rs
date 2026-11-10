//! Health feature dependency injection.

use std::sync::Arc;

use crate::features::health::use_cases::HealthCheckUseCase;
use crate::interfaces::di::Container;
use lattice_core::application::ports::{EmbeddingPort, MockEmbeddingPort};
use lattice_core::shared::persistence::Database;

#[derive(Clone)]
pub struct HealthDi {
    pub health_check_use_case: Arc<HealthCheckUseCase>,
}

pub fn build(database: Database) -> HealthDi {
    // Health uses a degraded embedding port — the real check is that DB is
    // reachable. No embedding model is wired here, so it reports not ready.
    // LLM readiness is read from the loaded models when the check runs.
    let embedding = Arc::new(MockEmbeddingPort::new_degraded()) as Arc<dyn EmbeddingPort>;

    HealthDi {
        health_check_use_case: Arc::new(HealthCheckUseCase::new(database, embedding)),
    }
}

/// Health's registrar surface on `Container`.
impl Container {
    // Health (from SystemModule)
    pub fn health_check_use_case(&self) -> Arc<HealthCheckUseCase> {
        Arc::clone(self.system.health_check_use_case())
    }
}
