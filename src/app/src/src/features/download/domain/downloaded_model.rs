//! Downloaded Model Domain Entity
//!
//! Represents a model that has been downloaded to the local filesystem.
//! Tracks usage statistics and configuration for local models.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::fmt;
use std::path::{Path, PathBuf};

use crate::domain::model_type_classifier::ModelTypeClassifier;

// Re-export ModelType from model_metadata for backward compatibility
pub use crate::domain::model_metadata::ModelType;

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
    pub fn from_db(kind: &str, path: Option<String>) -> Result<Self, String> {
        match kind {
            "local_file" => Ok(ModelLocation::LocalFile {
                path: PathBuf::from(
                    path.ok_or_else(|| "local_file row missing storage_path".to_string())?,
                ),
            }),
            "local_dir" => Ok(ModelLocation::LocalDirectory {
                path: PathBuf::from(
                    path.ok_or_else(|| "local_dir row missing storage_path".to_string())?,
                ),
            }),
            "remote_ollama" => Ok(ModelLocation::RemoteOllama),
            other => Err(format!(
                "Invalid storage_kind '{}': expected 'local_file', 'local_dir', or 'remote_ollama'",
                other
            )),
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

/// Domain entity for a downloaded model
///
/// This entity represents a model file that has been downloaded
/// and is available for use in the application (e.g., for chat, embeddings, OCR).
///
/// ## Business Rules
///
/// - Only one model can be active for chat at a time (enforced by database trigger)
/// - Only one model can be active for embedding at a time (enforced by database trigger)
/// - `use_count` increments each time the model is used
/// - `last_used_at` updates to current time when model is used
/// - Model metadata stored as JSON for flexibility
/// - Model type must match operation type (chat models for chat, embedding models for embeddings)
///
/// ## Invariants
///
/// - `model_id` must be unique across all downloaded models
/// - `file_path` must point to an existing file when model is created
/// - `file_size_bytes` must be > 0
/// - `use_count` must be >= 0
/// - `model_type` must match file extension (.gguf for chat, .safetensors for embedding)
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
}

impl DownloadedModel {
    /// Create a new downloaded model record
    ///
    /// # Arguments
    ///
    /// * `id` - Unique identifier for this record
    /// * `model_name` - Human-readable name
    /// * `model_id` - Unique model identifier
    /// * `file_path` - Absolute path to model file
    /// * `file_size_bytes` - Size of file in bytes
    /// * `architecture` - Model architecture family (e.g., "llama", "mistral", "phi")
    /// * `metadata` - Optional JSON metadata
    ///
    /// # Business Logic
    ///
    /// - Sets `downloaded_at` to current timestamp
    /// - Initializes `use_count` to 0
    /// - Sets `last_used_at` to None (never used yet)
    /// - Sets `is_active_for_chat` to false by default
    /// - Sets `is_active_for_embedding` to false by default
    /// - Infers `model_type` from file extension
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - `file_size_bytes` <= 0
    /// - `model_id` is empty
    /// - `model_name` is empty
    /// - `architecture` is empty
    pub fn new(
        id: String,
        model_name: String,
        model_id: String,
        location: ModelLocation,
        file_size_bytes: i64,
        architecture: String,
        metadata: Option<JsonValue>,
    ) -> Result<Self, String> {
        // Validate inputs
        if model_id.trim().is_empty() {
            return Err("model_id cannot be empty".to_string());
        }

        if model_name.trim().is_empty() {
            return Err("model_name cannot be empty".to_string());
        }

        if architecture.trim().is_empty() {
            return Err("architecture cannot be empty".to_string());
        }

        // Local artifacts must have a real size; remote (Ollama) rows carry 0.
        if location.is_local() && file_size_bytes <= 0 {
            return Err("file_size_bytes must be greater than 0".to_string());
        }

        // Classify model type using multi-strategy classifier. The
        // classifier inspects file extension when available; for
        // directory layouts it falls back to model_name heuristics.
        let classifier = ModelTypeClassifier;
        let classification = classifier.classify(&model_name, location.loadable_path());

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
        })
    }

    /// Create a DownloadedModel from database fields
    ///
    /// Used by repository when reconstructing from database.
    /// Does not perform validation since data is assumed to be valid in DB.
    #[allow(clippy::too_many_arguments)]
    pub fn from_db(
        id: String,
        model_name: String,
        model_id: String,
        location: ModelLocation,
        file_size_bytes: i64,
        model_type: ModelType,
        architecture: String,
        downloaded_at: DateTime<Utc>,
        last_used_at: Option<DateTime<Utc>>,
        use_count: i64,
        is_active_for_chat: bool,
        is_active_for_embedding: bool,
        metadata: Option<JsonValue>,
        is_active_for_utility: bool,
    ) -> Self {
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

    /// Set whether this model is active for chat
    ///
    /// # Arguments
    ///
    /// * `active` - True to set as active chat model, false otherwise
    ///
    /// # Business Logic
    ///
    /// - Database trigger ensures only one model can be active for chat
    /// - This just updates the field; trigger handles deactivating others
    pub fn set_active_for_chat(&mut self, active: bool) {
        self.is_active_for_chat = active;
    }

    /// Set whether this model is active for embedding
    ///
    /// # Arguments
    ///
    /// * `active` - True to set as active embedding model, false otherwise
    ///
    /// # Business Logic
    ///
    /// - Database trigger ensures only one model can be active for embedding
    /// - This just updates the field; trigger handles deactivating others
    pub fn set_active_for_embedding(&mut self, active: bool) {
        self.is_active_for_embedding = active;
    }

    /// Set whether this model is active for the utility role
    ///
    /// # Arguments
    ///
    /// * `active` - True to set as active utility model, false otherwise
    ///
    /// # Business Logic
    ///
    /// - Database trigger ensures only one model can be active for utility
    /// - This just updates the field; trigger handles deactivating others
    pub fn set_active_for_utility(&mut self, active: bool) {
        self.is_active_for_utility = active;
    }

    /// Serialize metadata to JSON string
    ///
    /// Returns JSON string for database storage, or None if no metadata.
    pub fn metadata_to_json(&self) -> Option<String> {
        self.metadata.as_ref().map(|m| m.to_string())
    }

    /// Deserialize metadata from JSON string
    ///
    /// # Arguments
    ///
    /// * `json` - JSON string to parse
    ///
    /// # Errors
    ///
    /// Returns error if JSON is invalid
    pub fn metadata_from_json(json: Option<&str>) -> Result<Option<JsonValue>, String> {
        match json {
            Some(s) if !s.trim().is_empty() => {
                serde_json::from_str(s).map(Some).map_err(|e| e.to_string())
            }
            _ => Ok(None),
        }
    }

    /// Validate that model type matches the intended operation
    ///
    /// # Arguments
    ///
    /// * `for_chat` - True if validating for chat operations
    ///
    /// # Returns
    ///
    /// Ok(()) if model type is compatible, Err with user-friendly message otherwise
    ///
    /// # Examples
    ///
    /// ```
    /// let chat_model = DownloadedModel::new(..., PathBuf::from("model.gguf"), ...)?;
    /// assert!(chat_model.validate_for_operation(true).is_ok());
    /// assert!(chat_model.validate_for_operation(false).is_err());
    /// ```
    pub fn validate_for_operation(&self, for_chat: bool) -> Result<(), String> {
        if for_chat {
            if !self.model_type.is_chat_compatible() {
                return Err(format!(
                    "Cannot use embedding model '{}' for chat. Please download a chat model (.gguf format).",
                    self.model_name
                ));
            }
        } else if !self.model_type.is_embedding_compatible() {
            return Err(format!(
                "Cannot use chat model '{}' for embeddings. Please download an embedding model (.onnx format).",
                self.model_name
            ));
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
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]
mod tests {
    use super::*;
    use std::path::PathBuf;

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
