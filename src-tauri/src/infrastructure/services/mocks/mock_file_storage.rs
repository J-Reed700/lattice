//! Mock implementations for testing
//!
//! This module provides mock implementations of service traits.

#[cfg(test)]
use crate::infrastructure::services::traits::*;
#[cfg(test)]
use crate::shared::error::Result;
#[cfg(test)]
use async_trait::async_trait;

#[cfg(test)]
/// Mock file storage service for testing
pub struct MockFileStorageService {
    files: std::sync::Arc<parking_lot::RwLock<std::collections::HashMap<String, FileRecord>>>,
}

#[cfg(test)]
impl MockFileStorageService {
    pub fn new() -> Self {
        Self {
            files: std::sync::Arc::new(parking_lot::RwLock::new(std::collections::HashMap::new())),
        }
    }
}

#[cfg(test)]
impl Default for MockFileStorageService {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
#[cfg(test)]
impl FileStorageServiceTrait for MockFileStorageService {
    async fn store_file(
        &self,
        source_path: crate::shared::domain_types::ValidatedFilePath,
        mime_type: &str,
        metadata: Option<serde_json::Value>,
    ) -> Result<FileRecord> {
        let path = source_path.as_path();

        let record = FileRecord {
            id: uuid::Uuid::new_v4().to_string(),
            content_hash: "mock-hash".to_string(),
            file_name: path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown")
                .to_string(),
            file_extension: path
                .extension()
                .and_then(|e| e.to_str())
                .map(|s| s.to_string()),
            mime_type: mime_type.to_string(),
            size_bytes: 0,
            storage_path: path.to_string_lossy().to_string(),
            is_indexed: false,
            created_at: chrono::Utc::now().timestamp(),
            accessed_at: chrono::Utc::now().timestamp(),
            ref_count: 1,
            metadata,
        };

        self.files.write().insert(record.id.clone(), record.clone());
        Ok(record)
    }

    async fn get_file_path(&self, file_id: &str) -> Result<std::path::PathBuf> {
        self.files
            .read()
            .get(file_id)
            .map(|r| std::path::PathBuf::from(&r.storage_path))
            .ok_or_else(|| {
                crate::shared::error::AppError::NotFound(format!("File not found: {}", file_id))
            })
    }

    async fn delete_file(&self, file_id: &str) -> Result<()> {
        self.files.write().remove(file_id).ok_or_else(|| {
            crate::shared::error::AppError::NotFound(format!("File not found: {}", file_id))
        })?;
        Ok(())
    }

    async fn get_file_metadata(&self, file_id: &str) -> Result<Option<serde_json::Value>> {
        Ok(self
            .files
            .read()
            .get(file_id)
            .and_then(|r| r.metadata.clone()))
    }
}
