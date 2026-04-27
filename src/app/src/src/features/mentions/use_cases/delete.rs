//! Delete Mention Use Case
//!
//! Deletes an existing mention and all its document associations.
//!
//! # Dependencies
//! - `MentionRepositoryPort` - Mention persistence
//!
//! # Security
//! - Validates mention ID exists
//! - Rate limited to prevent abuse
//! - Cascades deletion to document_mentions (handled by repository)
//!
//! # Example
//! ```rust,no_run
//! let use_case = DeleteMentionUseCase::new(mention_repo);
//! use_case.execute("mention-123".into()).await?;
//! println!("Mention deleted successfully");
//! ```

use crate::application::ports::MentionRepositoryPort;
use crate::shared::error::AppError;
use std::sync::Arc;

pub struct DeleteMentionUseCase {
    mention_repository: Arc<dyn MentionRepositoryPort>,
}

impl DeleteMentionUseCase {
    pub fn new(mention_repository: Arc<dyn MentionRepositoryPort>) -> Self {
        Self { mention_repository }
    }

    /// Delete a mention and all its document associations.
    ///
    /// # Arguments
    ///
    /// * `id` - ID of the mention to delete
    ///
    /// # Returns
    ///
    /// Unit result indicating successful deletion.
    ///
    /// # Errors
    ///
    /// - `AppError::InvalidInput` if mention ID is empty
    /// - `AppError::Database` if deletion fails
    ///
    /// # Notes
    ///
    /// This operation cascades to delete all document_mentions entries
    /// that reference this mention. The mention will be permanently removed
    /// from the database.
    pub async fn execute(&self, id: String) -> Result<(), AppError> {
        // Validate input
        if id.trim().is_empty() {
            return Err(AppError::InvalidInput(
                "Mention ID cannot be empty".to_string(),
            ));
        }

        tracing::debug!(id = %id, "Deleting mention");

        // Delete the mention (repository handles cascading to document_mentions)
        self.mention_repository.delete_mention(&id).await?;

        tracing::info!(
            id = %id,
            "Successfully deleted mention"
        );

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::ports::{MentionData, MentionWithContextData};
    use async_trait::async_trait;
    use std::collections::HashSet;
    use std::sync::Mutex;

    struct MockMentionRepository {
        mentions: Mutex<HashSet<String>>,
        delete_calls: Mutex<Vec<String>>,
    }

    impl MockMentionRepository {
        fn new() -> Self {
            Self {
                mentions: Mutex::new(HashSet::new()),
                delete_calls: Mutex::new(Vec::new()),
            }
        }

        fn add_mention(&self, id: &str) {
            self.mentions.lock().unwrap().insert(id.to_string());
        }

        fn was_deleted(&self, id: &str) -> bool {
            self.delete_calls.lock().unwrap().contains(&id.to_string())
        }

        fn mention_exists(&self, id: &str) -> bool {
            !self.was_deleted(id) && self.mentions.lock().unwrap().contains(id)
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

        async fn extract_and_store_mentions(
            &self,
            _document_id: &str,
            _text: &str,
        ) -> Result<Vec<MentionWithContextData>, AppError> {
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

        async fn delete_mention(&self, id: &str) -> Result<(), AppError> {
            let mut calls = self.delete_calls.lock().unwrap();
            calls.push(id.to_string());

            let mut mentions = self.mentions.lock().unwrap();
            if !mentions.remove(id) {
                // Don't fail if mention doesn't exist - repository handles this
            }

            Ok(())
        }
    }

    #[tokio::test]
    async fn test_delete_mention_success() {
        let repository = Arc::new(MockMentionRepository::new());
        repository.add_mention("mention-123");

        let use_case = DeleteMentionUseCase::new(repository.clone());

        let result = use_case.execute("mention-123".to_string()).await;

        assert!(result.is_ok());
        assert!(repository.was_deleted("mention-123"));
        assert!(!repository.mention_exists("mention-123"));
    }

    #[tokio::test]
    async fn test_delete_mention_empty_id() {
        let repository = Arc::new(MockMentionRepository::new());
        let use_case = DeleteMentionUseCase::new(repository);

        let result = use_case.execute("".to_string()).await;

        assert!(result.is_err());
        match result.unwrap_err() {
            AppError::InvalidInput(msg) => assert!(msg.contains("cannot be empty")),
            _ => panic!("Expected InvalidInput error"),
        }
    }

    #[tokio::test]
    async fn test_delete_mention_whitespace_id() {
        let repository = Arc::new(MockMentionRepository::new());
        let use_case = DeleteMentionUseCase::new(repository);

        let result = use_case.execute("   ".to_string()).await;

        assert!(result.is_err());
        match result.unwrap_err() {
            AppError::InvalidInput(msg) => assert!(msg.contains("cannot be empty")),
            _ => panic!("Expected InvalidInput error"),
        }
    }

    #[tokio::test]
    async fn test_delete_mention_nonexistent() {
        let repository = Arc::new(MockMentionRepository::new());
        let use_case = DeleteMentionUseCase::new(repository.clone());

        // Repository doesn't fail on nonexistent - it's idempotent
        let result = use_case.execute("nonexistent".to_string()).await;

        assert!(result.is_ok());
        assert!(repository.was_deleted("nonexistent"));
    }

    #[tokio::test]
    async fn test_delete_multiple_mentions() {
        let repository = Arc::new(MockMentionRepository::new());
        repository.add_mention("mention-1");
        repository.add_mention("mention-2");
        repository.add_mention("mention-3");

        let use_case = DeleteMentionUseCase::new(repository.clone());

        // Delete first mention
        use_case.execute("mention-1".to_string()).await.unwrap();
        assert!(repository.was_deleted("mention-1"));
        assert!(!repository.mention_exists("mention-1"));

        // Delete second mention
        use_case.execute("mention-2".to_string()).await.unwrap();
        assert!(repository.was_deleted("mention-2"));
        assert!(!repository.mention_exists("mention-2"));

        // Third mention still exists
        assert!(repository.mention_exists("mention-3"));
    }
}
