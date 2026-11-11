//! Initialization feature dependency injection.
//!
//! Covers only the database bootstrap use case. The
//! `first_run_setup` use case is constructed at a higher orchestration layer.

use std::sync::Arc;

use crate::features::initialization::use_cases::InitializeDatabaseUseCase;
use crate::interfaces::di::Container;
use lattice_core::shared::persistence::Database;

#[derive(Clone)]
pub struct InitializationDi {
    pub initialize_database_use_case: Arc<InitializeDatabaseUseCase>,
}

pub fn build(database: Database) -> InitializationDi {
    InitializationDi {
        initialize_database_use_case: Arc::new(InitializeDatabaseUseCase::new(database)),
    }
}

/// Initialization's registrar surface on `Container`.
impl Container {
    pub fn initialize_database_use_case(&self) -> Arc<InitializeDatabaseUseCase> {
        Arc::clone(self.system.initialize_database_use_case())
    }
}
