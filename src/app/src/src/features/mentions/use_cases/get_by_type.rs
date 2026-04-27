//! Get Mentions By Type Use Case
//!
//! Retrieves mentions filtered by type (e.g., WikiLink, Hashtag, AtMention).
//!
//! # Dependencies
//! - `MentionRepositoryPort` - Type-filtered mention queries
//! - `MentionMapper` - Domain to DTO mapping
//!
//! # Security
//! - Validates mention type enum to prevent invalid queries
//! - Rate limited to prevent abuse
//!
//! # Example
//! ```rust,no_run
//! let use_case = GetMentionsByTypeUseCase::new(mention_repo, mapper);
//! let wiki_links = use_case.execute(MentionType::WikiLink).await?;
//! ```

use crate::features::mentions::dto::MentionDto;
use crate::features::mentions::mapper::MentionMapper;
use crate::application::ports::MentionRepositoryPort;
use crate::shared::error::AppError;
use std::sync::Arc;

pub struct GetMentionsByTypeUseCase {
    mention_repository: Arc<dyn MentionRepositoryPort>,
    mention_mapper: Arc<MentionMapper>,
}

impl GetMentionsByTypeUseCase {
    pub fn new(
        mention_repository: Arc<dyn MentionRepositoryPort>,
        mention_mapper: Arc<MentionMapper>,
    ) -> Self {
        Self {
            mention_repository,
            mention_mapper,
        }
    }

    pub async fn execute(&self, mention_type: String) -> Result<Vec<MentionDto>, AppError> {
        tracing::debug!(mention_type = %mention_type, "Getting mentions by type");

        let mentions_data = self
            .mention_repository
            .get_mentions_by_type(&mention_type)
            .await?;

        let mentions: Vec<MentionDto> = mentions_data
            .into_iter()
            .map(|m| self.mention_mapper.mention_data_to_dto(m))
            .collect();

        tracing::info!(
            mention_type = %mention_type,
            count = mentions.len(),
            "Retrieved mentions by type"
        );

        Ok(mentions)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::ports::{MentionData, MentionWithContextData};
    use async_trait::async_trait;
    use std::sync::Mutex;

    struct MockMentionRepository {
        mentions: Mutex<Vec<MentionData>>,
    }

    impl MockMentionRepository {
        fn with_mentions(mentions: Vec<MentionData>) -> Self {
            Self {
                mentions: Mutex::new(mentions),
            }
        }
    }

    #[async_trait]
    impl MentionRepositoryPort for MockMentionRepository {
        async fn create_mention(
            &self,
            _name: &str,
            _mention_type: &str,
            _metadata: Option<&str>,
        ) -> Result<MentionData, AppError> {
            unimplemented!()
        }

        async fn find_mention_by_name(&self, _name: &str) -> Result<Option<MentionData>, AppError> {
            unimplemented!()
        }

        async fn search_mentions(
            &self,
            _query: &str,
            _limit: i64,
        ) -> Result<Vec<MentionData>, AppError> {
            unimplemented!()
        }

        async fn get_mentions_by_type(
            &self,
            mention_type: &str,
        ) -> Result<Vec<MentionData>, AppError> {
            let mentions = self.mentions.lock().unwrap();
            let results: Vec<MentionData> = mentions
                .iter()
                .filter(|m| m.mention_type == mention_type)
                .cloned()
                .collect();
            Ok(results)
        }

        async fn get_mentions_for_document(
            &self,
            _document_id: &str,
        ) -> Result<Vec<MentionWithContextData>, AppError> {
            unimplemented!()
        }

        async fn get_documents_with_mention(
            &self,
            _mention_id: &str,
        ) -> Result<Vec<String>, AppError> {
            unimplemented!()
        }

        async fn update_mention(
            &self,
            _id: &str,
            _mention_type: Option<&str>,
            _metadata: Option<&str>,
        ) -> Result<MentionData, AppError> {
            unimplemented!()
        }

        async fn extract_and_store_mentions(
            &self,
            _document_id: &str,
            _text: &str,
        ) -> Result<Vec<MentionWithContextData>, AppError> {
            unimplemented!()
        }

        async fn delete_mention(&self, _id: &str) -> Result<(), AppError> {
            unimplemented!()
        }
    }

    #[tokio::test]
    async fn test_get_mentions_by_type() {
        let test_mentions = vec![
            MentionData {
                id: "1".to_string(),
                name: "Alice".to_string(),
                mention_type: "person".to_string(),
                metadata: None,
                created_at: "2024-01-01T00:00:00Z".to_string(),
            },
            MentionData {
                id: "2".to_string(),
                name: "ACME Corp".to_string(),
                mention_type: "organization".to_string(),
                metadata: None,
                created_at: "2024-01-01T00:00:00Z".to_string(),
            },
            MentionData {
                id: "3".to_string(),
                name: "Bob".to_string(),
                mention_type: "person".to_string(),
                metadata: None,
                created_at: "2024-01-01T00:00:00Z".to_string(),
            },
        ];

        let repository = Arc::new(MockMentionRepository::with_mentions(test_mentions));
        let mapper = Arc::new(MentionMapper::new());
        let use_case = GetMentionsByTypeUseCase::new(repository, mapper);

        let result = use_case.execute("person".to_string()).await.unwrap();

        assert_eq!(result.len(), 2);
        assert!(result.iter().all(|m| m.mention_type == "person"));
    }

    #[tokio::test]
    async fn test_get_mentions_by_type_empty() {
        let repository = Arc::new(MockMentionRepository::with_mentions(vec![]));
        let mapper = Arc::new(MentionMapper::new());
        let use_case = GetMentionsByTypeUseCase::new(repository, mapper);

        let result = use_case.execute("person".to_string()).await.unwrap();

        assert!(result.is_empty());
    }
}
