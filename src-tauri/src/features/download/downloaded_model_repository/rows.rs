//! The `models` row shape, its SELECT, and conversion into `DownloadedModel`.

use std::path::{Path, PathBuf};

use chrono::{DateTime, NaiveDateTime, Utc};
use tracing::warn;

use crate::domain::models::checkpoint::{WEIGHTS_PYTORCH_BIN, WEIGHTS_SAFETENSORS};
use crate::domain::models::downloaded::{DownloadedModel, ModelLocation, StoredDownloadedModel};
use crate::domain::models::metadata::ModelType;
use crate::domain::value_objects::ArtifactIdentity;
use crate::shared::error::{AppError, Result};

/// Database record structure for RETURNING queries.
///
/// Mirrors the row produced by `MODEL_SELECT_BASE`. The `(storage_kind,
/// storage_path)` pair is the canonical representation of `ModelLocation`
/// in the DB; both are read verbatim with no JOIN-time recomputation.
#[derive(Debug, sqlx::FromRow)]
pub(super) struct DownloadedModelRecord {
    id: String,
    model_name: String,
    pub(super) model_id: String,
    storage_kind: String,
    storage_path: Option<String>,
    file_size_bytes: i64,
    model_type: String,
    architecture: String,
    downloaded_at: Option<String>,
    last_used_at: Option<String>,
    use_count: i64,
    is_active_for_chat: i64,
    is_active_for_embedding: i64,
    metadata: Option<String>,
    is_active_for_utility: i64,
    pub(super) embedding_artifact_identity: Option<String>,
}

/// Read columns directly from `models`. The previous version of this
/// query had a CTE that re-derived the "primary file" by alphabetical
/// order over `model_files` and `COALESCE`-overrode the saga-written
/// path, which silently corrupted multi-file safetensors models. The
/// SSOT for the loadable artifact is `models.storage_kind` +
/// `models.storage_path`, written by the saga and never recomputed.
const MODEL_SELECT_BASE: &str = r#"
SELECT
    m.id,
    m.model_name,
    m.model_id,
    m.storage_kind,
    m.storage_path,
    COALESCE(m.total_size_bytes, 0) AS file_size_bytes,
    m.model_type,
    m.architecture,
    m.downloaded_at,
    m.last_used_at,
    m.use_count,
    m.is_active_for_chat,
    m.is_active_for_embedding,
    m.metadata,
    m.is_active_for_utility,
    m.embedding_artifact_identity
FROM models m
"#;

/// Map domain model type strings to the models-table CHECK constraint values.
///
/// The domain uses modern values (`language_model`, `text_embeddings`), while the
/// legacy `models` table constraint still expects (`chat`, `embedding`, ...).
pub(super) fn normalize_model_type_for_models_table(model_type: ModelType) -> &'static str {
    match model_type {
        ModelType::LanguageModel => "chat",
        ModelType::TextEmbeddings => "embedding",
        ModelType::Vision => "multi_modal",
        ModelType::Reranker => "qa",
        ModelType::Transcription => "custom",
    }
}

/// Weights the Candle runtime can load: any safetensors shard, or the
/// `pytorch_model.bin` pickle for checkpoints like BAAI/bge-m3 that never
/// published a safetensors conversion.
pub(super) fn directory_has_loadable_weights(path: &Path) -> bool {
    path.is_dir()
        && std::fs::read_dir(path)
            .ok()
            .into_iter()
            .flatten()
            .filter_map(std::result::Result::ok)
            .map(|entry| entry.path())
            .any(|entry| {
                entry.is_file()
                    && (entry.extension().and_then(|value| value.to_str()) == Some("safetensors")
                        || entry.file_name().and_then(|value| value.to_str())
                            == Some(WEIGHTS_PYTORCH_BIN))
            })
}

pub(super) fn location_is_available(location: &ModelLocation) -> bool {
    match location {
        ModelLocation::LocalFile { path } => path.is_file(),
        ModelLocation::LocalDirectory { path } => directory_has_loadable_weights(path),
        ModelLocation::RemoteOllama => true,
    }
}

