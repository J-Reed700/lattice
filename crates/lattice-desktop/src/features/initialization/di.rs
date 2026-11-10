//! Initialization feature dependency injection.
//!
//! Covers only the database + models bootstrap use cases. The
//! `first_run_setup` use case is constructed at a higher orchestration layer.

use std::sync::Arc;

use crate::features::initialization::use_cases::{
    InitializeDatabaseUseCase, InitializeModelsUseCase,
};
use crate::infrastructure::services::traits::ModelManagerTrait;
use crate::interfaces::di::Container;
use lattice_core::shared::persistence::Database;

#[derive(Clone)]
pub struct InitializationDi {
    pub initialize_database_use_case: Arc<InitializeDatabaseUseCase>,
    pub initialize_models_use_case: Arc<InitializeModelsUseCase>,
}

pub fn build(database: Database, model_manager: Arc<dyn ModelManagerTrait>) -> InitializationDi {
    InitializationDi {
        initialize_database_use_case: Arc::new(InitializeDatabaseUseCase::new(database)),
        initialize_models_use_case: Arc::new(InitializeModelsUseCase::new(model_manager)),
    }
}

/// Initialization's registrar surface on `Container`.
impl Container {
    pub fn initialize_database_use_case(&self) -> Arc<InitializeDatabaseUseCase> {
        Arc::clone(self.system.initialize_database_use_case())
    }

    pub fn initialize_models_use_case(&self) -> Arc<InitializeModelsUseCase> {
        Arc::clone(self.system.initialize_models_use_case())
    }
}
