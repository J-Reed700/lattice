//! Stats feature dependency injection.

use std::sync::Arc;

use sqlx::SqlitePool;

use crate::application::ports::{DatabaseStatsPort, RepositoryPort};
use crate::domain::entities::{Chunk, Document as DocumentEntity};
use crate::features::stats::database_stats::DatabaseStatsAdapter;
use crate::features::stats::use_cases::get_system_stats::GetSystemStatsUseCase;
use crate::features::tags::entity::Tag as TagEntity;
use crate::infrastructure::persistence::repositories::{
    ChunkRepositoryImpl, DocumentRepositoryImpl, TagRepositoryImpl,
};

#[derive(Clone)]
pub struct StatsDi {
    pub get_system_stats_use_case: Arc<GetSystemStatsUseCase>,
}

pub fn build(db_pool: SqlitePool) -> StatsDi {
    let document_repo = Arc::new(DocumentRepositoryImpl::new(db_pool.clone()))
        as Arc<dyn RepositoryPort<DocumentEntity>>;
    let chunk_repo =
        Arc::new(ChunkRepositoryImpl::new(db_pool.clone())) as Arc<dyn RepositoryPort<Chunk>>;
    let tag_repo =
        Arc::new(TagRepositoryImpl::new(db_pool.clone())) as Arc<dyn RepositoryPort<TagEntity>>;
    let database_stats =
        Arc::new(DatabaseStatsAdapter::new(db_pool)) as Arc<dyn DatabaseStatsPort>;

    StatsDi {
        get_system_stats_use_case: Arc::new(GetSystemStatsUseCase::new(
            document_repo,
            chunk_repo,
            tag_repo,
            database_stats,
        )),
    }
}
