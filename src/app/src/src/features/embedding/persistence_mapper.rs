//! Embedding Mapper - Domain Entity ↔ Infrastructure DTO
//!
//! Handles the dual-struct pattern where:
//! - Domain `Embedding`: Business metadata (chunk_id, model, dimension, timestamp)
//! - Infrastructure `EmbeddingDTO`: Persistence data including vector
//!
//! This separation keeps domain pure while allowing efficient vector storage.

use crate::features::embedding::entity::Embedding;
use crate::shared::domain_types::ChunkId;
use crate::shared::error::{AppError, Result};
use chrono::{DateTime, Utc};

/// Database representation with vector data
#[derive(Debug, Clone)]
pub struct EmbeddingDTO {
    pub chunk_id: String,
    pub embedding: Vec<f32>,
    pub model_name: String,
    pub dimension: usize,
    pub computed_at: DateTime<Utc>,
    pub model_version: Option<String>,
}

/// Mapper for bidirectional conversion
pub struct EmbeddingMapper;

impl EmbeddingMapper {
    /// Convert domain entity + vector to DTO for storage
    ///
    /// # Arguments
    /// * `entity` - Domain embedding (metadata)
    /// * `vector` - Actual embedding vector
    ///
    /// # Example
    /// ```rust
    /// let entity = Embedding::new(chunk_id, "model".into(), 384);
    /// let dto = EmbeddingMapper::to_dto(&entity, vector);
    /// ```
    pub fn to_dto(entity: &Embedding, vector: Vec<f32>) -> EmbeddingDTO {
        EmbeddingDTO {
            chunk_id: entity.chunk_id().to_string(),
            embedding: vector,
            model_name: entity.model_name().to_string(),
            dimension: entity.dimension(),
            computed_at: entity.computed_at(),
            model_version: entity.model_version().map(String::from),
        }
    }

    /// Convert DTO from database to domain entity
    ///
    /// Returns entity (metadata) separately from vector.
    /// Caller must extract vector from DTO separately.
    ///
    /// # Example
    /// ```rust
    /// let (entity, vector) = EmbeddingMapper::from_dto(dto)?;
    /// ```
    pub fn from_dto(dto: &EmbeddingDTO) -> Result<Embedding> {
        let chunk_id = ChunkId::from_string(dto.chunk_id.clone())?;

        let entity = Embedding::with_timestamp(
            chunk_id,
            dto.model_name.clone(),
            dto.dimension,
            dto.computed_at,
        );

        Ok(entity)
    }

    /// Validate vector dimension matches entity
    pub fn validate_dimension(entity: &Embedding, vector: &[f32]) -> Result<()> {
        if vector.len() != entity.dimension() {
            return Err(AppError::InvalidInput(format!(
                "Vector dimension {} does not match entity dimension {}",
                vector.len(),
                entity.dimension()
            )));
        }
        Ok(())
    }

    /// Extract vector from DTO
    pub fn extract_vector(dto: &EmbeddingDTO) -> Vec<f32> {
        dto.embedding.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::embedding::entity::Embedding;
    use crate::shared::domain_types::ChunkId;

    #[test]
    fn test_round_trip_conversion() {
        let chunk_id = ChunkId::new();
        let entity = Embedding::new(chunk_id.clone(), "test-model".to_string(), 384);
        let vector = vec![0.1; 384];

        let dto = EmbeddingMapper::to_dto(&entity, vector.clone());
        let converted = EmbeddingMapper::from_dto(&dto).unwrap();

        assert_eq!(converted.chunk_id(), entity.chunk_id());
        assert_eq!(converted.model_name(), entity.model_name());
        assert_eq!(converted.dimension(), entity.dimension());
        assert_eq!(dto.embedding, vector);
    }

    #[test]
    fn test_validate_dimension_success() {
        let chunk_id = ChunkId::new();
        let entity = Embedding::new(chunk_id, "model".to_string(), 384);
        let vector = vec![0.0; 384];

        assert!(EmbeddingMapper::validate_dimension(&entity, &vector).is_ok());
    }

    #[test]
    fn test_validate_dimension_failure() {
        let chunk_id = ChunkId::new();
        let entity = Embedding::new(chunk_id, "model".to_string(), 384);
        let vector = vec![0.0; 512]; // Wrong dimension

        assert!(EmbeddingMapper::validate_dimension(&entity, &vector).is_err());
    }
}
