//! # Document Mapper
//!
//! Maps between database models and domain entities for documents.
//!
//! This mapper implements the transformation layer between the anemic
//! database model (used by SQLx) and the rich domain entity.
//!
//! ## Architecture
//!
//! - **Domain Entity** (`domain::entities::Document`) - Rich model with business logic
//! - **DB Model** (defined in repository) - Anemic struct for database persistence
//! - **Mapper** - Converts between the two

use chrono::{DateTime, Utc};
use lattice_core::domain::entities::document::{
    Category, Document as DomainDocument, DocumentStatus, Language, StoredDocument,
};
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::types::{DocumentId, ValidatedFilePath};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Database model for documents.
///
/// This is an anemic data structure that mirrors the database schema.
/// It contains no business logic - only data transfer.
///
/// **Important**: This type should NEVER leak outside the infrastructure layer.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct DocumentModel {
    pub id: String,
    pub file_path: String,
    pub file_name: String,
    pub file_type: Option<String>,
    pub mime_type: String,
    pub size_bytes: i64,
    pub modified_at: String,
    pub indexed_at: String,
    pub checksum: String,
    pub status: String,
    pub error_message: Option<String>,
    // Extended metadata
    // SQLite types: TEXT, REAL (f64), INTEGER (i64)
    pub language: String,
    pub category: String,
    pub quality_score: f64, // SQLite REAL = f64
    pub access_count: i64,  // SQLite INTEGER = i64
    pub last_accessed_at: Option<String>,
    pub word_count: i64, // SQLite INTEGER = i64
    #[sqlx(default)]
    pub content: String,
    #[sqlx(default)]
    pub source_context: Option<String>,
}

/// Mapper for Document entity and database model.
///
/// Provides bidirectional conversion between domain entities and database models.
pub struct DocumentMapper;

