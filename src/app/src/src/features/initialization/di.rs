//! Initialization feature dependency injection.
//!
//! Covers only the database + models bootstrap use cases. The
//! `first_run_setup` use case is constructed at a higher orchestration layer.

use std::sync::Arc;

use sqlx::SqlitePool;

use crate::features::initialization::use_cases::{InitializeDatabaseUseCase, InitializeModelsUseCase};
use crate::infrastructure::services::traits::ModelManagerTrait;

#[derive(Clone)]
pub struct InitializationDi {
    pub initialize_database_use_case: Arc<InitializeDatabaseUseCase>,
    pub initialize_models_use_case: Arc<InitializeModelsUseCase>,
}

pub fn build(
    db_pool: SqlitePool,
    model_manager: Arc<dyn ModelManagerTrait>,
) -> InitializationDi {
    InitializationDi {
        initialize_database_use_case: Arc::new(InitializeDatabaseUseCase::new(Arc::new(db_pool))),
        initialize_models_use_case: Arc::new(InitializeModelsUseCase::new(model_manager)),
    }
}
