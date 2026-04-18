//! Recent documents feature dependency injection.

use std::sync::Arc;

use sqlx::SqlitePool;

use crate::application::ports::RecentDocumentsRepositoryPort;
use crate::features::recent::repository::RecentDocumentsRepository;
use crate::features::recent::use_cases::{
    ClearRecentHistoryUseCase, GetRecentDocumentsUseCase, TrackAccessUseCase,
};

#[derive(Clone)]
pub struct RecentDi {
    pub recent_docs_repo: Arc<dyn RecentDocumentsRepositoryPort>,
    pub track_access_use_case: Arc<TrackAccessUseCase>,
    pub get_recent_documents_use_case: Arc<GetRecentDocumentsUseCase>,
    pub clear_recent_history_use_case: Arc<ClearRecentHistoryUseCase>,
}

pub fn build(db_pool: SqlitePool) -> RecentDi {
    let recent_docs_repo = Arc::new(RecentDocumentsRepository::new(db_pool))
        as Arc<dyn RecentDocumentsRepositoryPort>;

    RecentDi {
        track_access_use_case: Arc::new(TrackAccessUseCase::new(recent_docs_repo.clone())),
        get_recent_documents_use_case: Arc::new(GetRecentDocumentsUseCase::new(
            recent_docs_repo.clone(),
        )),
        clear_recent_history_use_case: Arc::new(ClearRecentHistoryUseCase::new(
            recent_docs_repo.clone(),
        )),
        recent_docs_repo,
    }
}
