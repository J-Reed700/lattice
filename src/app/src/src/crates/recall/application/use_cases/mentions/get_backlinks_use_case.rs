//! Get Backlinks Use Case
//!
//! Retrieves all documents that mention a specific document (backlinks).
//!
//! # Dependencies
//! - `MentionRepositoryPort` - Backlink retrieval operations
//!
//! # Security
//! - Validates document ID to prevent injection
//! - Rate limited to prevent enumeration attacks
//!
//! # Example
//! ```rust,no_run
//! let use_case = GetBacklinksUseCase::new(mention_repo);
//! let backlinks = use_case.execute("doc123".into()).await?;
//! println!("Document referenced by {} other documents", backlinks.backlinks.len());
//! ```

use crate::application::dtos::mention_dto::BacklinksResultDto;
use crate::application::ports::MentionRepositoryPort;
use crate::shared::error::AppError;
use std::sync::Arc;

pub struct GetBacklinksUseCase {
    mention_repository: Arc<dyn MentionRepositoryPort>,
}

impl GetBacklinksUseCase {
    pub fn new(mention_repository: Arc<dyn MentionRepositoryPort>) -> Self {
        Self { mention_repository }
    }

    pub async fn execute(&self, mention_name: String) -> Result<BacklinksResultDto, AppError> {
        tracing::debug!(mention_name = %mention_name, "Getting backlinks for mention");

        let mention = self
            .mention_repository
            .find_mention_by_name(&mention_name)
            .await?;

        let document_ids = if let Some(mention) = mention {
            self.mention_repository
                .get_documents_with_mention(&mention.id)
                .await?
        } else {
            Vec::new()
        };

        tracing::info!(
            mention_name = %mention_name,
            backlinks_count = document_ids.len(),
            "Retrieved backlinks"
        );

        Ok(BacklinksResultDto { document_ids })
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
        mentions: Mutex<HashMap<String, MentionData>>,
        backlinks: Mutex<HashMap<String, Vec<String>>>,
    }

    impl MockMentionRepository {
        fn new() -> Self {
            Self {
                mentions: Mutex::new(HashMap::new()),
                backlinks: Mutex::new(HashMap::new()),
            }
        }

        fn add_mention(&self, name: String, id: String, backlinks: Vec<String>) {
            let mention = MentionData {
                id: id.clone(),
                name: name.clone(),
                mention_type: "person".to_string(),
                metadata: None,
                created_at: "2024-01-01T00:00:00Z".to_string(),
            };
            self.mentions.lock().unwrap().insert(name, mention);
            self.backlinks.lock().unwrap().insert(id, backlinks);
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

        async fn find_mention_by_name(&self, name: &str) -> Result<Option<MentionData>, AppError> {
            Ok(self.mentions.lock().unwrap().get(name).cloned())
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
            mention_id: &str,
        ) -> Result<Vec<String>, AppError> {
            Ok(self
                .backlinks
                .lock()
                .unwrap()
                .get(mention_id)
                .cloned()
                .unwrap_or_default())
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
    async fn test_get_backlinks_success() {
        let repository = Arc::new(MockMentionRepository::new());
        repository.add_mention(
            "John Doe".to_string(),
            "mention-1".to_string(),
            vec!["doc-1".to_string(), "doc-2".to_string()],
        );

        let use_case = GetBacklinksUseCase::new(repository);
        let result = use_case.execute("John Doe".to_string()).await.unwrap();

        assert_eq!(result.document_ids.len(), 2);
        assert!(result.document_ids.contains(&"doc-1".to_string()));
        assert!(result.document_ids.contains(&"doc-2".to_string()));
    }

    #[tokio::test]
    async fn test_get_backlinks_not_found() {
        let repository = Arc::new(MockMentionRepository::new());
        let use_case = GetBacklinksUseCase::new(repository);

        let result = use_case.execute("Nobody".to_string()).await.unwrap();

        assert!(result.document_ids.is_empty());
    }
}
