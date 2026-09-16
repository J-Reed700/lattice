//! Get File Path By ID Use Case
//!
//! Retrieves the file path for a document by its ID.

use crate::application::ports::{DocumentRepositoryPort, FileStoragePort};
use crate::features::file::dto::{GetFilePathByIdRequestDto, GetFilePathByIdResponseDto};
use crate::infrastructure::security::FileAccessConfig;
use crate::shared::error::{AppError, Result};
use std::path::PathBuf;
use std::sync::Arc;

/// Use case for retrieving a file path by document ID.
///
/// This use case:
/// 1. Looks up the file path from the document repository
/// 2. Resolves it to an absolute path
/// 3. Checks if the file exists on disk
/// 4. Returns the path and existence status
pub struct GetFilePathByIdUseCase {
    document_repository: Arc<dyn DocumentRepositoryPort>,
    file_storage: Arc<dyn FileStoragePort>,
    file_access_config: Arc<FileAccessConfig>,
    vault_path: PathBuf,
}

impl GetFilePathByIdUseCase {
    /// Create a new use case instance.
    pub fn new(
        document_repository: Arc<dyn DocumentRepositoryPort>,
        file_storage: Arc<dyn FileStoragePort>,
        file_access_config: Arc<FileAccessConfig>,
        vault_path: PathBuf,
    ) -> Self {
        Self {
            document_repository,
            file_storage,
            file_access_config,
            vault_path,
        }
    }

