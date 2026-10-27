//! # Tag Mapper
//!
//! Converts between domain tag models and DTOs.
//!
//! This mapper handles conversion between tag domain models and their DTO representations.
//! Until tag domain models are introduced, this mapper provides
//! the structure for future implementation.

use crate::features::tags::dto::{CreateTagRequestDto, TagDto};
use lattice_core::shared::error::AppError;
use lattice_core::shared::types::TagId;

/// Mapper for tag-related conversions.
pub struct TagMapper;

impl TagMapper {
    /// Create a TagDto from basic components.
    pub fn create_dto(
        id: TagId,
        name: String,
        color: Option<String>,
        description: Option<String>,
    ) -> TagDto {
        TagDto {
            id: id.to_string(),
            name,
            color,
            description,
        }
    }

    /// Convert TagId to string for DTO.
    pub fn id_to_string(tag_id: &TagId) -> String {
        tag_id.to_string()
    }

    /// Convert string to TagId.
    ///
    /// # Errors
    ///
    /// Returns error if the string is not a valid UUID
    pub fn string_to_id(id_str: &str) -> lattice_core::shared::error::Result<TagId> {
        TagId::from_string(id_str.to_string())
            .map_err(|e| AppError::ValidationFailed(format!("Invalid tag ID: {}", e)))
    }

    /// Extract data from create request DTO.
    pub fn from_create_request(
        request: CreateTagRequestDto,
    ) -> (String, Option<String>, Option<String>) {
        (request.name, request.color, request.description)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_dto() {
        let tag_id = TagId::new();
        let dto = TagMapper::create_dto(
            tag_id.clone(),
            "Important".to_string(),
            Some("#FF0000".to_string()),
            Some("Important documents".to_string()),
        );

        assert_eq!(dto.id, tag_id.to_string());
        assert_eq!(dto.name, "Important");
        assert_eq!(dto.color, Some("#FF0000".to_string()));
    }

    #[test]
    fn test_id_to_string() {
        let tag_id = TagId::new();
        let id_str = TagMapper::id_to_string(&tag_id);

        assert!(!id_str.is_empty());
        assert!(uuid::Uuid::parse_str(&id_str).is_ok());
    }

    #[test]
    fn test_string_to_id() {
        let tag_id = TagId::new();
        let id_str = tag_id.to_string();
        let parsed_id = TagMapper::string_to_id(&id_str).unwrap();

        assert_eq!(tag_id.to_string(), parsed_id.to_string());
    }

    #[test]
    fn test_string_to_id_invalid() {
        let result = TagMapper::string_to_id("not-a-uuid");
        assert!(result.is_err());
    }

    #[test]
    fn test_from_create_request() {
        let request = CreateTagRequestDto {
            name: "Work".to_string(),
            color: Some("#0000FF".to_string()),
            description: Some("Work documents".to_string()),
        };

        let (name, color, description) = TagMapper::from_create_request(request);

        assert_eq!(name, "Work");
        assert_eq!(color, Some("#0000FF".to_string()));
        assert_eq!(description, Some("Work documents".to_string()));
    }
}
