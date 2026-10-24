//! Downloaded Model Domain Entity
//!
//! Represents a model that has been downloaded to the local filesystem.
//! Tracks usage statistics and configuration for local models.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::fmt;
use std::path::{Path, PathBuf};

use crate::domain::models::classifier::ModelTypeClassifier;
use crate::domain::models::metadata::ModelType;
use crate::domain::value_objects::ArtifactIdentity;
use crate::shared::error::{AppError, Result};

///
/// This is orthogonal to role flags (chat/utility/embedding) and to the
/// `model_type` classification — a `LocalFile` GGUF can be a chat model
/// today and an embedding model tomorrow.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ModelLocation {
    LocalFile { path: PathBuf },
    LocalDirectory { path: PathBuf },
    RemoteOllama,
}

impl ModelLocation {
    /// Stable string for the SQL `storage_kind` discriminator column.
    pub fn kind_db_str(&self) -> &'static str {
        match self {
            ModelLocation::LocalFile { .. } => "local_file",
            ModelLocation::LocalDirectory { .. } => "local_dir",
            ModelLocation::RemoteOllama => "remote_ollama",
        }
    }

    /// On-disk path the inference engine should be handed. `None` for
    /// remote-only locations.
    pub fn loadable_path(&self) -> Option<&Path> {
        match self {
            ModelLocation::LocalFile { path } | ModelLocation::LocalDirectory { path } => {
                Some(path.as_path())
            }
            ModelLocation::RemoteOllama => None,
        }
    }

    /// Directory containing the model. For `LocalFile` this is the
    /// weight file's parent (sibling tokenizer/config files live there).
    /// For `LocalDirectory` it's the directory itself. `None` for remote.
    pub fn enclosing_dir(&self) -> Option<PathBuf> {
        match self {
            ModelLocation::LocalFile { path } => path.parent().map(|p| p.to_path_buf()),
            ModelLocation::LocalDirectory { path } => Some(path.clone()),
            ModelLocation::RemoteOllama => None,
        }
    }

    /// True for any variant that lives on the local filesystem.
    pub fn is_local(&self) -> bool {
        matches!(
            self,
            ModelLocation::LocalFile { .. } | ModelLocation::LocalDirectory { .. }
        )
    }

    /// Reconstruct from the `(storage_kind, storage_path)` columns.
    pub fn from_db(kind: &str, path: Option<String>) -> Result<Self> {
        let missing_path = || AppError::InvalidData(format!("{} row missing storage_path", kind));
        match kind {
            "local_file" => Ok(ModelLocation::LocalFile {
                path: PathBuf::from(path.ok_or_else(missing_path)?),
            }),
            "local_dir" => Ok(ModelLocation::LocalDirectory {
                path: PathBuf::from(path.ok_or_else(missing_path)?),
            }),
            "remote_ollama" => Ok(ModelLocation::RemoteOllama),
            other => Err(AppError::InvalidData(format!(
                "Invalid storage_kind '{}': expected 'local_file', 'local_dir', or 'remote_ollama'",
                other
            ))),
        }
    }

    /// `(storage_kind, storage_path)` pair for INSERT/UPDATE bindings.
    pub fn to_db(&self) -> (&'static str, Option<String>) {
        match self {
            ModelLocation::LocalFile { path } => {
                ("local_file", Some(path.to_string_lossy().into_owned()))
            }
            ModelLocation::LocalDirectory { path } => {
                ("local_dir", Some(path.to_string_lossy().into_owned()))
            }
            ModelLocation::RemoteOllama => ("remote_ollama", None),
        }
    }
}

impl fmt::Display for ModelLocation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ModelLocation::LocalFile { path } => write!(f, "local_file:{}", path.display()),
            ModelLocation::LocalDirectory { path } => write!(f, "local_dir:{}", path.display()),
            ModelLocation::RemoteOllama => f.write_str("remote_ollama"),
        }
    }
}