    /// Execute the use case.
    ///
    /// # Arguments
    ///
    /// * `request` - Request containing the document ID
    ///
    /// # Returns
    ///
    /// Response containing the file path and whether it exists on disk.
    ///
    /// # Errors
    ///
    /// - `AppError::NotFound` if document does not exist
    /// - `AppError::Database` if repository query fails
    pub async fn execute(
        &self,
        request: GetFilePathByIdRequestDto,
    ) -> Result<GetFilePathByIdResponseDto> {
        // Look up file path from repository
        let relative_path = self
            .document_repository
            .find_file_path_by_id(&request.document_id)
            .await?;

        // Resolve to absolute path
        let full_path = self.vault_path.join(&relative_path);

        // CRITICAL: Validate resolved path to prevent directory traversal (CWE-22)
        let validated_path = self
            .file_access_config
            .validate_path(full_path.to_string_lossy().to_string())
            .map_err(|e| AppError::InvalidInput(format!("Invalid file path: {}", e)))?;

        let exists = self.file_storage.exists(&validated_path).await;

        Ok(GetFilePathByIdResponseDto {
            path: validated_path.display().to_string(),
            exists,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::ports::{DocumentRepositoryPort, RepositoryPort};
    use crate::domain::entities::Document;
    use crate::shared::error::AppError;
    use async_trait::async_trait;
    use std::path::Path;

    struct MockDocumentRepository {
        documents: Vec<(String, String)>,
    }

    impl MockDocumentRepository {
        fn new() -> Self {
            Self {
                documents: Vec::new(),
            }
        }

        fn with_document(mut self, id: &str, path: &str) -> Self {
            self.documents.push((id.to_string(), path.to_string()));
            self
        }
    }

    #[async_trait]
    impl RepositoryPort<Document> for MockDocumentRepository {
        async fn find_by_id(&self, _id: &str) -> Result<Option<Document>> {
            unimplemented!()
        }

        async fn find_by_filter(
            &self,
            _filter: &dyn crate::application::ports::repository_port::Filter,
        ) -> Result<Vec<Document>> {
            unimplemented!()
        }

        async fn find_all(&self) -> Result<Vec<Document>> {
            unimplemented!()
        }

        async fn save(&self, _entity: &Document) -> Result<()> {
            unimplemented!()
        }

        async fn save_batch(&self, _entities: &[Document]) -> Result<()> {
            unimplemented!()
        }

        async fn delete(&self, _id: &str) -> Result<()> {
            unimplemented!()
        }

        async fn delete_batch(&self, _ids: &[&str]) -> Result<()> {
            unimplemented!()
        }

        async fn count(&self) -> Result<usize> {
            unimplemented!()
        }

        async fn exists(&self, _id: &str) -> Result<bool> {
            unimplemented!()
        }
    }

    #[async_trait]
    impl DocumentRepositoryPort for MockDocumentRepository {
        async fn find_file_path_by_id(&self, document_id: &str) -> Result<String> {
            self.documents
                .iter()
                .find(|(id, _)| id == document_id)
                .map(|(_, path)| path.clone())
                .ok_or_else(|| AppError::NotFound(format!("Document not found: {}", document_id)))
        }

        async fn document_exists(&self, document_id: &str) -> Result<bool> {
            Ok(self.documents.iter().any(|(id, _)| id == document_id))
        }

        async fn find_id_by_path(&self, file_path: &str) -> Result<Option<String>> {
            Ok(self
                .documents
                .iter()
                .find(|(_, path)| path == file_path)
                .map(|(id, _)| id.clone()))
        }

        async fn delete(&self, _document_id: &str) -> Result<()> {
            unimplemented!()
        }

        async fn find_by_checksum(
            &self,
            _checksum: &crate::domain::value_objects::Checksum,
        ) -> Result<Option<crate::domain::entities::Document>> {
            Ok(None)
        }

        async fn count_documents(&self) -> Result<i64> {
            Ok(self.documents.len() as i64)
        }

        async fn count_chunks(&self) -> Result<i64> {
            Ok(0)
        }

        async fn find_all_paginated(
            &self,
            _limit: usize,
        ) -> Result<Vec<crate::domain::entities::Document>> {
            Ok(vec![])
        }
    }

    struct MockFileStorage {
        existing_files: Vec<String>,
    }

    impl MockFileStorage {
        fn new() -> Self {
            Self {
                existing_files: Vec::new(),
            }
        }

        fn with_file(mut self, path: &str) -> Self {
            self.existing_files.push(path.to_string());
            self
        }
    }

    #[async_trait]
    impl crate::application::ports::FileStoragePort for MockFileStorage {
        async fn read_file(&self, _path: &Path) -> Result<String> {
            Ok(String::new())
        }

        async fn read_file_bytes(&self, _path: &Path) -> Result<Vec<u8>> {
            Ok(Vec::new())
        }

        async fn write_file(&self, _path: &Path, _content: &str) -> Result<()> {
            Ok(())
        }

        async fn write_file_bytes(&self, _path: &Path, _content: &[u8]) -> Result<()> {
            Ok(())
        }

        async fn delete_file(&self, _path: &Path) -> Result<()> {
            Ok(())
        }

        async fn compute_hash(&self, _path: &Path) -> Result<String> {
            Ok("mock_hash".to_string())
        }

        async fn exists(&self, path: &Path) -> bool {
            self.existing_files
                .iter()
                .any(|p| p == &path.display().to_string())
        }

        async fn metadata(&self, _path: &Path) -> Result<crate::application::ports::FileMetadata> {
            Ok(crate::application::ports::FileMetadata {
                size: 0,
                modified_at: 0,
                is_file: true,
                is_directory: false,
            })
        }
    }

    #[tokio::test]
    async fn test_get_file_path_by_id_success_file_exists() {
        use crate::infrastructure::security::FileAccessConfig;

        let doc_repo =
            Arc::new(MockDocumentRepository::new().with_document("doc-123", "docs/file.txt"));
        let file_storage = Arc::new(MockFileStorage::new().with_file("/lattice/docs/file.txt"));
        let file_access_config = Arc::new(FileAccessConfig::new(vec![PathBuf::from("/lattice")]));
        let vault_path = PathBuf::from("/lattice");

        let use_case =
            GetFilePathByIdUseCase::new(doc_repo, file_storage, file_access_config, vault_path);

        let request = GetFilePathByIdRequestDto {
            document_id: "doc-123".to_string(),
        };

        let result = use_case.execute(request).await;

        assert!(result.is_ok());
        let response = result.unwrap();
        assert_eq!(response.path, "/lattice/docs/file.txt");
        assert!(response.exists);
    }

    #[tokio::test]
    async fn test_get_file_path_by_id_success_file_missing() {
        use crate::infrastructure::security::FileAccessConfig;

        let doc_repo =
            Arc::new(MockDocumentRepository::new().with_document("doc-123", "docs/missing.txt"));
        let file_storage = Arc::new(MockFileStorage::new()); // File not on disk
        let file_access_config = Arc::new(FileAccessConfig::new(vec![PathBuf::from("/lattice")]));
        let vault_path = PathBuf::from("/lattice");

        let use_case =
            GetFilePathByIdUseCase::new(doc_repo, file_storage, file_access_config, vault_path);

        let request = GetFilePathByIdRequestDto {
            document_id: "doc-123".to_string(),
        };

        let result = use_case.execute(request).await;

        assert!(result.is_ok());
        let response = result.unwrap();
        assert_eq!(response.path, "/lattice/docs/missing.txt");
        assert!(!response.exists);
    }

    #[tokio::test]
    async fn test_get_file_path_by_id_document_not_found() {
        use crate::infrastructure::security::FileAccessConfig;

        let doc_repo = Arc::new(MockDocumentRepository::new()); // No documents
        let file_storage = Arc::new(MockFileStorage::new());
        let file_access_config = Arc::new(FileAccessConfig::new(vec![PathBuf::from("/lattice")]));
        let vault_path = PathBuf::from("/lattice");

        let use_case =
            GetFilePathByIdUseCase::new(doc_repo, file_storage, file_access_config, vault_path);

        let request = GetFilePathByIdRequestDto {
            document_id: "nonexistent".to_string(),
        };

        let result = use_case.execute(request).await;

        assert!(result.is_err());
        assert!(matches!(result, Err(AppError::NotFound(_))));
    }
}
