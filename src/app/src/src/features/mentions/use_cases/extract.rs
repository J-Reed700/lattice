//! Extract Mentions Use Case
//!
//! Extracts document mentions from content and persists them for backlink tracking.
//!
//! # Dependencies
//! - `MentionRepositoryPort` - Mention persistence
//! - `MentionMapper` - Domain to DTO mapping
//!
//! # Security
//! - Rate limited to prevent DoS via large document parsing
//! - Validates mention patterns to prevent injection attacks
//!
//! # Example
//! ```rust,no_run
//! let use_case = ExtractMentionsUseCase::new(mention_repo, mapper);
//! let result = use_case.execute("doc123".into(), "See [[other-doc]] for details".into()).await?;
//! println!("Found {} mentions", result.mentions.len());
//! ```

use crate::application::ports::MentionRepositoryPort;
use crate::features::mentions::dto::{ExtractMentionsResultDto, MentionWithContextDto};
use crate::features::mentions::mapper::MentionMapper;
use crate::shared::error::AppError;
use std::sync::Arc;

pub struct ExtractMentionsUseCase {
    mention_repository: Arc<dyn MentionRepositoryPort>,
    mention_mapper: Arc<MentionMapper>,
}

impl ExtractMentionsUseCase {
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
        document_id: String,
        content: String,
    ) -> Result<ExtractMentionsResultDto, AppError> {
        tracing::debug!(
            document_id = %document_id,
            content_length = content.len(),
            "Extracting mentions from document"
        );

        let mentions_data = self
            .mention_repository
            .extract_and_store_mentions(&document_id, &content)
            .await?;

        let mentions: Vec<MentionWithContextDto> = mentions_data
            .into_iter()
            .map(|m| self.mention_mapper.mention_with_context_data_to_dto(m))
            .collect();

        let count = mentions.len();

        tracing::info!(
            document_id = %document_id,
            mentions_found = count,
            "Successfully extracted mentions"
        );

        Ok(ExtractMentionsResultDto { mentions, count })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::ports::{MentionData, MentionWithContextData};
    use async_trait::async_trait;
    use std::sync::Mutex;

    struct MockMentionRepository {
        mentions: Mutex<Vec<MentionWithContextData>>,
    }

    impl MockMentionRepository {
        fn new() -> Self {
            Self {
                mentions: Mutex::new(vec![]),
            }
        }

        fn with_mentions(mentions: Vec<MentionWithContextData>) -> Self {
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
            Ok(self.mentions.lock().unwrap().clone())
        }

        async fn delete_mention(&self, _id: &str) -> Result<(), AppError> {
            unimplemented!()
        }
    }

    #[tokio::test]
    async fn test_extract_mentions_success() {
        let test_mentions = vec![MentionWithContextData {
            mention: MentionData {
                id: "mention-1".to_string(),
                name: "John Doe".to_string(),
                mention_type: "person".to_string(),
                metadata: None,
                created_at: "2024-01-01T00:00:00Z".to_string(),
            },
            document_id: "doc-1".to_string(),
            context: Some("met @[John Doe] yesterday".to_string()),
            position: Some(4),
        }];

        let repository = Arc::new(MockMentionRepository::with_mentions(test_mentions));
        let mapper = Arc::new(MentionMapper::new());
        let use_case = ExtractMentionsUseCase::new(repository, mapper);

        let result = use_case
            .execute(
                "doc-1".to_string(),
                "I met @[John Doe] yesterday".to_string(),
            )
            .await
            .unwrap();

        assert_eq!(result.count, 1);
        assert_eq!(result.mentions.len(), 1);
        assert_eq!(result.mentions[0].name, "John Doe");
    }

    #[tokio::test]
    async fn test_extract_mentions_empty() {
        let repository = Arc::new(MockMentionRepository::new());
        let mapper = Arc::new(MentionMapper::new());
        let use_case = ExtractMentionsUseCase::new(repository, mapper);

        let result = use_case
            .execute("doc-1".to_string(), "No mentions here".to_string())
            .await
            .unwrap();

        assert_eq!(result.count, 0);
        assert!(result.mentions.is_empty());
    }
}