/// Domain entity for a downloaded model available for chat, embeddings, or utility.
/// Note: only one model can be active for each role at a time (enforced by DB triggers).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DownloadedModel {
    /// Unique identifier for this downloaded model record
    id: String,

    /// Human-readable name of the model
    model_name: String,

    /// Unique model identifier (e.g., "llama-3.2-1b-instruct")
    model_id: String,

    /// Where the model lives — local file, local directory, or remote.
    /// Replaces the previous `(backend, file_path)` pair where the
    /// combinations were not type-checked.
    location: ModelLocation,

    /// Size of the model in bytes (sum across files for multi-file
    /// layouts; 0 for remote-hosted models).
    file_size_bytes: i64,

    /// Type of model (chat or embedding)
    model_type: ModelType,

    /// Model architecture family (e.g., "llama", "mistral", "phi", "bert", "bge")
    /// Required by inference engines (Llama.cpp, ONNX Runtime) to identify model family
    architecture: String,

    /// Timestamp when model was downloaded
    downloaded_at: DateTime<Utc>,

    /// Timestamp when model was last used (None if never used)
    last_used_at: Option<DateTime<Utc>>,

    /// Number of times this model has been used
    use_count: i64,

    /// Whether this model is currently active for chat feature
    is_active_for_chat: bool,

    /// Whether this model is currently active for embedding feature
    is_active_for_embedding: bool,

    /// Additional metadata about the model (provider, version, capabilities, etc.)
    /// Stored as JSON for flexibility
    metadata: Option<JsonValue>,

    /// Whether this model is currently active for the utility role
    /// (HyDE expansion, router, intent classification)
    is_active_for_utility: bool,

    /// Content identity of the model's artifacts, recorded when the model is
    /// activated for embedding. `None` for remote models and for local models
    /// that have never been activated for embedding.
    embedding_artifact_identity: Option<ArtifactIdentity>,
}

/// Every field of a downloaded model as it was stored, for
/// [`DownloadedModel::from_db`].
pub struct StoredDownloadedModel {
    pub id: String,
    pub model_name: String,
    pub model_id: String,
    pub location: ModelLocation,
    pub file_size_bytes: i64,
    pub model_type: ModelType,
    pub architecture: String,
    pub downloaded_at: DateTime<Utc>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub use_count: i64,
    pub is_active_for_chat: bool,
    pub is_active_for_embedding: bool,
    pub metadata: Option<JsonValue>,
    pub is_active_for_utility: bool,
    pub embedding_artifact_identity: Option<ArtifactIdentity>,
}

impl DownloadedModel {
    /// Create a new downloaded model record.
    pub fn new(
        id: String,
        model_name: String,
        model_id: String,
        location: ModelLocation,
        file_size_bytes: i64,
        architecture: String,
        metadata: Option<JsonValue>,
    ) -> Result<Self> {
        Self::new_with_catalog(
            id,
            model_name,
            model_id,
            location,
            file_size_bytes,
            architecture,
            metadata,
            |_| None,
        )
    }

    /// Create a model using a catalog lookup supplied by the application.
    /// Catalog matches take precedence over filename and extension heuristics.
    pub fn new_with_catalog(
        id: String,
        model_name: String,
        model_id: String,
        location: ModelLocation,
        file_size_bytes: i64,
        architecture: String,
        metadata: Option<JsonValue>,
        catalog_lookup: impl FnOnce(&str) -> Option<ModelType>,
    ) -> Result<Self> {
        // Validate inputs
        if model_id.trim().is_empty() {
            return Err(AppError::InvalidInput(
                "model_id cannot be empty".to_string(),
            ));
        }

        if model_name.trim().is_empty() {
            return Err(AppError::InvalidInput(
                "model_name cannot be empty".to_string(),
            ));
        }

        if architecture.trim().is_empty() {
            return Err(AppError::InvalidInput(
                "architecture cannot be empty".to_string(),
            ));
        }

        // Local artifacts must have a real size; remote (Ollama) rows carry 0.
        if location.is_local() && file_size_bytes <= 0 {
            return Err(AppError::InvalidInput(
                "file_size_bytes must be greater than 0".to_string(),
            ));
        }

        // Classify model type using multi-strategy classifier. The
        // classifier inspects file extension when available; for
        // directory layouts it falls back to model_name heuristics.
        let classifier = ModelTypeClassifier;
        let classification =
            classifier.classify_with_catalog(&model_name, location.loadable_path(), catalog_lookup);

        tracing::info!(
            model_id = %model_id,
            model_name = %model_name,
            model_type = ?classification.model_type,
            strategy = ?classification.strategy,
            confidence = classification.confidence,
            "Model type classified"
        );

        let model_type = classification.model_type;

        Ok(Self {
            id,
            model_name,
            model_id,
            location,
            file_size_bytes,
            model_type,
            architecture,
            downloaded_at: Utc::now(),
            last_used_at: None,
            use_count: 0,
            is_active_for_chat: false,
            is_active_for_embedding: false,
            metadata,
            is_active_for_utility: false,
            embedding_artifact_identity: None,
        })
    }

