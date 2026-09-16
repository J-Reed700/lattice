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
//! Citation provenance and retrieval context survive both mapping directions.
//! Original content stays separate from embedding/lexical context.

use crate::domain::entities::chunk::Chunk as DomainChunk;
use crate::domain::entities::document::Language;
use crate::shared::domain_types::{ChunkId, DocumentId};
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
    // Extended metadata
    // SQLite types: TEXT, INTEGER (i64)
    pub language: String,
    pub token_count: i64, // SQLite INTEGER = i64
    pub word_count: i64,
    pub has_code: i64, // SQLite BOOLEAN = INTEGER (0/1)
    pub section: Option<String>,
    #[sqlx(default)]
    pub page_number: Option<i64>,
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
    /// Preserves contextual indexing text and exact source provenance.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use lattice::infrastructure::persistence::mappers::ChunkMapper;
    /// use lattice::domain::entities::chunk::Chunk;
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
            contextualized_content: entity.context_prefix().map(|_| entity.embedding_text()),
            context_prefix: entity.context_prefix().map(str::to_owned),
            start_char: entity.start_char().map(|v| v as i64),
            end_char: entity.end_char().map(|v| v as i64),
            // Rich metadata fields (convert to SQLite types)
            language: entity.language().to_string(),
            token_count: entity.token_count() as i64, // Convert i32 -> i64 for SQLite
            word_count: entity.word_count() as i64,
            has_code: if entity.has_code() { 1 } else { 0 },
            section: entity.section().map(|s| s.to_string()),
            page_number: entity.page_number().map(i64::from),
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
    /// Restores contextual prefix and source offsets/page without changing text.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use lattice::infrastructure::persistence::mappers::ChunkMapper;
    ///
    /// let db_model = /* from database */;
    /// let entity = ChunkMapper::to_entity(&db_model)?;
    /// ```
    pub fn to_entity(model: &ChunkModel) -> Result<DomainChunk> {
        let id = ChunkId::from_string(model.id.clone())
            .map_err(|e| AppError::InvalidData(format!("Invalid chunk ID: {}", e)))?;

        let document_id = DocumentId::from_string(model.document_id.clone())
            .map_err(|e| AppError::InvalidData(format!("Invalid document ID: {}", e)))?;

        if model.chunk_index < 0 {
            return Err(AppError::InvalidData(format!(
                "Chunk index cannot be negative: {}",
                model.chunk_index
            )));
        }

        let language = model
            .language
            .parse::<Language>()
            .unwrap_or(Language::Unknown);

        // Create domain entity (convert SQLite types back to domain types)
        // Note: Infrastructure-only fields are not passed to domain entity
        use crate::domain::entities::chunk::ChunkParams;
        let mut chunk = DomainChunk::with_id(ChunkParams {
            id,
            document_id,
            content: model.content.clone(),
            index: model.chunk_index as usize,
            language,
            token_count: model.token_count as i32, // Convert i64 -> i32 for domain
            word_count: model.word_count as usize,
            has_code: model.has_code != 0, // Convert SQLite INTEGER (0/1) to bool
            section: model.section.clone(),
        });
        chunk.set_provenance(
            model.context_prefix.clone(),
            model.start_char.and_then(|n| n.try_into().ok()),
            model.end_char.and_then(|n| n.try_into().ok()),
            model.page_number.and_then(|n| n.try_into().ok()),
        );
        Ok(chunk)
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
