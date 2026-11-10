//! Recent documents feature dependency injection.
//!
//! Commands and tool execution share the same recent-document repository port.

use std::sync::Arc;

use crate::features::recent::repository::RecentDocumentsRepository;
use crate::interfaces::di::Container;
use lattice_core::application::ports::RecentDocumentsRepositoryPort;
use lattice_core::shared::persistence::DatabaseHandle;

#[derive(Clone)]
pub struct RecentDi {
    pub recent_docs_repo: Arc<dyn RecentDocumentsRepositoryPort>,
}

pub fn build(db_pool: impl DatabaseHandle) -> RecentDi {
    let db_pool = db_pool.database();
    let recent_docs_repo =
        Arc::new(RecentDocumentsRepository::new(db_pool)) as Arc<dyn RecentDocumentsRepositoryPort>;
    RecentDi { recent_docs_repo }
}

/// Recent documents' registrar surface on `Container`.
impl Container {
    pub fn recent_documents_repository(&self) -> Arc<dyn RecentDocumentsRepositoryPort> {
        Arc::clone(self.library.recent_docs_repo())
    }
}