    /// Create a DownloadedModel from database fields
    ///
    /// Used by repository when reconstructing from database.
    /// Does not perform validation since data is assumed to be valid in DB.
    pub fn from_db(stored: StoredDownloadedModel) -> Self {
        let StoredDownloadedModel {
            id,
            model_name,
            model_id,
            location,
            file_size_bytes,
            model_type,
            architecture,
            downloaded_at,
            last_used_at,
            use_count,
            is_active_for_chat,
            is_active_for_embedding,
            metadata,
            is_active_for_utility,
            embedding_artifact_identity,
        } = stored;
        Self {
            id,
            model_name,
            model_id,
            location,
            file_size_bytes,
            model_type,
            architecture,
            downloaded_at,
            last_used_at,
            use_count,
            is_active_for_chat,
            is_active_for_embedding,
            metadata,
            is_active_for_utility,
            embedding_artifact_identity,
        }
    }

    /// Mark this model as used
    ///
    /// Increments `use_count` and updates `last_used_at` to current time.
    ///
    /// # Business Logic
    ///
    /// - Atomically increments use count
    /// - Sets last_used_at to now
    /// - Returns self for method chaining
    pub fn mark_used(&mut self) {
        self.use_count += 1;
        self.last_used_at = Some(Utc::now());
    }

    pub fn set_active_for_chat(&mut self, active: bool) {
        self.is_active_for_chat = active;
    }

    pub fn set_active_for_utility(&mut self, active: bool) {
        self.is_active_for_utility = active;
    }

    /// Serialize metadata to JSON string.
    pub fn metadata_to_json(&self) -> Option<String> {
        self.metadata.as_ref().map(|m| m.to_string())
    }

    /// Deserialize metadata from JSON string.
    pub fn metadata_from_json(json: Option<&str>) -> Result<Option<JsonValue>> {
        match json {
            Some(s) if !s.trim().is_empty() => Ok(Some(serde_json::from_str(s)?)),
            _ => Ok(None),
        }
    }

    /// Validate that model type matches the intended operation.
    pub fn validate_for_operation(&self, for_chat: bool) -> Result<()> {
        if for_chat {
            if !self.model_type.is_chat_compatible() {
                return Err(AppError::InvalidInput(format!(
                    "Cannot use embedding model '{}' for chat. Please download a chat model (.gguf format).",
                    self.model_name
                )));
            }
        } else if !self.model_type.is_embedding_compatible() {
            return Err(AppError::InvalidInput(format!(
                "Cannot use chat model '{}' for embeddings. Please download an embedding model (.onnx format).",
                self.model_name
            )));
        }
        Ok(())
    }

    /// Check if this model is a chat model
    pub fn is_chat_model(&self) -> bool {
        self.model_type.is_chat_compatible()
    }

    /// Check if this model is an embedding model
    pub fn is_embedding_model(&self) -> bool {
        self.model_type.is_embedding_compatible()
    }

    // Getters
    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn model_name(&self) -> &str {
        &self.model_name
    }

    pub fn model_id(&self) -> &str {
        &self.model_id
    }

    pub fn location(&self) -> &ModelLocation {
        &self.location
    }

    pub fn loadable_path(&self) -> Option<&Path> {
        self.location.loadable_path()
    }

    pub fn file_size_bytes(&self) -> i64 {
        self.file_size_bytes
    }

    pub fn model_type(&self) -> ModelType {
        self.model_type
    }

    pub fn architecture(&self) -> &str {
        &self.architecture
    }

    pub fn downloaded_at(&self) -> &DateTime<Utc> {
        &self.downloaded_at
    }

    pub fn last_used_at(&self) -> Option<&DateTime<Utc>> {
        self.last_used_at.as_ref()
    }

    pub fn use_count(&self) -> i64 {
        self.use_count
    }

    pub fn is_active_for_chat(&self) -> bool {
        self.is_active_for_chat
    }

