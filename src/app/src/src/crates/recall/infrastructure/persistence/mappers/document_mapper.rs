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

use crate::domain::entities::document::{
    Category, Document as DomainDocument, DocumentStatus, Language,
};
use crate::domain_types::{DocumentId, ValidatedFilePath};
use crate::shared::error::{AppError, Result};
use chrono::{DateTime, Utc};
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
    #[sqlx(default)]
    pub error_message: Option<String>,
    // Rich metadata fields (Phase 2)
    // SQLite types: TEXT, REAL (f64), INTEGER (i64)
    pub language: String,
    pub category: String,
    pub quality_score: f64, // SQLite REAL = f64
    pub access_count: i64,  // SQLite INTEGER = i64
    pub last_accessed_at: Option<String>,
    pub word_count: i64, // SQLite INTEGER = i64
    #[sqlx(default)]
    pub content: String, // Document content (Oracle Step 1)
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
    /// use vault_desktop::infrastructure::persistence::mappers::DocumentMapper;
    /// use vault_desktop::domain::entities::Document;
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
            content: entity.content().to_string(),  // Oracle Step 1: Content field
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
    /// use vault_desktop::infrastructure::persistence::mappers::DocumentMapper;
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

        // Helper function to parse timestamps with nanosecond precision handling
        // RFC3339 officially supports up to 6 decimal places (microseconds), but
        // some file systems (like ext4) use nanosecond precision (9 decimals).
        // Chrono's parse_from_rfc3339 rejects >6 decimals, so we truncate.
        let parse_timestamp = |ts: &str| -> Result<DateTime<Utc>> {
            // First try parsing as-is
            match DateTime::parse_from_rfc3339(ts) {
                Ok(dt) => Ok(dt.with_timezone(&Utc)),
                Err(_) => {
                    // If parsing fails, check for nanosecond precision and truncate
                    // Format: "2025-12-07T08:16:30.126611234+00:00"
                    // We need to find the fractional seconds and truncate to 6 digits
                    if let Some(dot_idx) = ts.find('.') {
                        if let Some(tz_idx) = ts[dot_idx..].find(&['+', '-'][..]) {
                            let tz_start = dot_idx + tz_idx;
                            let fraction = &ts[dot_idx + 1..tz_start];

                            // If fraction has >6 digits, truncate to 6
                            if fraction.len() > 6 {
                                let truncated = format!(
                                    "{}.{}{}",
                                    &ts[..dot_idx],
                                    &fraction[..6],
                                    &ts[tz_start..]
                                );
                                tracing::debug!(
                                    "Truncated nanosecond timestamp: {} -> {}",
                                    ts,
                                    truncated
                                );
                                return DateTime::parse_from_rfc3339(&truncated)
                                    .map(|dt| dt.with_timezone(&Utc))
                                    .map_err(|e| {
                                        AppError::InvalidData(format!(
                                            "Invalid timestamp even after truncation: {} -> {}",
                                            ts, e
                                        ))
                                    });
                            }
                        }
                    }

                    // If we couldn't fix it, return the original error
                    Err(AppError::InvalidData(format!(
                        "Invalid RFC3339 timestamp: {}",
                        ts
                    )))
                }
            }
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

        // Parse document ID
        let id = DocumentId::from_string(model.id.clone()).map_err(|e| {
            let err_msg = format!("Invalid document ID: {} (value: {})", e, model.id);
            tracing::error!("{}", err_msg);
            AppError::InvalidData(err_msg)
        })?;

        // Parse file path
        let file_path = ValidatedFilePath::new(PathBuf::from(&model.file_path)).map_err(|e| {
            let err_msg = format!(
                "Invalid file path for doc {}: {} (value: {})",
                model.id, e, model.file_path
            );
            tracing::error!("{}", err_msg);
            AppError::InvalidData(err_msg)
        })?;

        // Parse status
        let status = model.status.parse::<DocumentStatus>().map_err(|e| {
            let err_msg = format!(
                "Invalid status for doc {}: {} (value: {})",
                model.id, e, model.status
            );
            tracing::error!("{}", err_msg);
            AppError::InvalidData(err_msg)
        })?;

        // Parse language
        let language = model
            .language
            .parse::<Language>()
            .unwrap_or(Language::Unknown);

        // Parse category
        let category = model
            .category
            .parse::<Category>()
            .unwrap_or(Category::Uncategorized);

        // Parse last_accessed_at
        let last_accessed_at = model
            .last_accessed_at
            .as_ref()
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&Utc));

        // Convert checksum String to Checksum value object
        let checksum =
            crate::domain::value_objects::checksum::Checksum::new(model.checksum.clone())?;

        // Create domain entity (convert SQLite types back to domain types)
        let entity = DomainDocument::with_id(
            id,
            file_path,
            model.file_name.clone(),
            model.file_type.clone(),
            model.mime_type.clone(),
            model.size_bytes,
            modified_at,
            indexed_at,
            checksum,
            status,
            model.error_message.clone(),
            language,
            category,
            model.quality_score as f32, // Convert f64 -> f32 for domain
            model.access_count as i32,  // Convert i64 -> i32 for domain
            last_accessed_at,
            model.word_count as i32, // Convert i64 -> i32 for domain
            model.content.clone(),   // Oracle Step 1: Content field
        );

        tracing::debug!(
            "Successfully parsed document: id={}, file_name={}",
            model.id,
            model.file_name
        );

        Ok(entity)
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
    ///
    /// # Warning
    ///
    /// **This method silently drops documents that fail to parse!**
    /// Parse errors are logged at ERROR level. Check logs to diagnose missing documents.
    pub fn to_entities(models: &[DocumentModel]) -> Vec<DomainDocument> {
        tracing::info!(
            "Converting batch of {} database models to domain entities",
            models.len()
        );

        let mut success_count = 0;
        let mut failure_count = 0;

        let entities: Vec<DomainDocument> = models
            .iter()
            .filter_map(|model| {
                match Self::to_entity(model) {
                    Ok(entity) => {
                        success_count += 1;
                        Some(entity)
                    }
                    Err(e) => {
                        failure_count += 1;
                        tracing::error!(
                            "❌ DOCUMENT PARSE FAILURE #{}: Failed to parse document id={}, file_path={}: {}",
                            failure_count,
                            model.id,
                            model.file_path,
                            e
                        );
                        None
                    }
                }
            })
            .collect();

        if failure_count > 0 {
            tracing::error!(
                "⚠️  SILENT FAILURE WARNING: {}/{} documents were DROPPED due to parse errors. Check error logs above.",
                failure_count,
                models.len()
            );
        }

        tracing::info!(
            "Batch conversion complete: {} succeeded, {} failed, {} returned",
            success_count,
            failure_count,
            entities.len()
        );

        entities
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

// ============================================================================
// Tests
// ============================================================================
