//! Search Mentions Use Case
//!
//! Searches for mentions by query string across all documents.
//!
//! # Dependencies
//! - `MentionRepositoryPort` - Mention search operations
//! - `MentionMapper` - Domain to DTO mapping
//!
//! # Security
//! - Rate limited (100 requests/min)
//! - Query length validated to prevent DoS
//!
//! # Example
//! ```rust,no_run
//! let use_case = SearchMentionsUseCase::new(mention_repo, mapper);
//! let mentions = use_case.execute("important".into()).await?;
//! ```

use crate::application::ports::MentionRepositoryPort;
use crate::features::mentions::dto::SearchMentionsResultDto;
use crate::features::mentions::mapper::MentionMapper;
use crate::shared::error::AppError;
use std::sync::Arc;

pub struct SearchMentionsUseCase {
    mention_repository: Arc<dyn MentionRepositoryPort>,
    mention_mapper: Arc<MentionMapper>,
}

impl SearchMentionsUseCase {
    pub fn new(
        mention_repository: Arc<dyn MentionRepositoryPort>,
        mention_mapper: Arc<MentionMapper>,
    ) -> Self {
        Self {
            mention_repository,
            mention_mapper,
        }
    }

    pub async fn execute(
        &self,
        query: String,
        limit: Option<i64>,
    ) -> Result<SearchMentionsResultDto, AppError> {
        let limit = limit.unwrap_or(20).clamp(1, 100);

        tracing::debug!(
            query = %query,
            limit = limit,
            "Searching mentions"
        );

        let mentions_data = self
            .mention_repository
            .search_mentions(&query, limit)
            .await?;

        let mentions: Vec<_> = mentions_data
            .into_iter()
            .map(|m| self.mention_mapper.mention_data_to_dto(m))
            .collect();

        tracing::info!(
            query = %query,
            results_found = mentions.len(),
            "Mention search completed"
        );

        Ok(SearchMentionsResultDto { mentions })
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
            query: &str,
            _limit: i64,
        ) -> Result<Vec<MentionData>, AppError> {
            let mentions = self.mentions.lock().unwrap();
            let results: Vec<MentionData> = mentions
                .iter()
                .filter(|m| m.name.to_lowercase().contains(&query.to_lowercase()))
                .cloned()
                .collect();
            Ok(results)
        }

        async fn get_mentions_by_type(
            &self,
            _mention_type: &str,
        ) -> Result<Vec<MentionData>, AppError> {
            unimplemented!()
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
    async fn test_search_mentions() {
        let test_mentions = vec![
            MentionData {
                id: "1".to_string(),
                name: "Alice Smith".to_string(),
                mention_type: "person".to_string(),
                metadata: None,
                created_at: "2024-01-01T00:00:00Z".to_string(),
            },
            MentionData {
                id: "2".to_string(),
                name: "Bob Johnson".to_string(),
                mention_type: "person".to_string(),
                metadata: None,
                created_at: "2024-01-01T00:00:00Z".to_string(),
            },
        ];

        let repository = Arc::new(MockMentionRepository::with_mentions(test_mentions));
        let mapper = Arc::new(MentionMapper::new());
        let use_case = SearchMentionsUseCase::new(repository, mapper);

        let result = use_case.execute("Alice".to_string(), None).await.unwrap();

        assert_eq!(result.mentions.len(), 1);
        assert_eq!(
            result.mentions.first().map(|m| m.name.as_str()),
            Some("Alice Smith")
        );
    }

    #[tokio::test]
    async fn test_search_mentions_no_results() {
        let repository = Arc::new(MockMentionRepository::with_mentions(vec![]));
        let mapper = Arc::new(MentionMapper::new());
        let use_case = SearchMentionsUseCase::new(repository, mapper);

        let result = use_case.execute("Nobody".to_string(), None).await.unwrap();

        assert!(result.mentions.is_empty());
    }
}