    pub fn is_active_for_embedding(&self) -> bool {
        self.is_active_for_embedding
    }

    pub fn metadata(&self) -> Option<&JsonValue> {
        self.metadata.as_ref()
    }

    pub fn is_active_for_utility(&self) -> bool {
        self.is_active_for_utility
    }

    pub fn embedding_artifact_identity(&self) -> Option<&ArtifactIdentity> {
        self.embedding_artifact_identity.as_ref()
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn catalog_classification_overrides_file_extension() {
        let model = DownloadedModel::new_with_catalog(
            "id".into(),
            "custom-model".into(),
            "custom-model".into(),
            ModelLocation::LocalFile {
                path: PathBuf::from("/tmp/model.gguf"),
            },
            100,
            "bert".into(),
            None,
            |_| Some(ModelType::TextEmbeddings),
        )
        .unwrap();
        assert!(model.is_embedding_model());
        assert!(!model.is_chat_model());
    }

    #[test]
    fn catalog_miss_preserves_extension_classification() {
        let model = DownloadedModel::new_with_catalog(
            "id".into(),
            "custom-model".into(),
            "custom-model".into(),
            ModelLocation::LocalFile {
                path: PathBuf::from("/tmp/model.gguf"),
            },
            100,
            "llama".into(),
            None,
            |_| None,
        )
        .unwrap();
        assert!(model.is_chat_model());
    }

    #[test]
    fn remote_ollama_allows_zero_size() {
        let result = DownloadedModel::new(
            "id-1".to_string(),
            "Ollama Server".to_string(),
            "__ollama_server__".to_string(),
            ModelLocation::RemoteOllama,
            0,
            "ollama".to_string(),
            None,
        );
        assert!(
            result.is_ok(),
            "RemoteOllama must permit zero-byte size, got: {:?}",
            result.err()
        );
        let model = result.unwrap();
        assert_eq!(model.location(), &ModelLocation::RemoteOllama);
        assert_eq!(model.file_size_bytes(), 0);
        assert!(model.loadable_path().is_none());
        assert!(!model.is_active_for_utility());
    }

    #[test]
    fn local_file_rejects_zero_size() {
        let result = DownloadedModel::new(
            "id-2".to_string(),
            "Local Model".to_string(),
            "local-model".to_string(),
            ModelLocation::LocalFile {
                path: PathBuf::from("/tmp/m.gguf"),
            },
            0,
            "llama".to_string(),
            None,
        );
        assert!(result.is_err(), "LocalFile must reject zero-byte size");
    }

    #[test]
    fn local_directory_rejects_zero_size() {
        let result = DownloadedModel::new(
            "id-3".to_string(),
            "Safetensors Model".to_string(),
            "safe-model".to_string(),
            ModelLocation::LocalDirectory {
                path: PathBuf::from("/tmp/safe"),
            },
            0,
            "bert".to_string(),
            None,
        );
        assert!(result.is_err(), "LocalDirectory must reject zero-byte size");
    }

    #[test]
    fn location_roundtrips_through_db_pair() {
        let cases = vec![
            ModelLocation::LocalFile {
                path: PathBuf::from("/tmp/m.gguf"),
            },
            ModelLocation::LocalDirectory {
                path: PathBuf::from("/tmp/safe"),
            },
            ModelLocation::RemoteOllama,
        ];
        for original in cases {
            let (kind, path) = original.to_db();
            let restored = ModelLocation::from_db(kind, path).expect("roundtrip parse");
            assert_eq!(restored, original);
        }
    }

    #[test]
    fn location_from_db_rejects_unknown_kind() {
        let err = ModelLocation::from_db("remote", Some("/tmp/x".into()));
        assert!(err.is_err(), "unknown storage_kind must be rejected");
    }

    #[test]
    fn local_file_enclosing_dir_is_parent() {
        let loc = ModelLocation::LocalFile {
            path: PathBuf::from("/tmp/safe/model.gguf"),
        };
        assert_eq!(loc.enclosing_dir(), Some(PathBuf::from("/tmp/safe")));
    }

    #[test]
    fn local_dir_enclosing_dir_is_self() {
        let loc = ModelLocation::LocalDirectory {
            path: PathBuf::from("/tmp/safe"),
        };
        assert_eq!(loc.enclosing_dir(), Some(PathBuf::from("/tmp/safe")));
    }
}
