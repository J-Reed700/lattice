//! # Tag Mapper
//!
//! Maps between database models and domain entities for tags.
//!
//! This mapper implements the transformation layer between the anemic
//! database model (used by SQLx) and the rich domain entity.

use crate::features::tags::entity::Tag as DomainTag;
use crate::shared::domain_types::{TagId, TagName};
use crate::shared::error::{AppError, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Database model for tags.
///
/// This is an anemic data structure that mirrors the database schema.
/// It contains no business logic - only data transfer.
///
/// **Important**: This type should NEVER leak outside the infrastructure layer.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TagModel {
    pub id: String,
    pub name: String,
    pub color: String,
    pub created_at: String,
    pub updated_at: String,
}

/// Mapper for Tag entity and database model.
///
/// Provides bidirectional conversion between domain entities and database models.
pub struct TagMapper;

impl TagMapper {
    /// Convert domain entity to database model.
    ///
    /// # Arguments
    ///
    /// * `entity` - Domain tag entity
    ///
    /// # Returns
    ///
    /// Database model ready for persistence.
    pub fn to_model(entity: &DomainTag) -> TagModel {
        TagModel {
            id: entity.id().as_str().to_string(),
            name: entity.name().as_str().to_string(),
            color: entity.color().to_string(),
            created_at: entity.created_at().to_rfc3339(),
            updated_at: entity.updated_at().to_rfc3339(),
        }
    }

    /// Convert database model to domain entity.
    ///
    /// # Arguments
    ///
    /// * `model` - Database model from SQLx
    ///
    /// # Returns
    ///
    /// Domain entity with business logic.
    ///
    /// # Errors
    ///
    /// - `AppError::InvalidData` if timestamps cannot be parsed
    /// - `AppError::InvalidData` if tag ID is invalid
    /// - `AppError::InvalidData` if tag name is invalid
    pub fn to_entity(model: &TagModel) -> Result<DomainTag> {
        let created_at = DateTime::parse_from_rfc3339(&model.created_at)
            .map_err(|e| AppError::InvalidData(format!("Invalid created_at timestamp: {}", e)))?
            .with_timezone(&Utc);

        let updated_at = DateTime::parse_from_rfc3339(&model.updated_at)
            .map_err(|e| AppError::InvalidData(format!("Invalid updated_at timestamp: {}", e)))?
            .with_timezone(&Utc);

        let id = TagId::from_string(model.id.clone())
            .map_err(|e| AppError::InvalidData(format!("Invalid tag ID: {}", e)))?;

        let name = TagName::new(model.name.clone())
            .map_err(|e| AppError::InvalidData(format!("Invalid tag name: {}", e)))?;

        Ok(DomainTag::with_id(
            id,
            name,
            model.color.clone(),
            created_at,
            updated_at,
        ))
    }

    /// Convert a batch of database models to domain entities.
    ///
    /// Continues processing on errors, collecting all successful conversions.
    ///
    /// # Arguments
    ///
    /// * `models` - Vector of database models
    ///
    /// # Returns
    ///
    /// Vector of domain entities (may be shorter than input if some conversions failed).
    pub fn to_entities(models: &[TagModel]) -> Vec<DomainTag> {
        models
            .iter()
            .filter_map(|model| Self::to_entity(model).ok())
            .collect()
    }

    /// Convert a batch of domain entities to database models.
    ///
    /// # Arguments
    ///
    /// * `entities` - Vector of domain entities
    ///
    /// # Returns
    ///
    /// Vector of database models ready for persistence.
    pub fn to_models(entities: &[DomainTag]) -> Vec<TagModel> {
        entities.iter().map(Self::to_model).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_entity_to_model() {
        let name = TagName::new("rust".to_string()).unwrap();
        let entity = DomainTag::new(name, "#ff5733".to_string());

        let model = TagMapper::to_model(&entity);

        assert_eq!(model.id, entity.id().as_str());
        assert_eq!(model.name, "rust");
        assert_eq!(model.color, "#ff5733");
    }

    #[test]
    fn test_model_to_entity() {
        let now = Utc::now();
        let id = TagId::new();

        let model = TagModel {
            id: id.as_str().to_string(),
            name: "python".to_string(),
            color: "#6366f1".to_string(),
            created_at: now.to_rfc3339(),
            updated_at: now.to_rfc3339(),
        };

        let entity = TagMapper::to_entity(&model).unwrap();

        assert_eq!(entity.id().as_str(), id.as_str());
        assert_eq!(entity.name().as_str(), "python");
        assert_eq!(entity.color(), "#6366f1");
    }

    #[test]
    fn test_roundtrip_conversion() {
        let name = TagName::new("machine-learning".to_string()).unwrap();
        let original = DomainTag::new(name, "#00ff00".to_string());

        // Entity -> Model -> Entity
        let model = TagMapper::to_model(&original);
        let converted = TagMapper::to_entity(&model).unwrap();

        assert_eq!(original.id().as_str(), converted.id().as_str());
        assert_eq!(original.name().as_str(), converted.name().as_str());
        assert_eq!(original.color(), converted.color());
    }

    #[test]
    fn test_batch_conversion() {
        let entities = vec![
            DomainTag::new(
                TagName::new("tag1".to_string()).unwrap(),
                "#ff0000".to_string(),
            ),
            DomainTag::new(
                TagName::new("tag2".to_string()).unwrap(),
                "#00ff00".to_string(),
            ),
        ];

        let models = TagMapper::to_models(&entities);
        assert_eq!(models.len(), 2);

        let converted = TagMapper::to_entities(&models);
        assert_eq!(converted.len(), 2);

        assert_eq!(converted[0].name().as_str(), "tag1");
        assert_eq!(converted[1].name().as_str(), "tag2");
    }

    #[test]
    fn test_invalid_timestamp() {
        let model = TagModel {
            id: TagId::new().as_str().to_string(),
            name: "test".to_string(),
            color: "#ff5733".to_string(),
            created_at: "invalid-timestamp".to_string(),
            updated_at: Utc::now().to_rfc3339(),
        };

        let result = TagMapper::to_entity(&model);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), AppError::InvalidData(_)));
    }

    #[test]
    fn test_invalid_tag_name() {
        let now = Utc::now();
        let model = TagModel {
            id: TagId::new().as_str().to_string(),
            name: "".to_string(), // Empty name is invalid
            color: "#ff5733".to_string(),
            created_at: now.to_rfc3339(),
            updated_at: now.to_rfc3339(),
        };

        let result = TagMapper::to_entity(&model);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), AppError::InvalidData(_)));
    }
}
