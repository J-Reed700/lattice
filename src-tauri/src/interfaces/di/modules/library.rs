//! The library module: tags, mentions, backlinks, favorites and recent documents.

use super::CoreModule;
use sqlx::SqlitePool;
use std::sync::Arc;

// Application Use Cases - Tags
use crate::features::tags::use_cases::{
    ApplyTagsUseCase, CreateTagUseCase, DeleteTagUseCase, GetTagsUseCase,
    RemoveTagFromDocumentUseCase, SearchByTagUseCase, UpdateTagUseCase,
};

// (Favorites and Recent: no use cases — Tauri commands route through
// raw sqlx in commands.rs. Repository ports are still wired below for
// the function-calling executor.)

// Application Use Cases - Mentions
use crate::features::mentions::use_cases::{
    CreateMentionUseCase, DeleteMentionUseCase, ExtractMentionsUseCase, GetBacklinksUseCase,
    GetMentionsByTypeUseCase, GetMentionsForDocumentUseCase, SearchMentionsUseCase,
};

// Application Ports
use crate::application::ports::{
    FavoritesRepositoryPort, MentionRepositoryPort, RecentDocumentsRepositoryPort,
};

// Service Traits
use crate::features::tags::TagServiceTrait;

/// Library module for document organization (tags, favorites, mentions)
///
/// **Fields**: ~20
/// - 18 use cases (tags, favorites, recent, mentions)
/// - 1 service (tag service)
/// - 4 repositories
#[derive(Clone)]
pub struct LibraryModule {
    tags: crate::features::tags::di::TagsDi,
    favorites: crate::features::favorites::di::FavoritesDi,
    recent: crate::features::recent::di::RecentDi,
    mentions: crate::features::mentions::di::MentionsDi,
}

impl LibraryModule {
    /// Build LibraryModule by composing feature-local DI builders.
    pub async fn new(
        db_pool: SqlitePool,
        _core: Arc<CoreModule>,
    ) -> crate::shared::error::Result<Self> {
        Ok(Self {
            tags: crate::features::tags::di::build(db_pool.clone()),
            favorites: crate::features::favorites::di::build(db_pool.clone()),
            recent: crate::features::recent::di::build(db_pool.clone()),
            mentions: crate::features::mentions::di::build(db_pool),
        })
    }

    // Tag use case getters
    pub fn create_tag_use_case(&self) -> &Arc<CreateTagUseCase> {
        &self.tags.create_tag_use_case
    }

    pub fn update_tag_use_case(&self) -> &Arc<UpdateTagUseCase> {
        &self.tags.update_tag_use_case
    }

    pub fn delete_tag_use_case(&self) -> &Arc<DeleteTagUseCase> {
        &self.tags.delete_tag_use_case
    }

    pub fn remove_tag_from_document_use_case(&self) -> &Arc<RemoveTagFromDocumentUseCase> {
        &self.tags.remove_tag_from_document_use_case
    }

    pub fn get_tags_use_case(&self) -> &Arc<GetTagsUseCase> {
        &self.tags.get_tags_use_case
    }

    pub fn apply_tags_use_case(&self) -> &Arc<ApplyTagsUseCase> {
        &self.tags.apply_tags_use_case
    }

    pub fn search_by_tag_use_case(&self) -> &Arc<SearchByTagUseCase> {
        &self.tags.search_by_tag_use_case
    }

    // (Favorites + Recent: no use case getters — Tauri commands route
    // through raw sqlx in their respective commands.rs files. The
    // repository ports are exposed below for the function-calling
    // executor.)

    // Mentions use case getters
    pub fn extract_mentions_use_case(&self) -> &Arc<ExtractMentionsUseCase> {
        &self.mentions.extract_mentions_use_case
    }

    pub fn search_mentions_use_case(&self) -> &Arc<SearchMentionsUseCase> {
        &self.mentions.search_mentions_use_case
    }

    pub fn get_backlinks_use_case(&self) -> &Arc<GetBacklinksUseCase> {
        &self.mentions.get_backlinks_use_case
    }

    pub fn get_mentions_by_type_use_case(&self) -> &Arc<GetMentionsByTypeUseCase> {
        &self.mentions.get_mentions_by_type_use_case
    }

    pub fn get_mentions_for_document_use_case(&self) -> &Arc<GetMentionsForDocumentUseCase> {
        &self.mentions.get_mentions_for_document_use_case
    }

    pub fn favorites_repo(&self) -> &Arc<dyn FavoritesRepositoryPort> {
        &self.favorites.favorites_repo
    }

    pub fn recent_docs_repo(&self) -> &Arc<dyn RecentDocumentsRepositoryPort> {
        &self.recent.recent_docs_repo
    }

    pub fn create_mention_use_case(&self) -> &Arc<CreateMentionUseCase> {
        &self.mentions.create_mention_use_case
    }

    pub fn delete_mention_use_case(&self) -> &Arc<DeleteMentionUseCase> {
        &self.mentions.delete_mention_use_case
    }

    // Service getters
    pub fn tag_service(&self) -> &Arc<dyn TagServiceTrait> {
        &self.tags.tag_service
    }

    // Repository getters needed by Container
    pub fn mention_repo(&self) -> &Arc<dyn MentionRepositoryPort> {
        &self.mentions.mention_repo
    }
}
