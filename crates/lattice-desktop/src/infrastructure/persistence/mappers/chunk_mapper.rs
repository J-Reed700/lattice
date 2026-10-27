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

use lattice_core::domain::entities::chunk::Chunk as DomainChunk;
use lattice_core::domain::entities::document::Language;
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::types::{ChunkId, DocumentId};
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
    #[sqlx(default)]
    pub from_ocr: bool,
}

/// Mapper for Chunk entity and database model.
///
/// Provides bidirectional conversion between domain entities and database models.
pub struct ChunkMapper;

impl ChunkMapper {
    /// Convert domain entity to database model.
    ///
    /// # Note
    ///
    /// Preserves contextual indexing text and exact source provenance.
    pub fn to_model(entity: &DomainChunk) -> Result<ChunkModel> {
        let to_sqlite_integer = |value: usize, field: &str| {
            i64::try_from(value).map_err(|_| {
                AppError::InvalidData(format!("Chunk {field} exceeds SQLite INTEGER range"))
            })
        };
        Ok(ChunkModel {
            id: entity.id().as_str().to_string(),
            document_id: entity.document_id().as_str().to_string(),
            content: entity.content().to_string(),
            chunk_index: to_sqlite_integer(entity.index(), "chunk_index")?,
            // Infrastructure-only fields - not in domain entity
            contextualized_content: entity.context_prefix().map(|_| entity.embedding_text()),
            context_prefix: entity.context_prefix().map(str::to_owned),
            start_char: entity
                .start_char()
                .map(|v| to_sqlite_integer(v, "start_char"))
                .transpose()?,
            end_char: entity
                .end_char()
                .map(|v| to_sqlite_integer(v, "end_char"))
                .transpose()?,
            // Rich metadata fields (convert to SQLite types)
            language: entity.language().to_string(),
            token_count: entity.token_count() as i64, // Convert i32 -> i64 for SQLite
            word_count: to_sqlite_integer(entity.word_count(), "word_count")?,
            has_code: if entity.has_code() { 1 } else { 0 },
            section: entity.section().map(|s| s.to_string()),
            page_number: entity.page_number().map(i64::from),
            from_ocr: entity.from_ocr(),
        })
    }

    /// Convert database model to domain entity.
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
    pub fn to_entity(model: &ChunkModel) -> Result<DomainChunk> {
        let id = ChunkId::from_string(model.id.clone())
            .map_err(|e| AppError::InvalidData(format!("Invalid chunk ID: {}", e)))?;

        let document_id = DocumentId::from_string(model.document_id.clone())
            .map_err(|e| AppError::InvalidData(format!("Invalid document ID: {}", e)))?;

        let chunk_index = usize::try_from(model.chunk_index).map_err(|_| {
            AppError::InvalidData(format!(
                "Chunk index is out of range: {}",
                model.chunk_index
            ))
        })?;
        let token_count = i32::try_from(model.token_count).map_err(|_| {
            AppError::InvalidData(format!(
                "Chunk token count is out of range: {}",
                model.token_count
            ))
        })?;
        if token_count < 0 {
            return Err(AppError::InvalidData(format!(
                "Chunk token count cannot be negative: {token_count}"
            )));
        }
        let word_count = usize::try_from(model.word_count).map_err(|_| {
            AppError::InvalidData(format!(
                "Chunk word count is out of range: {}",
                model.word_count
            ))
        })?;
        let has_code = match model.has_code {
            0 => false,
            1 => true,
            value => {
                return Err(AppError::InvalidData(format!(
                    "Chunk has_code is not boolean: {value}"
                )))
            }
        };
        let start_char = model
            .start_char
            .map(usize::try_from)
            .transpose()
            .map_err(|_| AppError::InvalidData("Chunk start_char is negative".into()))?;
        let end_char = model
            .end_char
            .map(usize::try_from)
            .transpose()
            .map_err(|_| AppError::InvalidData("Chunk end_char is negative".into()))?;
        let page_number = model
            .page_number
            .map(u32::try_from)
            .transpose()
            .map_err(|_| AppError::InvalidData("Chunk page_number is out of range".into()))?;

        let language = model
            .language
            .parse::<Language>()
            .unwrap_or(Language::Unknown);

        // Create domain entity (convert SQLite types back to domain types)
        // Note: Infrastructure-only fields are not passed to domain entity
        use lattice_core::domain::entities::chunk::ChunkParams;
        let mut chunk = DomainChunk::with_id(ChunkParams {
            id,
            document_id,
            content: model.content.clone(),
            index: chunk_index,
            language,
            token_count,
            word_count,
            has_code,
            section: model.section.clone(),
        });
        chunk.set_provenance(
            model.context_prefix.clone(),
            start_char,
            end_char,
            page_number,
        );
        chunk.set_from_ocr(model.from_ocr);
        Ok(chunk)
    }

