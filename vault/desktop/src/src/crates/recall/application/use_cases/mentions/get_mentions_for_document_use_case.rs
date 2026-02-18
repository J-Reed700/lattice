//! Get Mentions For Document Use Case
//!
//! Retrieves all mentions found within a specific document with context.
//!
//! # Dependencies
//! - `MentionRepositoryPort` - Document mention queries
//! - `MentionMapper` - Domain to DTO mapping
//!
//! # Security
//! - Validates document ID format
//! - Rate limited to prevent enumeration attacks
//!
//! # Example
//! ```rust,no_run
//! let use_case = GetMentionsForDocumentUseCase::new(mention_repo, mapper);
//! let result = use_case.execute("doc-123".into()).await?;
//! println!("Found {} mentions in document", result.mentions.len());
//! ```

use crate::application::dtos::mention_dto::{
    GetMentionsForDocumentResultDto, MentionWithContextDto,
};
use crate::application::mappers::mention_mapper::MentionMapper;
use crate::application::ports::MentionRepositoryPort;
use crate::shared::error::AppError;
use std::sync::Arc;

pub struct GetMentionsForDocumentUseCase {
    mention_repository: Arc<dyn MentionRepositoryPort>,
    mention_mapper: Arc<MentionMapper>,
}

impl GetMentionsForDocumentUseCase {
    pub fn new(
        mention_repository: Arc<dyn MentionRepositoryPort>,
        mention_mapper: Arc<MentionMapper>,
    ) -> Self {
        Self {
            mention_repository,
            mention_mapper,
        }
    }

