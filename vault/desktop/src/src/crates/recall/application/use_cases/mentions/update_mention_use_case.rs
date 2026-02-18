//! Update Mention Use Case
//!
//! Updates an existing mention's type or metadata.
//!
//! # Dependencies
//! - `MentionRepositoryPort` - Mention persistence
//! - `MentionMapper` - Domain to DTO mapping
//!
//! # Security
//! - Validates mention ID exists
//! - Validates mention type is a valid enum value
//! - Rate limited to prevent abuse
//!
//! # Example
//! ```rust,no_run
//! let use_case = UpdateMentionUseCase::new(mention_repo, mapper);
//! let updated = use_case.execute(
//!     "mention-123".into(),
//!     Some("organization".into()),
//!     None
//! ).await?;
//! println!("Updated mention type to: {}", updated.mention_type);
//! ```

use crate::application::dtos::mention_dto::MentionDto;
use crate::application::mappers::mention_mapper::MentionMapper;
use crate::application::ports::MentionRepositoryPort;
use crate::shared::error::AppError;
use std::sync::Arc;

pub struct UpdateMentionUseCase {
    mention_repository: Arc<dyn MentionRepositoryPort>,
    mention_mapper: Arc<MentionMapper>,
}

impl UpdateMentionUseCase {
    pub fn new(
        mention_repository: Arc<dyn MentionRepositoryPort>,
        mention_mapper: Arc<MentionMapper>,
    ) -> Self {
        Self {
            mention_repository,
            mention_mapper,
        }
    }

    /// Update an existing mention.
    ///
    /// # Arguments
    ///
    /// * `id` - ID of the mention to update
    /// * `mention_type` - Optional new type: "person", "organization", "location", or "general"
    /// * `metadata` - Optional new metadata JSON string
    ///
    /// # Returns
    ///
    /// Updated mention DTO.
    ///
    /// # Errors
    ///
    /// - `AppError::InvalidInput` if mention_type is invalid
    /// - `AppError::NotFound` if mention doesn't exist
    /// - `AppError::Database` if update fails
    pub async fn execute(
        &self,
        id: String,
        mention_type: Option<String>,
        metadata: Option<String>,
    ) -> Result<MentionDto, AppError> {
        // Validate input
        if id.trim().is_empty() {
            return Err(AppError::InvalidInput(
                "Mention ID cannot be empty".to_string(),
            ));
        }

        // Validate mention type if provided
        if let Some(ref mtype) = mention_type {
            let valid_types = ["person", "organization", "location", "general"];
            if !valid_types.contains(&mtype.as_str()) {
                return Err(AppError::InvalidInput(format!(
                    "Invalid mention type: {}. Must be one of: person, organization, location, general",
                    mtype
                )));
            }
        }

        tracing::debug!(
            id = %id,
            mention_type = ?mention_type,
            has_metadata = metadata.is_some(),
            "Updating mention"
        );

        let mention_data = self
            .mention_repository
            .update_mention(&id, mention_type.as_deref(), metadata.as_deref())
            .await?;

        let mention_dto = self.mention_mapper.mention_data_to_dto(mention_data);

        tracing::info!(
            id = %mention_dto.id,
            name = %mention_dto.name,
            mention_type = %mention_dto.mention_type,
            "Successfully updated mention"
        );

        Ok(mention_dto)
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
    }

    impl MockMentionRepository {
        fn new() -> Self {
            Self {
                mentions: Mutex::new(HashMap::new()),
            }
        }

        fn add_mention(&self, mention: MentionData) {
            self.mentions
                .lock()
                .unwrap()
                .insert(mention.id.clone(), mention);
        }

        fn get_mention(&self, id: &str) -> Option<MentionData> {
            self.mentions.lock().unwrap().get(id).cloned()
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
            id: &str,
            mention_type: Option<&str>,
            metadata: Option<&str>,
        ) -> Result<MentionData, AppError> {
            let mut mentions = self.mentions.lock().unwrap();
            let mention = mentions
                .get_mut(id)
                .ok_or_else(|| AppError::NotFound(format!("Mention not found: {}", id)))?;

            if let Some(mtype) = mention_type {
                mention.mention_type = mtype.to_string();
            }
            if let Some(meta) = metadata {
                mention.metadata = Some(meta.to_string());
            }

            Ok(mention.clone())
        }

        async fn delete_mention(&self, _id: &str) -> Result<(), AppError> {
            unimplemented!()
        }
    }