    /// Convert all models or return the first invalid row. Repository reads
    /// must not silently omit persisted chunks.
    pub fn to_entities(models: &[ChunkModel]) -> Result<Vec<DomainChunk>> {
        models.iter().map(Self::to_entity).collect()
    }

    /// Convert a batch of domain entities to database models.
    pub fn to_models(entities: &[DomainChunk]) -> Result<Vec<ChunkModel>> {
        entities.iter().map(Self::to_model).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_model() -> ChunkModel {
        let chunk = DomainChunk::new(DocumentId::new(), "mapper fixture".to_owned(), 0);
        ChunkMapper::to_model(&chunk).unwrap()
    }

    #[test]
    fn rejects_out_of_range_database_numbers_instead_of_wrapping() {
        let mut model = valid_model();
        model.token_count = i64::from(i32::MAX) + 1;
        assert!(matches!(
            ChunkMapper::to_entity(&model),
            Err(AppError::InvalidData(_))
        ));

        let mut model = valid_model();
        model.token_count = -1;
        assert!(matches!(
            ChunkMapper::to_entity(&model),
            Err(AppError::InvalidData(_))
        ));

        let mut model = valid_model();
        model.word_count = -1;
        assert!(matches!(
            ChunkMapper::to_entity(&model),
            Err(AppError::InvalidData(_))
        ));

        let mut model = valid_model();
        model.start_char = Some(-1);
        assert!(matches!(
            ChunkMapper::to_entity(&model),
            Err(AppError::InvalidData(_))
        ));

        let mut model = valid_model();
        model.page_number = Some(i64::from(u32::MAX) + 1);
        assert!(matches!(
            ChunkMapper::to_entity(&model),
            Err(AppError::InvalidData(_))
        ));

        let mut model = valid_model();
        model.has_code = 2;
        assert!(matches!(
            ChunkMapper::to_entity(&model),
            Err(AppError::InvalidData(_))
        ));
    }

    #[test]
    fn batch_hydration_fails_instead_of_dropping_invalid_rows() {
        let good = valid_model();
        let mut bad = valid_model();
        bad.chunk_index = -1;
        assert!(matches!(
            ChunkMapper::to_entities(&[good, bad]),
            Err(AppError::InvalidData(_))
        ));
    }

    #[test]
    #[cfg(target_pointer_width = "64")]
    fn model_conversion_rejects_usize_values_that_sqlite_cannot_store() {
        let chunk = DomainChunk::new(DocumentId::new(), "oversized".into(), usize::MAX);
        assert!(matches!(
            ChunkMapper::to_model(&chunk),
            Err(AppError::InvalidData(_))
        ));

        let mut chunk = DomainChunk::new(DocumentId::new(), "oversized".into(), 0);
        chunk.set_provenance(None, Some(usize::MAX), None, None);
        assert!(matches!(
            ChunkMapper::to_model(&chunk),
            Err(AppError::InvalidData(_))
        ));
    }
}