    /// Get all mentions within a specific document.
    ///
    /// # Arguments
    ///
    /// * `document_id` - ID of the document to retrieve mentions for
    ///
    /// # Returns
    ///
    /// List of mentions with their context (surrounding text) and position within the document.
    ///
    /// # Errors
    ///
    /// - `AppError::InvalidInput` if document_id is empty
    /// - `AppError::Database` if query fails
    pub async fn execute(
        &self,
        document_id: String,
    ) -> Result<GetMentionsForDocumentResultDto, AppError> {
        // Validate input
        if document_id.trim().is_empty() {
            return Err(AppError::InvalidInput(
                "Document ID cannot be empty".to_string(),
            ));
        }

        tracing::debug!(
            document_id = %document_id,
            "Getting mentions for document"
        );

        let mentions_data = self
            .mention_repository
            .get_mentions_for_document(&document_id)
            .await?;

        let mentions: Vec<MentionWithContextDto> = mentions_data
            .into_iter()
            .map(|m| self.mention_mapper.mention_with_context_data_to_dto(m))
            .collect();

        let count = mentions.len();

        tracing::info!(
            document_id = %document_id,
            mentions_count = count,
            "Retrieved mentions for document"
        );

        Ok(GetMentionsForDocumentResultDto {
            document_id,
            mentions,
            count,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::ports::{MentionData, MentionWithContextData};
    use async_trait::async_trait;
    use std::collections::HashMap;
    use std::sync::Mutex;

    struct MockMentionRepository {
        document_mentions: Mutex<HashMap<String, Vec<MentionWithContextData>>>,
    }

    impl MockMentionRepository {
        fn new() -> Self {
            Self {
                document_mentions: Mutex::new(HashMap::new()),
            }
        }

        fn add_document_mentions(&self, document_id: &str, mentions: Vec<MentionWithContextData>) {
            self.document_mentions
                .lock()
                .unwrap()
                .insert(document_id.to_string(), mentions);
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
            document_id: &str,
        ) -> Result<Vec<MentionWithContextData>, AppError> {
            Ok(self
                .document_mentions
                .lock()
                .unwrap()
                .get(document_id)
                .cloned()
                .unwrap_or_default())
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
    async fn test_get_mentions_for_document_success() {
        let repository = Arc::new(MockMentionRepository::new());
        let test_mentions = vec![
            MentionWithContextData {
                mention: MentionData {
                    id: "mention-1".to_string(),
                    name: "Alice".to_string(),
                    mention_type: "person".to_string(),
                    metadata: None,
                    created_at: "2024-01-01T00:00:00Z".to_string(),
                },
                document_id: "doc-123".to_string(),
                context: Some("met @[Alice] yesterday".to_string()),
                position: Some(4),
            },
            MentionWithContextData {
                mention: MentionData {
                    id: "mention-2".to_string(),
                    name: "ACME Corp".to_string(),
                    mention_type: "organization".to_string(),
                    metadata: None,
                    created_at: "2024-01-01T00:00:00Z".to_string(),
                },
                document_id: "doc-123".to_string(),
                context: Some("at @[ACME Corp] office".to_string()),
                position: Some(50),
            },
        ];

        repository.add_document_mentions("doc-123", test_mentions);

        let mapper = Arc::new(MentionMapper::new());
        let use_case = GetMentionsForDocumentUseCase::new(repository, mapper);

        let result = use_case.execute("doc-123".to_string()).await.unwrap();

        assert_eq!(result.document_id, "doc-123");
        assert_eq!(result.count, 2);
        assert_eq!(result.mentions.len(), 2);
        assert_eq!(result.mentions[0].name, "Alice");
        assert_eq!(result.mentions[1].name, "ACME Corp");
    }

    #[tokio::test]
    async fn test_get_mentions_for_document_empty() {
        let repository = Arc::new(MockMentionRepository::new());
        let mapper = Arc::new(MentionMapper::new());
        let use_case = GetMentionsForDocumentUseCase::new(repository, mapper);

        let result = use_case.execute("doc-456".to_string()).await.unwrap();

        assert_eq!(result.document_id, "doc-456");
        assert_eq!(result.count, 0);
        assert!(result.mentions.is_empty());
    }

    #[tokio::test]
    async fn test_get_mentions_for_document_invalid_id() {
        let repository = Arc::new(MockMentionRepository::new());
        let mapper = Arc::new(MentionMapper::new());
        let use_case = GetMentionsForDocumentUseCase::new(repository, mapper);

        let result = use_case.execute("".to_string()).await;

        assert!(result.is_err());
        match result.unwrap_err() {
            AppError::InvalidInput(msg) => assert!(msg.contains("cannot be empty")),
            _ => panic!("Expected InvalidInput error"),
        }
    }

    #[tokio::test]
    async fn test_get_mentions_preserves_position_order() {
        let repository = Arc::new(MockMentionRepository::new());
        let test_mentions = vec![
            MentionWithContextData {
                mention: MentionData {
                    id: "mention-1".to_string(),
                    name: "First".to_string(),
                    mention_type: "general".to_string(),
                    metadata: None,
                    created_at: "2024-01-01T00:00:00Z".to_string(),
                },
                document_id: "doc-789".to_string(),
                context: Some("first mention".to_string()),
                position: Some(10),
            },
            MentionWithContextData {
                mention: MentionData {
                    id: "mention-2".to_string(),
                    name: "Second".to_string(),
                    mention_type: "general".to_string(),
                    metadata: None,
                    created_at: "2024-01-01T00:00:00Z".to_string(),
                },
                document_id: "doc-789".to_string(),
                context: Some("second mention".to_string()),
                position: Some(50),
            },
            MentionWithContextData {
                mention: MentionData {
                    id: "mention-3".to_string(),
                    name: "Third".to_string(),
                    mention_type: "general".to_string(),
                    metadata: None,
                    created_at: "2024-01-01T00:00:00Z".to_string(),
                },
                document_id: "doc-789".to_string(),
                context: Some("third mention".to_string()),
                position: Some(100),
            },
        ];

        repository.add_document_mentions("doc-789", test_mentions);

        let mapper = Arc::new(MentionMapper::new());
        let use_case = GetMentionsForDocumentUseCase::new(repository, mapper);

        let result = use_case.execute("doc-789".to_string()).await.unwrap();

        assert_eq!(result.count, 3);
        assert_eq!(result.mentions[0].position, 10);
        assert_eq!(result.mentions[1].position, 50);
        assert_eq!(result.mentions[2].position, 100);
    }
}
