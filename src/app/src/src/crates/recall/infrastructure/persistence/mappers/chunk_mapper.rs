//! # Chunk Mapper
//!
//! Maps between database models and domain entities for chunks.
//!
//! This mapper implements the transformation layer between the anemic
//! database model (used by SQLx) and the rich domain entity.
//!
//! ## Architecture
//!
//! - **Domain Entity** (`domain::entities::Chunk`) - Rich model with business logic
//! - **DB Model** (defined here) - Anemic struct for database persistence
//! - **Mapper** - Converts between the two
//!
//! ## Field Mapping
//!
//! The database model contains additional fields not present in the domain entity:
//! - `contextualized_content` - Used for retrieval augmentation (infrastructure concern)
//! - `context_prefix` - Context for the chunk (infrastructure concern)
//! - `start_char`, `end_char` - Character positions (infrastructure concern)
//!
//! These are infrastructure-level details and are not exposed to the domain layer.

use crate::domain::entities::chunk::Chunk as DomainChunk;
use crate::domain::entities::document::Language;
use crate::domain_types::{ChunkId, DocumentId};
use crate::shared::error::{AppError, Result};
use serde::{Deserialize, Serialize};

/// Database model for chunks.
///
/// This is an anemic data structure that mirrors the database schema.
/// It contains no business logic - only data transfer.
///
/// **Important**: This type should NEVER leak outside the infrastructure layer.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct ChunkModel {
    pub id: String,
    pub document_id: String,
    pub content: String,
    pub chunk_index: i64,
    // Infrastructure-only fields (not in domain entity)
    pub contextualized_content: Option<String>,
    pub context_prefix: Option<String>,
    pub start_char: Option<i64>,
    pub end_char: Option<i64>,
    // Rich metadata fields (Phase 2)
    // SQLite types: TEXT, INTEGER (i64)
    pub language: String,
    pub token_count: i64, // SQLite INTEGER = i64
    // Phase 1 metadata
    pub word_count: i64,
    pub has_code: i64, // SQLite BOOLEAN = INTEGER (0/1)
    pub section: Option<String>,
}

/// Mapper for Chunk entity and database model.
///
/// Provides bidirectional conversion between domain entities and database models.
pub struct ChunkMapper;

impl ChunkMapper {
    /// Convert domain entity to database model.
    ///
    /// # Arguments
    ///
    /// * `entity` - Domain chunk entity
    ///
    /// # Returns
    ///
    /// Database model ready for persistence.
    ///
    /// # Note
    ///
    /// Infrastructure-only fields (contextualized_content, context_prefix, etc.)
    /// are set to None/default values. These should be populated separately
    /// by infrastructure services if needed.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use vault_desktop::infrastructure::persistence::mappers::ChunkMapper;
    /// use vault_desktop::domain::entities::chunk::Chunk;
    ///
    /// let entity = Chunk::new(/* ... */);
    /// let db_model = ChunkMapper::to_model(&entity);
    /// ```
    pub fn to_model(entity: &DomainChunk) -> ChunkModel {
        ChunkModel {
            id: entity.id().as_str().to_string(),
            document_id: entity.document_id().as_str().to_string(),
            content: entity.content().to_string(),
            chunk_index: entity.index() as i64,
            // Infrastructure-only fields - not in domain entity
            contextualized_content: None,
            context_prefix: None,
            start_char: None,
            end_char: None,
            // Rich metadata fields (convert to SQLite types)
            language: entity.language().to_string(),
            token_count: entity.token_count() as i64, // Convert i32 -> i64 for SQLite
            // Phase 1 metadata
            word_count: entity.word_count() as i64,
            has_code: if entity.has_code() { 1 } else { 0 },
            section: entity.section().map(|s| s.to_string()),
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
    /// - `AppError::InvalidData` if chunk ID is invalid
    /// - `AppError::InvalidData` if document ID is invalid
    /// - `AppError::InvalidData` if chunk_index is negative
    ///
    /// # Note
    ///
    /// Infrastructure-only fields (contextualized_content, etc.) are discarded
    /// during conversion. The domain entity only contains core business data.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use vault_desktop::infrastructure::persistence::mappers::ChunkMapper;
    ///
    /// let db_model = /* from database */;
    /// let entity = ChunkMapper::to_entity(&db_model)?;
    /// ```
    pub fn to_entity(model: &ChunkModel) -> Result<DomainChunk> {
        // Parse chunk ID
        let id = ChunkId::from_string(model.id.clone())
            .map_err(|e| AppError::InvalidData(format!("Invalid chunk ID: {}", e)))?;

        // Parse document ID
        let document_id = DocumentId::from_string(model.document_id.clone())
            .map_err(|e| AppError::InvalidData(format!("Invalid document ID: {}", e)))?;

        // Validate chunk_index is non-negative
        if model.chunk_index < 0 {
            return Err(AppError::InvalidData(format!(
                "Chunk index cannot be negative: {}",
                model.chunk_index
            )));
        }

        // Parse language
        let language = model
            .language
            .parse::<Language>()
            .unwrap_or(Language::Unknown);

        // Create domain entity (convert SQLite types back to domain types)
        // Note: Infrastructure-only fields are not passed to domain entity
        use crate::domain::entities::chunk::ChunkParams;
        Ok(DomainChunk::with_id(ChunkParams {
            id,
            document_id,
            content: model.content.clone(),
            index: model.chunk_index as usize,
            language,
            token_count: model.token_count as i32, // Convert i64 -> i32 for domain
            word_count: model.word_count as usize,
            has_code: model.has_code != 0, // Convert SQLite INTEGER (0/1) to bool
            section: model.section.clone(),
        }))
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
    pub fn to_entities(models: &[ChunkModel]) -> Vec<DomainChunk> {
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
    pub fn to_models(entities: &[DomainChunk]) -> Vec<ChunkModel> {
        entities.iter().map(Self::to_model).collect()
    }
}

// ============================================================================
// Tests
// ============================================================================