impl DocumentMapper {
    /// Convert domain entity to database model.
    ///
    /// # Arguments
    ///
    /// * `entity` - Domain document entity
    ///
    /// # Returns
    ///
    /// Database model ready for persistence.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use lattice::infrastructure::persistence::mappers::DocumentMapper;
    /// use lattice_core::domain::entities::Document;
    ///
    /// let entity = Document::new(/* ... */);
    /// let db_model = DocumentMapper::to_model(&entity);
    /// ```
    pub fn to_model(entity: &DomainDocument) -> DocumentModel {
        DocumentModel {
            id: entity.id().as_str().to_string(),
            file_path: entity.file_path().to_string_lossy().to_string(),
            file_name: entity.file_name().to_string(),
            file_type: entity.file_type().map(|s| s.to_string()),
            mime_type: entity.mime_type().to_string(),
            size_bytes: entity.size_bytes(),
            modified_at: entity.modified_at().to_rfc3339(),
            indexed_at: entity.indexed_at().to_rfc3339(),
            checksum: entity.checksum().to_string(),
            status: entity.status().to_string(),
            error_message: entity.error_message().map(|s| s.to_string()),
            language: entity.language().to_string(),
            category: entity.category().to_string(),
            quality_score: entity.quality_score() as f64, // Convert f32 -> f64 for SQLite
            access_count: entity.access_count() as i64,   // Convert i32 -> i64 for SQLite
            last_accessed_at: entity.last_accessed_at().map(|dt| dt.to_rfc3339()),
            word_count: entity.word_count() as i64, // Convert i32 -> i64 for SQLite
            source_context: entity
                .source_context()
                .map(|value| serde_json::json!(value).to_string()),
            content: entity.content().to_string(),
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
    /// - `AppError::InvalidData` if file path is invalid
    /// - `AppError::InvalidData` if status is invalid
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use lattice::infrastructure::persistence::mappers::DocumentMapper;
    ///
    /// let db_model = /* from database */;
    /// let entity = DocumentMapper::to_entity(&db_model)?;
    /// ```
    pub fn to_entity(model: &DocumentModel) -> Result<DomainDocument> {
        // Log start of parsing for diagnostics
        tracing::debug!(
            "Parsing document model: id={}, file_path={}, status={}",
            model.id,
            model.file_path,
            model.status
        );

        // Chrono accepts RFC3339 fractional seconds at nanosecond precision.
        let parse_timestamp = |ts: &str| -> Result<DateTime<Utc>> {
            DateTime::parse_from_rfc3339(ts)
                .map(|dt| dt.with_timezone(&Utc))
                .map_err(|e| AppError::InvalidData(format!("Invalid RFC3339 timestamp: {ts}: {e}")))
        };

        // Parse timestamps with nanosecond precision handling
        let modified_at = parse_timestamp(&model.modified_at).map_err(|e| {
            let err_msg = format!(
                "Invalid modified_at timestamp for doc {}: {} (value: {})",
                model.id, e, model.modified_at
            );
            tracing::error!("{}", err_msg);
            e
        })?;

        let indexed_at = parse_timestamp(&model.indexed_at).map_err(|e| {
            let err_msg = format!(
                "Invalid indexed_at timestamp for doc {}: {} (value: {})",
                model.id, e, model.indexed_at
            );
            tracing::error!("{}", err_msg);
            e
        })?;

        let id = DocumentId::from_string(model.id.clone()).map_err(|e| {
            let err_msg = format!("Invalid document ID: {} (value: {})", e, model.id);
            tracing::error!("{}", err_msg);
            AppError::InvalidData(err_msg)
        })?;

        let file_path = ValidatedFilePath::new(PathBuf::from(&model.file_path)).map_err(|e| {
            let err_msg = format!(
                "Invalid file path for doc {}: {} (value: {})",
                model.id, e, model.file_path
            );
            tracing::error!("{}", err_msg);
            AppError::InvalidData(err_msg)
        })?;

        let status = model.status.parse::<DocumentStatus>().map_err(|e| {
            let err_msg = format!(
                "Invalid status for doc {}: {} (value: {})",
                model.id, e, model.status
            );
            tracing::error!("{}", err_msg);
            AppError::InvalidData(err_msg)
        })?;

        let language = model
            .language
            .parse::<Language>()
            .unwrap_or(Language::Unknown);

        let category = model
            .category
            .parse::<Category>()
            .unwrap_or(Category::Uncategorized);

        let last_accessed_at = model
            .last_accessed_at
            .as_deref()
            .map(parse_timestamp)
            .transpose()?;

        let access_count = i32::try_from(model.access_count)
            .ok()
            .filter(|count| *count >= 0)
            .ok_or_else(|| {
                AppError::InvalidData(format!(
                    "Document {} access_count is outside the supported nonnegative range: {}",
                    model.id, model.access_count
                ))
            })?;
        let word_count = i32::try_from(model.word_count)
            .ok()
            .filter(|count| *count >= 0)
            .ok_or_else(|| {
                AppError::InvalidData(format!(
                    "Document {} word_count is outside the supported nonnegative range: {}",
                    model.id, model.word_count
                ))
            })?;

        let checksum =
            lattice_core::domain::value_objects::checksum::Checksum::new(model.checksum.clone())?;

        let mut entity = DomainDocument::with_id(StoredDocument {
            id,
            file_path,
            file_name: model.file_name.clone(),
            file_type: model.file_type.clone(),
            mime_type: model.mime_type.clone(),
            size_bytes: model.size_bytes,
            modified_at,
            indexed_at,
            checksum,
            status,
            error_message: model.error_message.clone(),
            language,
            category,
            quality_score: model.quality_score as f32, // Convert f64 -> f32 for domain
            access_count,
            last_accessed_at,
            word_count,
            content: model.content.clone(),
        });

        tracing::debug!(
            "Successfully parsed document: id={}, file_name={}",
            model.id,
            model.file_name
        );

        entity.set_source_context(
            model
                .source_context
                .as_deref()
                .map(serde_json::from_str)
                .transpose()?,
        );
        Ok(entity)
    }

    /// Convert a batch of database models to domain entities.
    ///
    /// Returns an error if any row cannot be represented in the domain model.
    ///
    /// # Arguments
    ///
    /// * `models` - Vector of database models
    ///
    /// # Returns
    ///
    /// All converted entities, or the first conversion error.
    pub fn to_entities(models: &[DocumentModel]) -> Result<Vec<DomainDocument>> {
        models.iter().map(Self::to_entity).collect()
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
    pub fn to_models(entities: &[DomainDocument]) -> Vec<DocumentModel> {
        entities.iter().map(Self::to_model).collect()
    }
}
