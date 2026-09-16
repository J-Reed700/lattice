use super::model_file::ModelFile;
use crate::domain::model_metadata::ModelType;
use crate::domain::value_objects::model_status::{FileStatus, ModelStatus};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Model {
    pub id: String,
    pub model_id: String,
    pub name: String,
    pub description: Option<String>,
    pub base_path: String,
    pub total_size_bytes: i64,
    pub architecture: String,
    pub model_type: String,
    pub status: ModelStatus,
    pub files: Vec<ModelFile>,
    pub is_active_for_chat: bool,
    pub is_active_for_embedding: bool,
    pub use_count: u64,
    pub last_used_at: Option<DateTime<Utc>>,
    pub metadata: Option<serde_json::Value>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub downloaded_at: Option<DateTime<Utc>>,
}

impl Model {
    pub fn new(
        id: String,
        model_name: String,
        model_id: String,
        file_path: std::path::PathBuf,
        file_size_bytes: i64,
        metadata: Option<serde_json::Value>,
    ) -> Result<Self, String> {
        if file_size_bytes <= 0 {
            return Err("file_size_bytes must be > 0".to_string());
        }
        if model_id.is_empty() {
            return Err("model_id cannot be empty".to_string());
        }
        if model_name.is_empty() {
            return Err("model_name cannot be empty".to_string());
        }

        let base_path_str = file_path
            .parent()
            .ok_or_else(|| "file_path must have a parent directory".to_string())?
            .to_str()
            .ok_or_else(|| "file_path parent directory is not valid UTF-8".to_string())?
            .to_string();

        let now = Utc::now();

        Ok(Self {
            id,
            model_id,
            name: model_name,
            description: None,
            base_path: base_path_str,
            total_size_bytes: file_size_bytes,
            architecture: "unknown".to_string(),
            model_type: "unknown".to_string(),
            status: ModelStatus::Pending,
            files: vec![],
            is_active_for_chat: false,
            is_active_for_embedding: false,
            use_count: 0,
            last_used_at: None,
            metadata,
            created_at: now,
            updated_at: now,
            downloaded_at: Some(now),
        })
    }

    pub fn progress(&self) -> f32 {
        if self.total_size_bytes == 0 {
            return 0.0;
        }
        let downloaded: i64 = self.files.iter().map(|f| f.downloaded_bytes).sum();
        downloaded as f32 / self.total_size_bytes as f32
    }

    pub fn is_complete(&self) -> bool {
        self.files.iter().all(|f| f.status == FileStatus::Completed)
    }

    pub fn model_id(&self) -> &str {
        &self.model_id
    }

    pub fn model_name(&self) -> &str {
        &self.name
    }

    pub fn file_path(&self) -> std::path::PathBuf {
        std::path::PathBuf::from(&self.base_path)
    }

    pub fn validate_for_operation(&self, is_chat: bool) -> Result<(), String> {
        let model_type = ModelType::from_db_string(&self.model_type)?;
        if is_chat {
            if !model_type.is_chat_compatible() {
                return Err(format!(
                    "Cannot use embedding model '{}' for chat. Please download a chat model (.gguf format).",
                    self.name
                ));
            }
        } else if !model_type.is_embedding_compatible() {
            return Err(format!(
                "Cannot use chat model '{}' for embeddings. Please download an embedding model (.onnx format).",
                self.name
            ));
        }
        Ok(())
    }
}
