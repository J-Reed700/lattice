//! Maps between database models and domain entities for chunks.

use lattice_core::domain::entities::chunk::Chunk as DomainChunk;
use lattice_core::domain::entities::document::Language;
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::types::{ChunkId, DocumentId};
use serde::{Deserialize, Serialize};

/// Database model for chunks (SQLx persistence).
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct ChunkModel {
    pub id: String,
    pub document_id: String,
    pub content: String,
    pub chunk_index: i64,
    pub contextualized_content: Option<String>,
    pub context_prefix: Option<String>,
    pub start_char: Option<i64>,
    pub end_char: Option<i64>,
    pub language: String,
    pub token_count: i64,
    pub word_count: i64,
    pub has_code: i64,
    pub section: Option<String>,
    #[sqlx(default)]
    pub page_number: Option<i64>,
    #[sqlx(default)]
    pub from_ocr: bool,
}

/// Mapper for Chunk entity and database model.
pub struct ChunkMapper;

impl ChunkMapper {
    /// Convert domain entity to database model, preserving provenance and contextual text.
    pub fn to_model(entity: &DomainChunk) -> ChunkModel {
        ChunkModel {
            id: entity.id().as_str().to_string(),
            document_id: entity.document_id().as_str().to_string(),
            content: entity.content().to_string(),
            chunk_index: entity.index() as i64,
            contextualized_content: entity.context_prefix().map(|_| entity.embedding_text()),
            context_prefix: entity.context_prefix().map(str::to_owned),
            start_char: entity.start_char().map(|v| v as i64),
            end_char: entity.end_char().map(|v| v as i64),
            language: entity.language().to_string(),
            token_count: entity.token_count() as i64,
            word_count: entity.word_count() as i64,
            has_code: if entity.has_code() { 1 } else { 0 },
            section: entity.section().map(|s| s.to_string()),
            page_number: entity.page_number().map(i64::from),
            from_ocr: entity.from_ocr(),
        }
    }

    /// Convert database model to domain entity, restoring provenance and offsets.
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

        use lattice_core::domain::entities::chunk::ChunkParams;
        let mut chunk = DomainChunk::with_id(ChunkParams {
            id,
            document_id,
            content: model.content.clone(),
            index: model.chunk_index as usize,
            language,
            token_count: model.token_count as i32,
            word_count: model.word_count as usize,
            has_code: model.has_code != 0,
            section: model.section.clone(),
        });
        chunk.set_provenance(
            model.context_prefix.clone(),
            model.start_char.and_then(|n| n.try_into().ok()),
            model.end_char.and_then(|n| n.try_into().ok()),
            model.page_number.and_then(|n| n.try_into().ok()),
        );
        chunk.set_from_ocr(model.from_ocr);
        Ok(chunk)
    }

    /// Convert a batch of database models to domain entities, skipping invalid records.
    pub fn to_entities(models: &[ChunkModel]) -> Vec<DomainChunk> {
        models
            .iter()
            .filter_map(|model| Self::to_entity(model).ok())
            .collect()
    }

    /// Convert a batch of domain entities to database models.
    pub fn to_models(entities: &[DomainChunk]) -> Vec<ChunkModel> {
        entities.iter().map(Self::to_model).collect()
    }
}
