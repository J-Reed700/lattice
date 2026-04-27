//! Create Mention Use Case
//!
//! Creates a new mention entity (person, organization, location, or general).
//!
//! # Dependencies
//! - `MentionRepositoryPort` - Mention persistence
//! - `MentionMapper` - Domain to DTO mapping
//!
//! # Security
//! - Validates mention name is not empty
//! - Validates mention type is a valid enum value
//! - Rate limited to prevent abuse
//!
//! # Example
//! ```rust,no_run
//! let use_case = CreateMentionUseCase::new(mention_repo, mapper);
//! let mention = use_case.execute("John Doe".into(), "person".into(), None).await?;
//! println!("Created mention: {}", mention.id);
//! ```

use crate::features::mentions::dto::MentionDto;
use crate::features::mentions::mapper::MentionMapper;
use crate::application::ports::MentionRepositoryPort;
use crate::shared::error::AppError;
use std::sync::Arc;

pub struct CreateMentionUseCase {
    mention_repository: Arc<dyn MentionRepositoryPort>,
    mention_mapper: Arc<MentionMapper>,
}

impl CreateMentionUseCase {
    pub fn new(
        mention_repository: Arc<dyn MentionRepositoryPort>,
        mention_mapper: Arc<MentionMapper>,
    ) -> Self {
        Self {
            mention_repository,
            mention_mapper,
        }
    }

