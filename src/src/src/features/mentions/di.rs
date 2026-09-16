//! Mentions feature dependency injection.

use std::sync::Arc;

use sqlx::SqlitePool;

use crate::application::ports::MentionRepositoryPort;
use crate::features::mentions::mapper::MentionMapper;
use crate::features::mentions::use_cases::{
    CreateMentionUseCase, DeleteMentionUseCase, ExtractMentionsUseCase, GetBacklinksUseCase,
    GetMentionsByTypeUseCase, GetMentionsForDocumentUseCase, SearchMentionsUseCase,
};
use crate::infrastructure::persistence::repositories::MentionRepository;
use crate::interfaces::di::Container;

#[derive(Clone)]
pub struct MentionsDi {
    pub mention_repo: Arc<dyn MentionRepositoryPort>,
    pub extract_mentions_use_case: Arc<ExtractMentionsUseCase>,
    pub search_mentions_use_case: Arc<SearchMentionsUseCase>,
    pub get_backlinks_use_case: Arc<GetBacklinksUseCase>,
    pub get_mentions_by_type_use_case: Arc<GetMentionsByTypeUseCase>,
    pub get_mentions_for_document_use_case: Arc<GetMentionsForDocumentUseCase>,
    pub create_mention_use_case: Arc<CreateMentionUseCase>,
    pub delete_mention_use_case: Arc<DeleteMentionUseCase>,
}

pub fn build(db_pool: SqlitePool) -> MentionsDi {
    let mention_repo = Arc::new(MentionRepository::new(db_pool)) as Arc<dyn MentionRepositoryPort>;
    let mapper = Arc::new(MentionMapper::new());

    MentionsDi {
        extract_mentions_use_case: Arc::new(ExtractMentionsUseCase::new(
            mention_repo.clone(),
            mapper.clone(),
        )),
        search_mentions_use_case: Arc::new(SearchMentionsUseCase::new(
            mention_repo.clone(),
            mapper.clone(),
        )),
        get_backlinks_use_case: Arc::new(GetBacklinksUseCase::new(mention_repo.clone())),
        get_mentions_by_type_use_case: Arc::new(GetMentionsByTypeUseCase::new(
            mention_repo.clone(),
            mapper.clone(),
        )),
        get_mentions_for_document_use_case: Arc::new(GetMentionsForDocumentUseCase::new(
            mention_repo.clone(),
            mapper.clone(),
        )),
        create_mention_use_case: Arc::new(CreateMentionUseCase::new(
            mention_repo.clone(),
            mapper.clone(),
        )),
        delete_mention_use_case: Arc::new(DeleteMentionUseCase::new(mention_repo.clone())),
        mention_repo,
    }
}

/// Mentions' registrar surface on `Container`.
impl Container {
    // Mentions (from LibraryModule)
    pub fn extract_mentions_use_case(&self) -> Arc<ExtractMentionsUseCase> {
        Arc::clone(self.library.extract_mentions_use_case())
    }

    pub fn search_mentions_use_case(&self) -> Arc<SearchMentionsUseCase> {
        Arc::clone(self.library.search_mentions_use_case())
    }

    pub fn get_backlinks_use_case(&self) -> Arc<GetBacklinksUseCase> {
        Arc::clone(self.library.get_backlinks_use_case())
    }

    pub fn get_mentions_by_type_use_case(&self) -> Arc<GetMentionsByTypeUseCase> {
        Arc::clone(self.library.get_mentions_by_type_use_case())
    }

    pub fn get_mentions_for_document_use_case(&self) -> Arc<GetMentionsForDocumentUseCase> {
        Arc::clone(self.library.get_mentions_for_document_use_case())
    }

    pub fn create_mention_use_case(&self) -> Arc<CreateMentionUseCase> {
        Arc::clone(self.library.create_mention_use_case())
    }

    pub fn delete_mention_use_case(&self) -> Arc<DeleteMentionUseCase> {
        Arc::clone(self.library.delete_mention_use_case())
    }
}