pub(super) fn fallback_artifact_path(location: &ModelLocation) -> Option<PathBuf> {
    match location {
        ModelLocation::LocalFile { path } => Some(path.clone()),
        // Name the weights file the directory actually holds. A checkpoint
        // that ships only `pytorch_model.bin` would otherwise be recorded
        // under a `model.safetensors` path that does not exist, and the
        // synthesized manifest row would report a zero size.
        ModelLocation::LocalDirectory { path } => Some(
            crate::domain::models::checkpoint::weights_path(path)
                .unwrap_or_else(|| path.join(WEIGHTS_SAFETENSORS)),
        ),
        ModelLocation::RemoteOllama => None,
    }
}

/// Convert database record to domain entity
impl TryFrom<DownloadedModelRecord> for DownloadedModel {
    type Error = AppError;

    fn try_from(record: DownloadedModelRecord) -> Result<Self, Self::Error> {
        let model_type_enum = ModelType::from_db_string(&record.model_type)?;

        let downloaded_at_dt =
            parse_optional_db_timestamp(record.downloaded_at.as_deref())?.unwrap_or_else(Utc::now);
        let last_used_at_dt = parse_optional_db_timestamp(record.last_used_at.as_deref())?;

        let metadata_val = match DownloadedModel::metadata_from_json(record.metadata.as_deref()) {
            Ok(metadata) => metadata,
            Err(e) => {
                warn!(
                    model_id = %record.model_id,
                    error = %e,
                    "Ignoring invalid model metadata JSON"
                );
                None
            }
        };

        let is_active_chat = record.is_active_for_chat != 0;
        let is_active_embedding = record.is_active_for_embedding != 0;
        let is_active_utility = record.is_active_for_utility != 0;

        let location =
            ModelLocation::from_db(&record.storage_kind, record.storage_path).map_err(|e| {
                AppError::InvalidData(format!(
                    "Invalid storage_kind/storage_path for model_id '{}': {}",
                    record.model_id, e
                ))
            })?;

        // Only activation writes this column, so a malformed value is
        // corruption, not something to read past.
        let embedding_artifact_identity = record
            .embedding_artifact_identity
            .as_deref()
            .map(ArtifactIdentity::parse)
            .transpose()
            .map_err(|e| {
                AppError::InvalidData(format!(
                    "Invalid embedding_artifact_identity for model_id '{}': {}",
                    record.model_id, e
                ))
            })?;

        Ok(DownloadedModel::from_db(StoredDownloadedModel {
            id: record.id,
            model_name: record.model_name,
            model_id: record.model_id,
            location,
            file_size_bytes: record.file_size_bytes,
            model_type: model_type_enum,
            architecture: record.architecture,
            downloaded_at: downloaded_at_dt,
            last_used_at: last_used_at_dt,
            use_count: record.use_count,
            is_active_for_chat: is_active_chat,
            is_active_for_embedding: is_active_embedding,
            metadata: metadata_val,
            is_active_for_utility: is_active_utility,
            embedding_artifact_identity,
        }))
    }
}

pub(super) fn build_model_select_query(suffix: &str) -> String {
    format!(
        r#"
{MODEL_SELECT_BASE}
{suffix}
"#
    )
}

pub(super) fn parse_optional_db_timestamp(value: Option<&str>) -> Result<Option<DateTime<Utc>>> {
    let Some(v) = value else {
        return Ok(None);
    };

    let trimmed = v.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }

    if let Some(parsed) = parse_db_timestamp(trimmed) {
        return Ok(Some(parsed));
    }

    warn!(timestamp = %trimmed, "Ignoring invalid timestamp format in downloaded model row");
    Ok(None)
}

fn parse_db_timestamp(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .map(|dt| dt.with_timezone(&Utc))
        .ok()
        .or_else(|| {
            NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S")
                .ok()
                .map(|naive| DateTime::<Utc>::from_naive_utc_and_offset(naive, Utc))
        })
}