    /// Create a new mention.
    ///
    /// # Arguments
    ///
    /// * `name` - Name of the entity being mentioned (e.g., "John Doe", "ACME Corp")
    /// * `mention_type` - Type of mention: "person", "organization", "location", or "general"
    /// * `metadata` - Optional metadata JSON string
    ///
    /// # Returns
    ///
    /// Created mention DTO with generated ID and timestamp.
    ///
    /// # Errors
    ///
    /// - `AppError::InvalidInput` if name is empty or mention_type is invalid
    /// - `AppError::Database` if storage fails
    pub async fn execute(
        &self,
        name: String,
        mention_type: String,
        metadata: Option<String>,
    ) -> Result<MentionDto, AppError> {
        // Validate input
        if name.trim().is_empty() {
            return Err(AppError::InvalidInput(
                "Mention name cannot be empty".to_string(),
            ));
        }

        // Validate mention type
        let valid_types = ["person", "organization", "location", "general"];
        if !valid_types.contains(&mention_type.as_str()) {
            return Err(AppError::InvalidInput(format!(
                "Invalid mention type: {}. Must be one of: person, organization, location, general",
                mention_type
            )));
        }

        tracing::debug!(
            name = %name,
            mention_type = %mention_type,
            has_metadata = metadata.is_some(),
            "Creating mention"
        );

        let mention_data = self
            .mention_repository
            .create_mention(&name, &mention_type, metadata.as_deref())
            .await?;

        let mention_dto = self.mention_mapper.mention_data_to_dto(mention_data);

        tracing::info!(
            id = %mention_dto.id,
            name = %mention_dto.name,
            mention_type = %mention_dto.mention_type,
            "Successfully created mention"
        );

        Ok(mention_dto)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::ports::{MentionData, MentionWithContextData};
    use async_trait::async_trait;
    use std::sync::Mutex;

    struct MockMentionRepository {
        created_mentions: Mutex<Vec<MentionData>>,
    }

    impl MockMentionRepository {
        fn new() -> Self {
            Self {
                created_mentions: Mutex::new(vec![]),
            }
        }

        fn get_created_mentions(&self) -> Vec<MentionData> {
            self.created_mentions.lock().unwrap().clone()
        }
    }

    #[async_trait]
    impl MentionRepositoryPort for MockMentionRepository {
        async fn create_mention(
            &self,
            name: &str,
            mention_type: &str,
            metadata: Option<&str>,
        ) -> Result<MentionData, AppError> {
            let mention = MentionData {
                id: "mention-123".to_string(),
                name: name.to_string(),
                mention_type: mention_type.to_string(),
                metadata: metadata.map(|s| s.to_string()),
                created_at: "2024-01-01T00:00:00Z".to_string(),
            };
            self.created_mentions.lock().unwrap().push(mention.clone());
            Ok(mention)
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

        async fn update_mention(
            &self,
            _id: &str,
            _mention_type: Option<&str>,
            _metadata: Option<&str>,
        ) -> Result<MentionData, AppError> {
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

        async fn delete_mention(&self, _id: &str) -> Result<(), AppError> {
            unimplemented!()
        }
    }

    #[tokio::test]
    async fn test_create_mention_person() {
        let repository = Arc::new(MockMentionRepository::new());
        let mapper = Arc::new(MentionMapper::new());
        let use_case = CreateMentionUseCase::new(repository.clone(), mapper);

        let result = use_case
            .execute("John Doe".to_string(), "person".to_string(), None)
            .await
            .unwrap();

        assert_eq!(result.name, "John Doe");
        assert_eq!(result.mention_type, "person");
        assert_eq!(result.id, "mention-123");

        let created = repository.get_created_mentions();
        assert_eq!(created.len(), 1);
        assert_eq!(created[0].name, "John Doe");
    }

    #[tokio::test]
    async fn test_create_mention_organization() {
        let repository = Arc::new(MockMentionRepository::new());
        let mapper = Arc::new(MentionMapper::new());
        let use_case = CreateMentionUseCase::new(repository, mapper);

        let result = use_case
            .execute("ACME Corp".to_string(), "organization".to_string(), None)
            .await
            .unwrap();

        assert_eq!(result.name, "ACME Corp");
        assert_eq!(result.mention_type, "organization");
    }

    #[tokio::test]
    async fn test_create_mention_with_metadata() {
        let repository = Arc::new(MockMentionRepository::new());
        let mapper = Arc::new(MentionMapper::new());
        let use_case = CreateMentionUseCase::new(repository.clone(), mapper);

        let metadata = r#"{"role": "CEO"}"#.to_string();
        let result = use_case
            .execute(
                "Jane Smith".to_string(),
                "person".to_string(),
                Some(metadata.clone()),
            )
            .await
            .unwrap();

        assert_eq!(result.name, "Jane Smith");
        assert_eq!(result.metadata, Some(metadata));

        let created = repository.get_created_mentions();
        assert_eq!(created.len(), 1);
        assert!(created[0].metadata.is_some());
    }

    #[tokio::test]
    async fn test_create_mention_empty_name() {
        let repository = Arc::new(MockMentionRepository::new());
        let mapper = Arc::new(MentionMapper::new());
        let use_case = CreateMentionUseCase::new(repository, mapper);

        let result = use_case
            .execute("".to_string(), "person".to_string(), None)
            .await;

        assert!(result.is_err());
        match result.unwrap_err() {
            AppError::InvalidInput(msg) => assert!(msg.contains("cannot be empty")),
            _ => panic!("Expected InvalidInput error"),
        }
    }

    #[tokio::test]
    async fn test_create_mention_invalid_type() {
        let repository = Arc::new(MockMentionRepository::new());
        let mapper = Arc::new(MentionMapper::new());
        let use_case = CreateMentionUseCase::new(repository, mapper);

        let result = use_case
            .execute("Test".to_string(), "invalid_type".to_string(), None)
            .await;

        assert!(result.is_err());
        match result.unwrap_err() {
            AppError::InvalidInput(msg) => assert!(msg.contains("Invalid mention type")),
            _ => panic!("Expected InvalidInput error"),
        }
    }

    #[tokio::test]
    async fn test_create_mention_whitespace_name() {
        let repository = Arc::new(MockMentionRepository::new());
        let mapper = Arc::new(MentionMapper::new());
        let use_case = CreateMentionUseCase::new(repository, mapper);

        let result = use_case
            .execute("   ".to_string(), "person".to_string(), None)
            .await;

        assert!(result.is_err());
        match result.unwrap_err() {
            AppError::InvalidInput(msg) => assert!(msg.contains("cannot be empty")),
            _ => panic!("Expected InvalidInput error"),
        }
    }
}
