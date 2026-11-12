//! Tags feature dependency injection.
//!
//! Owns construction of the tag service, use cases, and related wiring.
//! Composition roots (see `interfaces::di::modules::LibraryModule`) hold the
//! resulting [`TagsDi`] struct and expose getters that delegate into it.
//!
//! AI-powered tag use cases (generate, auto-tag-all) depend on a model provider
//! and are built via [`build_ai`]; they live in the AI composition root.

use std::sync::Arc;

use lattice_core::shared::persistence::Database;

use crate::features::tags::service::TagService;
use crate::features::tags::tag_generation::TagGenerationService;
use crate::features::tags::use_cases::{
    ApplyTagsUseCase, AutoTagAllDocumentsUseCase, CreateTagUseCase, DeleteTagUseCase,
    GenerateTagsUseCase, GetTagsUseCase, RemoveTagFromDocumentUseCase, SearchByTagUseCase,
    UpdateTagUseCase,
};
use crate::interfaces::di::Container;
use lattice_core::application::ports::LoadedChatModelPort;
use lattice_core::application::ports::TagServiceTrait;

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

/// The one tag service. Every feature that writes a document's tags is handed this
/// instance, so they all share its per-document lock.
pub fn build_service(database: Database) -> Arc<dyn TagServiceTrait> {
    Arc::new(TagService::new(database))
}

pub fn build(tag_service: Arc<dyn TagServiceTrait>) -> TagsDi {
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

/// AI-powered tag wiring: generation reads the currently-loaded model and writes
/// through the shared tag service.
#[derive(Clone)]
pub struct AiTagsDi {
    pub generate_tags_use_case: Arc<GenerateTagsUseCase>,
    pub auto_tag_all_documents_use_case: Arc<AutoTagAllDocumentsUseCase>,
}

pub fn build_ai(
    database: Database,
    tag_service: Arc<dyn TagServiceTrait>,
    model_provider: Arc<dyn LoadedChatModelPort>,
) -> AiTagsDi {
    let generation = Arc::new(TagGenerationService::new(
        database,
        tag_service,
        model_provider,
    ));

    AiTagsDi {
        generate_tags_use_case: Arc::new(GenerateTagsUseCase::new(generation.clone())),
        auto_tag_all_documents_use_case: Arc::new(AutoTagAllDocumentsUseCase::new(generation)),
    }
}

/// Tags' registrar surface on `Container`.
impl Container {
    // Tags (from LibraryModule, except AI-powered tags from AIModule)
    pub fn create_tag_use_case(&self) -> Arc<CreateTagUseCase> {
        Arc::clone(self.library.create_tag_use_case())
    }

    pub fn update_tag_use_case(&self) -> Arc<UpdateTagUseCase> {
        Arc::clone(self.library.update_tag_use_case())
    }

    pub fn delete_tag_use_case(&self) -> Arc<DeleteTagUseCase> {
        Arc::clone(self.library.delete_tag_use_case())
    }

    pub fn remove_tag_from_document_use_case(&self) -> Arc<RemoveTagFromDocumentUseCase> {
        Arc::clone(self.library.remove_tag_from_document_use_case())
    }

    pub fn get_tags_use_case(&self) -> Arc<GetTagsUseCase> {
        Arc::clone(self.library.get_tags_use_case())
    }

    pub fn apply_tags_use_case(&self) -> Arc<ApplyTagsUseCase> {
        Arc::clone(self.library.apply_tags_use_case())
    }

    pub fn search_by_tag_use_case(&self) -> Arc<SearchByTagUseCase> {
        Arc::clone(self.library.search_by_tag_use_case())
    }

    // AI-powered tags (from AIModule)
    pub fn generate_tags_use_case(&self) -> Arc<GenerateTagsUseCase> {
        Arc::clone(self.ai.generate_tags_use_case())
    }

    pub fn auto_tag_all_documents_use_case(&self) -> Arc<AutoTagAllDocumentsUseCase> {
        Arc::clone(self.ai.auto_tag_all_documents_use_case())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::tags::dto::AutoTagRequestDto;
    use crate::infrastructure::persistence::repositories::DocumentRepository;
    use async_trait::async_trait;
    use lattice_core::application::ports::llm_port::{CompletionRequest, CompletionResponse};
    use lattice_core::application::ports::{LLMPort, RepositoryPort};
    use lattice_core::domain::entities::{chunk::Chunk, Document};
    use lattice_core::domain::value_objects::Checksum;
    use lattice_core::shared::error::Result;

    struct Suggests;

    #[async_trait]
    impl LLMPort for Suggests {
        async fn complete(&self, _request: &CompletionRequest) -> Result<CompletionResponse> {
            Ok(CompletionResponse::from_text("rust, ownership"))
        }
        fn model_name(&self) -> &str {
            "suggests"
        }
        fn max_context_tokens(&self) -> usize {
            4096
        }
        async fn is_ready(&self) -> Result<bool> {
            Ok(true)
        }
    }

    struct Loaded;

    impl LoadedChatModelPort for Loaded {
        fn current_model(&self) -> Option<Arc<dyn LLMPort>> {
            Some(Arc::new(Suggests))
        }
    }

    #[tokio::test]
    async fn auto_tagging_applies_tags_only_when_the_document_lock_is_free() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect(":memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        let metadata = Document::new(
            lattice_core::shared::types::ValidatedFilePath::new(std::path::PathBuf::from(
                "/notes.txt",
            ))
            .unwrap(),
            "notes.txt".into(),
            "text/plain".into(),
            12,
            Checksum::new("a".repeat(64)).unwrap(),
        );
        let chunk = Chunk::new(metadata.id().clone(), "Ownership in Rust".into(), 0);
        let document = metadata.from_parts(vec![chunk], vec![]).unwrap();
        let document_id = document.id().as_str().to_string();
        let database = Database::new(pool);
        DocumentRepository::new(database.clone())
            .save_batch(std::slice::from_ref(&document))
            .await
            .unwrap();
        let tag_service = build_service(database.clone());
        let tags = build(tag_service.clone());
        let ai = build_ai(database, tag_service, Arc::new(Loaded));

        let held = tags
            .tag_service
            .acquire_lock_with_timeout(&document_id)
            .await
            .unwrap();
        let auto_tag = ai.auto_tag_all_documents_use_case.clone();
        let mut tagging = tokio::spawn(async move {
            auto_tag
                .execute(AutoTagRequestDto { max_documents: 10 })
                .await
        });

        let early = tokio::time::timeout(std::time::Duration::from_millis(300), &mut tagging).await;
        assert!(
            early.is_err(),
            "tags were applied while the document was locked"
        );

        drop(held);
        let response = tokio::time::timeout(std::time::Duration::from_secs(10), tagging)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert_eq!(response.tagged_count, 1);
        let mut names: Vec<String> = tags
            .tag_service
            .get_tags_for_document(&document_id)
            .await
            .unwrap()
            .iter()
            .map(|tag| tag.name().as_str().to_string())
            .collect();
        names.sort();
        assert_eq!(names, ["ownership", "rust"]);
    }
}