    #[tokio::test]
    async fn test_update_mention_type() {
        let repository = Arc::new(MockMentionRepository::new());
        repository.add_mention(MentionData {
            id: "mention-123".to_string(),
            name: "Ambiguous Name".to_string(),
            mention_type: "general".to_string(),
            metadata: None,
            created_at: "2024-01-01T00:00:00Z".to_string(),
        });

        let mapper = Arc::new(MentionMapper::new());
        let use_case = UpdateMentionUseCase::new(repository.clone(), mapper);

        let result = use_case
            .execute("mention-123".to_string(), Some("person".to_string()), None)
            .await
            .unwrap();

        assert_eq!(result.id, "mention-123");
        assert_eq!(result.mention_type, "person");
        assert_eq!(result.name, "Ambiguous Name");

        let updated = repository.get_mention("mention-123").unwrap();
        assert_eq!(updated.mention_type, "person");
    }

    #[tokio::test]
    async fn test_update_mention_metadata() {
        let repository = Arc::new(MockMentionRepository::new());
        repository.add_mention(MentionData {
            id: "mention-456".to_string(),
            name: "Jane Doe".to_string(),
            mention_type: "person".to_string(),
            metadata: None,
            created_at: "2024-01-01T00:00:00Z".to_string(),
        });

        let mapper = Arc::new(MentionMapper::new());
        let use_case = UpdateMentionUseCase::new(repository.clone(), mapper);

        let new_metadata = r#"{"role": "Manager"}"#.to_string();
        let result = use_case
            .execute("mention-456".to_string(), None, Some(new_metadata.clone()))
            .await
            .unwrap();

        assert_eq!(result.id, "mention-456");
        assert_eq!(result.metadata, Some(new_metadata));

        let updated = repository.get_mention("mention-456").unwrap();
        assert!(updated.metadata.is_some());
    }

    #[tokio::test]
    async fn test_update_mention_both_fields() {
        let repository = Arc::new(MockMentionRepository::new());
        repository.add_mention(MentionData {
            id: "mention-789".to_string(),
            name: "Company".to_string(),
            mention_type: "general".to_string(),
            metadata: None,
            created_at: "2024-01-01T00:00:00Z".to_string(),
        });

        let mapper = Arc::new(MentionMapper::new());
        let use_case = UpdateMentionUseCase::new(repository.clone(), mapper);

        let metadata = r#"{"industry": "tech"}"#.to_string();
        let result = use_case
            .execute(
                "mention-789".to_string(),
                Some("organization".to_string()),
                Some(metadata.clone()),
            )
            .await
            .unwrap();

        assert_eq!(result.mention_type, "organization");
        assert_eq!(result.metadata, Some(metadata));
    }

    #[tokio::test]
    async fn test_update_mention_not_found() {
        let repository = Arc::new(MockMentionRepository::new());
        let mapper = Arc::new(MentionMapper::new());
        let use_case = UpdateMentionUseCase::new(repository, mapper);

        let result = use_case
            .execute("nonexistent".to_string(), Some("person".to_string()), None)
            .await;

        assert!(result.is_err());
        match result.unwrap_err() {
            AppError::NotFound(msg) => assert!(msg.contains("not found")),
            _ => panic!("Expected NotFound error"),
        }
    }

    #[tokio::test]
    async fn test_update_mention_invalid_type() {
        let repository = Arc::new(MockMentionRepository::new());
        repository.add_mention(MentionData {
            id: "mention-999".to_string(),
            name: "Test".to_string(),
            mention_type: "general".to_string(),
            metadata: None,
            created_at: "2024-01-01T00:00:00Z".to_string(),
        });

        let mapper = Arc::new(MentionMapper::new());
        let use_case = UpdateMentionUseCase::new(repository, mapper);

        let result = use_case
            .execute("mention-999".to_string(), Some("invalid".to_string()), None)
            .await;

        assert!(result.is_err());
        match result.unwrap_err() {
            AppError::InvalidInput(msg) => assert!(msg.contains("Invalid mention type")),
            _ => panic!("Expected InvalidInput error"),
        }
    }

    #[tokio::test]
    async fn test_update_mention_empty_id() {
        let repository = Arc::new(MockMentionRepository::new());
        let mapper = Arc::new(MentionMapper::new());
        let use_case = UpdateMentionUseCase::new(repository, mapper);

        let result = use_case
            .execute("".to_string(), Some("person".to_string()), None)
            .await;

        assert!(result.is_err());
        match result.unwrap_err() {
            AppError::InvalidInput(msg) => assert!(msg.contains("cannot be empty")),
            _ => panic!("Expected InvalidInput error"),
        }
    }
}
