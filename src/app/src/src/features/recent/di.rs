//! Recent documents feature dependency injection.
//!
//! Tauri commands route through `commands.rs` (raw sqlx). The port +
//! concrete impl exist only because `function_calling/executor.rs`
//! consumes `Arc<dyn RecentDocumentsRepositoryPort>`.

use std::sync::Arc;

use sqlx::SqlitePool;

use crate::application::ports::RecentDocumentsRepositoryPort;
use crate::features::recent::repository::RecentDocumentsRepository;

#[derive(Clone)]
pub struct RecentDi {
    pub recent_docs_repo: Arc<dyn RecentDocumentsRepositoryPort>,
}

pub fn build(db_pool: SqlitePool) -> RecentDi {
    let recent_docs_repo =
        Arc::new(RecentDocumentsRepository::new(db_pool)) as Arc<dyn RecentDocumentsRepositoryPort>;
    RecentDi { recent_docs_repo }
}
