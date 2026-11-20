//! Health feature dependency injection.

use std::sync::Arc;

use crate::features::health::use_cases::HealthCheckUseCase;
use crate::interfaces::di::Container;
use lattice_core::application::ports::EmbeddingPort;
use lattice_core::shared::persistence::Database;

#[derive(Clone)]
pub struct HealthDi {
    pub health_check_use_case: Arc<HealthCheckUseCase>,
}

pub fn build(database: Database, embedding: Arc<dyn EmbeddingPort>) -> HealthDi {
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
