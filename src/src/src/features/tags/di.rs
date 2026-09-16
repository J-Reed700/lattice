//! Tags feature dependency injection.
//!
//! Owns construction of the tag service, use cases, and related wiring.
//! Composition roots (see `interfaces::di::modules::LibraryModule`) hold the
//! resulting [`TagsDi`] struct and expose getters that delegate into it.
//!
//! AI-powered tag use cases (generate, auto-tag-all) depend on the LLM cache
//! and are built via [`build_ai`]; they live in the AI composition root.

use std::sync::{Arc, RwLock};

use sqlx::SqlitePool;

use crate::application::ports::LLMPort;
use crate::features::tags::service::TagService;
use crate::features::tags::service_impl::TagServiceImpl;
use crate::features::tags::trait_def::TagServiceTrait;
use crate::features::tags::use_cases::{
    ApplyTagsUseCase, AutoTagAllDocumentsUseCase, CreateTagUseCase, DeleteTagUseCase,
    GenerateTagsUseCase, GetTagsUseCase, RemoveTagFromDocumentUseCase, SearchByTagUseCase,
    UpdateTagUseCase,
};

#[derive(Clone)]
pub struct TagsDi {
    pub tag_service: Arc<dyn TagServiceTrait>,
    pub create_tag_use_case: Arc<CreateTagUseCase>,
    pub update_tag_use_case: Arc<UpdateTagUseCase>,
    pub delete_tag_use_case: Arc<DeleteTagUseCase>,
    pub remove_tag_from_document_use_case: Arc<RemoveTagFromDocumentUseCase>,
    pub get_tags_use_case: Arc<GetTagsUseCase>,
    pub apply_tags_use_case: Arc<ApplyTagsUseCase>,
    pub search_by_tag_use_case: Arc<SearchByTagUseCase>,
}

pub fn build(db_pool: SqlitePool) -> TagsDi {
    let tag_service = Arc::new(TagService::new(db_pool)) as Arc<dyn TagServiceTrait>;

    TagsDi {
        create_tag_use_case: Arc::new(CreateTagUseCase::new(tag_service.clone())),
        update_tag_use_case: Arc::new(UpdateTagUseCase::new(tag_service.clone())),
        delete_tag_use_case: Arc::new(DeleteTagUseCase::new(tag_service.clone())),
        remove_tag_from_document_use_case: Arc::new(RemoveTagFromDocumentUseCase::new(
            tag_service.clone(),
        )),
        get_tags_use_case: Arc::new(GetTagsUseCase::new(tag_service.clone())),
        apply_tags_use_case: Arc::new(ApplyTagsUseCase::new(tag_service.clone())),
        search_by_tag_use_case: Arc::new(SearchByTagUseCase::new(tag_service.clone())),
        tag_service,
    }
}

/// AI-powered tag wiring: uses [`TagServiceImpl`] which takes the LLM cache
/// so generated tags come from the currently-loaded model.
#[derive(Clone)]
pub struct AiTagsDi {
    pub generate_tags_use_case: Arc<GenerateTagsUseCase>,
    pub auto_tag_all_documents_use_case: Arc<AutoTagAllDocumentsUseCase>,
}

pub fn build_ai(db_pool: SqlitePool, llm_cache: Arc<RwLock<Option<Arc<dyn LLMPort>>>>) -> AiTagsDi {
    let tag_service = Arc::new(TagServiceImpl::new(db_pool, llm_cache)) as Arc<dyn TagServiceTrait>;

    AiTagsDi {
        generate_tags_use_case: Arc::new(GenerateTagsUseCase::new(tag_service.clone())),
        auto_tag_all_documents_use_case: Arc::new(AutoTagAllDocumentsUseCase::new(
            tag_service.clone(),
        )),
    }
}
