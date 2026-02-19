use crate::domain::entities::model::Model;
use crate::domain::entities::model_file::ModelFile;
use crate::domain::repositories::model_repository::ModelRepository;
use crate::domain::value_objects::model_status::{FileStatus, ModelStatus};
use crate::error::AppError;
use chrono::Utc;
use std::sync::Arc;
use tracing::{debug, info};
use uuid::Uuid;

pub struct ModelService {
    repository: Arc<dyn ModelRepository>,
}

impl ModelService {
    pub fn new(repository: Arc<dyn ModelRepository>) -> Self {
        Self { repository }
    }

    pub async fn create_model(
        &self,
        model_id: String,
        name: String,
        description: Option<String>,
        base_path: String,
        architecture: String,
        model_type: String,
        files: Vec<ModelFileSpec>,
    ) -> Result<Model, AppError> {
        let existing = self.repository.find_by_model_id(&model_id).await?;
        if existing.is_some() {
            return Err(AppError::InvalidInput(format!(
                "Model already exists: {}",
                model_id
            )));
        }

        let total_size_bytes: i64 = files.iter().map(|f| f.size_bytes).sum();

        let model_files: Vec<ModelFile> = files
            .into_iter()
            .map(|spec| ModelFile {
                id: Uuid::new_v4().to_string(),
                model_id: model_id.clone(),
                file_name: spec.file_name,
                file_path: spec.file_path,
                relative_path: spec.relative_path,
                size_bytes: spec.size_bytes,
                downloaded_bytes: 0,
                checksum_sha256: spec.checksum_sha256,
                download_url: spec.download_url,
                status: FileStatus::Pending,
                created_at: Utc::now(),
                updated_at: Utc::now(),
                downloaded_at: None,
            })
            .collect();

        let model = Model {
            id: Uuid::new_v4().to_string(),
            model_id,
            name,
            description,
            base_path,
            total_size_bytes,
            architecture,
            model_type,
            status: ModelStatus::Pending,
            files: model_files,
            is_active_for_chat: false,
            is_active_for_embedding: false,
            use_count: 0,
            last_used_at: None,
            metadata: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            downloaded_at: None,
        };

        self.repository.create(&model).await?;
        info!(model_id = %model.model_id, "Model created");

        Ok(model)
    }

    pub async fn get_model(&self, model_id: &str) -> Result<Model, AppError> {
        self.repository
            .find_by_model_id(model_id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Model not found: {}", model_id)))
    }

    pub async fn list_models(&self) -> Result<Vec<Model>, AppError> {
        self.repository.find_all().await
    }

    pub async fn update_download_progress(
        &self,
        model_id: &str,
        file_name: &str,
        downloaded_bytes: i64,
    ) -> Result<(), AppError> {
        let model = self.get_model(model_id).await?;

        let file = model
            .files
            .iter()
            .find(|f| f.file_name == file_name)
            .ok_or_else(|| {
                AppError::NotFound(format!(
                    "File not found: {} in model {}",
                    file_name, model_id
                ))
            })?;

        let new_status = if downloaded_bytes >= file.size_bytes {
            FileStatus::Completed
        } else {
            FileStatus::Downloading
        };

        self.repository
            .update_file_progress(model_id, file_name, downloaded_bytes, new_status)
            .await?;

        if model.files.iter().all(|f| {
            f.file_name == file_name && new_status == FileStatus::Completed
                || f.file_name != file_name && f.status == FileStatus::Completed
        }) {
            self.repository
                .update_status(model_id, ModelStatus::Completed)
                .await?;
            info!(model_id = %model_id, "Model download completed");
        } else if model.status != ModelStatus::Downloading {
            self.repository
                .update_status(model_id, ModelStatus::Downloading)
                .await?;
        }

        debug!(model_id = %model_id, file_name = %file_name,
               downloaded_bytes, "Download progress updated");

        Ok(())
    }

    pub async fn mark_download_failed(
        &self,
        model_id: &str,
        file_name: Option<&str>,
    ) -> Result<(), AppError> {
        if let Some(fname) = file_name {
            self.repository
                .update_file_progress(model_id, fname, 0, FileStatus::Failed)
                .await?;
        }

        self.repository
            .update_status(model_id, ModelStatus::Failed)
            .await?;

        info!(model_id = %model_id, file_name = ?file_name, "Download marked as failed");
        Ok(())
    }

    pub async fn delete_model(&self, model_id: &str) -> Result<(), AppError> {
        self.repository.delete(model_id).await?;
        info!(model_id = %model_id, "Model deleted");
        Ok(())
    }

    pub async fn get_download_progress(
        &self,
        model_id: &str,
    ) -> Result<DownloadProgress, AppError> {
        let model = self.get_model(model_id).await?;

        let total_bytes = model.total_size_bytes;
        let downloaded_bytes: i64 = model.files.iter().map(|f| f.downloaded_bytes).sum();
        let progress_percent = if total_bytes > 0 {
            (downloaded_bytes as f32 / total_bytes as f32) * 100.0
        } else {
            0.0
        };

        Ok(DownloadProgress {
            model_id: model_id.to_string(),
            total_bytes,
            downloaded_bytes,
            progress_percent,
            status: model.status,
            file_progress: model
                .files
                .into_iter()
                .map(|f| FileProgress {
                    file_name: f.file_name,
                    size_bytes: f.size_bytes,
                    downloaded_bytes: f.downloaded_bytes,
                    status: f.status,
                })
                .collect(),
        })
    }
}

#[derive(Debug, Clone)]
pub struct ModelFileSpec {
    pub file_name: String,
    pub file_path: String,
    pub relative_path: String,
    pub size_bytes: i64,
    pub checksum_sha256: Option<String>,
    pub download_url: String,
}

#[derive(Debug, serde::Serialize)]
pub struct DownloadProgress {
    pub model_id: String,
    pub total_bytes: i64,
    pub downloaded_bytes: i64,
    pub progress_percent: f32,
    pub status: ModelStatus,
    pub file_progress: Vec<FileProgress>,
}

#[derive(Debug, serde::Serialize)]
pub struct FileProgress {
    pub file_name: String,
    pub size_bytes: i64,
    pub downloaded_bytes: i64,
    pub status: FileStatus,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::entities::model::Model;
    use crate::domain::entities::model_file::ModelFile;
    use crate::domain::repositories::mocks::MockModelRepository;
    use crate::domain::value_objects::model_status::{FileStatus, ModelStatus};
    use chrono::Utc;
    use std::sync::{Arc, Mutex};

    // ========================================
    // Test Helpers
    // ========================================

    /// Create a ModelService with a fresh mock repository
    ///
    /// Returns the service. Mock expectations must be set before calling
    /// service methods due to mockall's requirement for mutable access.
    fn create_test_service(mock_repo: MockModelRepository) -> ModelService {
        ModelService::new(Arc::new(mock_repo) as Arc<dyn ModelRepository>)
    }

    /// Create a test model specification with 1 file
    ///
    /// Returns (model_id, name, files) for use in create_model calls.
    /// Uses a unique UUID-based model ID to avoid test collisions.
    fn create_test_model_spec() -> (String, String, Vec<ModelFileSpec>) {
        let model_id = format!("test-model-{}", Uuid::new_v4());
        let name = "Test Model".to_string();
        let files = vec![ModelFileSpec {
            file_name: "model.bin".to_string(),
            file_path: "/path/to/model.bin".to_string(),
            relative_path: "model.bin".to_string(),
            size_bytes: 1024,
            checksum_sha256: Some("abc123".to_string()),
            download_url: "https://example.com/model.bin".to_string(),
        }];
        (model_id, name, files)
    }

    /// Create a test model specification with multiple files
    ///
    /// Returns (model_id, name, files) with 3 files for testing multi-file scenarios.
    fn create_multi_file_model_spec() -> (String, String, Vec<ModelFileSpec>) {
        let model_id = format!("test-model-{}", Uuid::new_v4());
        let name = "Multi-File Test Model".to_string();
        let files = vec![
            ModelFileSpec {
                file_name: "model.bin".to_string(),
                file_path: "/path/to/model.bin".to_string(),
                relative_path: "model.bin".to_string(),
                size_bytes: 1024,
                checksum_sha256: Some("abc123".to_string()),
                download_url: "https://example.com/model.bin".to_string(),
            },
            ModelFileSpec {
                file_name: "config.json".to_string(),
                file_path: "/path/to/config.json".to_string(),
                relative_path: "config.json".to_string(),
                size_bytes: 256,
                checksum_sha256: Some("def456".to_string()),
                download_url: "https://example.com/config.json".to_string(),
            },
            ModelFileSpec {
                file_name: "tokenizer.json".to_string(),
                file_path: "/path/to/tokenizer.json".to_string(),
                relative_path: "tokenizer.json".to_string(),
                size_bytes: 512,
                checksum_sha256: Some("ghi789".to_string()),
                download_url: "https://example.com/tokenizer.json".to_string(),
            },
        ];
        (model_id, name, files)
    }

    /// Create a test model entity with given ID, status, and files
    ///
    /// This helper creates a fully-populated Model entity for use in
    /// mock repository expectations and assertions.
    fn create_test_model(model_id: String, status: ModelStatus, files: Vec<ModelFile>) -> Model {
        let total_size_bytes = files.iter().map(|f| f.size_bytes).sum();

        Model {
            id: Uuid::new_v4().to_string(),
            model_id,
            name: "Test Model".to_string(),
            description: Some("A test model for unit testing".to_string()),
            base_path: "/path/to/models".to_string(),
            total_size_bytes,
            architecture: "test-arch".to_string(),
            model_type: "test-type".to_string(),
            status,
            files,
            is_active_for_chat: false,
            is_active_for_embedding: false,
            use_count: 0,
            last_used_at: None,
            metadata: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            downloaded_at: None,
        }
    }

    /// Create a test ModelFile with given file name and status
    ///
    /// Useful for building Model entities with specific file configurations.
    fn create_test_model_file(
        model_id: String,
        file_name: String,
        status: FileStatus,
        size_bytes: i64,
        downloaded_bytes: i64,
    ) -> ModelFile {
        ModelFile {
            id: Uuid::new_v4().to_string(),
            model_id,
            file_name: file_name.clone(),
            file_path: format!("/path/to/{}", file_name),
            relative_path: file_name,
            size_bytes,
            downloaded_bytes,
            checksum_sha256: Some("test-checksum".to_string()),
            download_url: "https://example.com/file".to_string(),
            status,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            downloaded_at: None,
        }
    }

    // ========================================
    // create_model Tests
    // ========================================

    #[tokio::test]
    async fn test_create_model_happy_path() {
        // Arrange - Create mock repository and test data
        let (model_id, name, files) = create_test_model_spec();
        let model_id_clone = model_id.clone();

        let mut mock_repo = MockModelRepository::new();
        mock_repo
            .expect_find_by_model_id()
            .withf(move |id| id == &model_id_clone)
            .times(1)
            .returning(|_| Ok(None));
        mock_repo.expect_create().times(1).returning(|_| Ok(()));

        let service = create_test_service(mock_repo);

        // Act - Create the model
        let result = service
            .create_model(
                model_id.clone(),
                name.clone(),
                Some("Test Description".to_string()),
                "/path/to/models".to_string(),
                "test-arch".to_string(),
                "test-type".to_string(),
                files,
            )
            .await;

        // Assert - Verify successful creation with correct properties
        assert!(result.is_ok(), "Model creation should succeed");
        let model = result.unwrap();
        assert_eq!(model.model_id, model_id);
        assert_eq!(model.name, name);
        assert_eq!(model.description, Some("Test Description".to_string()));
        assert_eq!(model.status, ModelStatus::Pending);
        assert_eq!(model.files.len(), 1);
        assert_eq!(model.files[0].file_name, "model.bin");
        assert_eq!(model.files[0].status, FileStatus::Pending);
        assert_eq!(model.files[0].downloaded_bytes, 0);
        assert_eq!(model.total_size_bytes, 1024);
    }

    #[tokio::test]
    async fn test_create_model_duplicate_id() {
        // Arrange - Create mock repository and test data
        let (model_id, name, files) = create_test_model_spec();
        let model_id_clone = model_id.clone();

        // Create an existing model
        let existing_file = create_test_model_file(
            model_id.clone(),
            "existing.bin".to_string(),
            FileStatus::Completed,
            1024,
            1024,
        );
        let existing_model = create_test_model(
            model_id.clone(),
            ModelStatus::Completed,
            vec![existing_file],
        );

        let mut mock_repo = MockModelRepository::new();
        mock_repo
            .expect_find_by_model_id()
            .withf(move |id| id == &model_id_clone)
            .times(1)
            .returning(move |_| Ok(Some(existing_model.clone())));

        let service = create_test_service(mock_repo);

        // Act - Attempt to create duplicate model
        let result = service
            .create_model(
                model_id.clone(),
                name,
                Some("Test Description".to_string()),
                "/path/to/models".to_string(),
                "test-arch".to_string(),
                "test-type".to_string(),
                files,
            )
            .await;

        // Assert - Should fail with InvalidInput error
        assert!(result.is_err(), "Duplicate model creation should fail");
        match result {
            Err(AppError::InvalidInput(msg)) => {
                assert!(
                    msg.contains("Model already exists"),
                    "Error should mention duplicate: {}",
                    msg
                );
            }
            _ => panic!("Expected InvalidInput error, got: {:?}", result),
        }
    }

    #[tokio::test]
    async fn test_create_model_empty_files() {
        // Arrange - Create mock repository with empty files list
        let model_id = format!("test-model-{}", Uuid::new_v4());
        let name = "Empty Files Model".to_string();
        let files: Vec<ModelFileSpec> = vec![];

        let mut mock_repo = MockModelRepository::new();
        mock_repo
            .expect_find_by_model_id()
            .times(1)
            .returning(|_| Ok(None));
        mock_repo.expect_create().times(1).returning(|_| Ok(()));

        let service = create_test_service(mock_repo);

        // Act - Create model with empty files
        let result = service
            .create_model(
                model_id.clone(),
                name,
                Some("Empty files test".to_string()),
                "/path/to/models".to_string(),
                "test-arch".to_string(),
                "test-type".to_string(),
                files,
            )
            .await;

        // Assert - Should succeed but have empty files and zero total size
        assert!(result.is_ok(), "Model with empty files should succeed");
        let model = result.unwrap();
        assert_eq!(model.files.len(), 0);
        assert_eq!(model.total_size_bytes, 0);
    }

    #[tokio::test]
    async fn test_create_model_calculates_total_size() {
        // Arrange - Create mock repository with multi-file spec
        let (model_id, name, files) = create_multi_file_model_spec();

        let mut mock_repo = MockModelRepository::new();
        mock_repo
            .expect_find_by_model_id()
            .times(1)
            .returning(|_| Ok(None));
        mock_repo.expect_create().times(1).returning(|_| Ok(()));

        let service = create_test_service(mock_repo);

        // Act - Create model with multiple files
        let result = service
            .create_model(
                model_id,
                name,
                Some("Multi-file test".to_string()),
                "/path/to/models".to_string(),
                "test-arch".to_string(),
                "test-type".to_string(),
                files,
            )
            .await;

        // Assert - Total size should be sum of all file sizes (1024 + 256 + 512 = 1792)
        assert!(result.is_ok(), "Multi-file model creation should succeed");
        let model = result.unwrap();
        assert_eq!(model.total_size_bytes, 1792);
        assert_eq!(model.files.len(), 3);
    }

    #[tokio::test]
    async fn test_create_model_sets_initial_state() {
        // Arrange - Create mock repository and test data
        let (model_id, name, files) = create_multi_file_model_spec();

        let mut mock_repo = MockModelRepository::new();
        mock_repo
            .expect_find_by_model_id()
            .times(1)
            .returning(|_| Ok(None));
        mock_repo.expect_create().times(1).returning(|_| Ok(()));

        let service = create_test_service(mock_repo);

        // Act - Create model
        let result = service
            .create_model(
                model_id,
                name,
                Some("Initial state test".to_string()),
                "/path/to/models".to_string(),
                "test-arch".to_string(),
                "test-type".to_string(),
                files,
            )
            .await;

        // Assert - Verify initial state is correct
        assert!(result.is_ok(), "Model creation should succeed");
        let model = result.unwrap();
        assert_eq!(
            model.status,
            ModelStatus::Pending,
            "Model status should be Pending"
        );
        assert_eq!(
            model.is_active_for_chat, false,
            "is_active_for_chat should be false"
        );
        assert_eq!(
            model.is_active_for_embedding, false,
            "is_active_for_embedding should be false"
        );

        // Verify all files have correct initial state
        for file in &model.files {
            assert_eq!(
                file.status,
                FileStatus::Pending,
                "File status should be Pending"
            );
            assert_eq!(file.downloaded_bytes, 0, "Downloaded bytes should be 0");
        }
    }

    // ========================================
    // update_download_progress Tests
    // ========================================

    #[tokio::test]
    async fn test_update_progress_transitions_to_downloading() {
        // Arrange - Create model with 1 file in Pending state
        let model_id = format!("test-model-{}", Uuid::new_v4());
        let file = create_test_model_file(
            model_id.clone(),
            "model.bin".to_string(),
            FileStatus::Pending,
            1024,
            0,
        );
        let model = create_test_model(model_id.clone(), ModelStatus::Pending, vec![file]);
        let model_clone = model.clone();

        let mut mock_repo = MockModelRepository::new();
        mock_repo
            .expect_find_by_model_id()
            .times(1)
            .returning(move |_| Ok(Some(model_clone.clone())));
        mock_repo
            .expect_update_file_progress()
            .withf(|_mid, fname, bytes, status| {
                fname == "model.bin" && *bytes == 512 && *status == FileStatus::Downloading
            })
            .times(1)
            .returning(|_, _, _, _| Ok(()));
        mock_repo
            .expect_update_status()
            .withf(|_mid, status| *status == ModelStatus::Downloading)
            .times(1)
            .returning(|_, _| Ok(()));

        let service = create_test_service(mock_repo);

        // Act - Update progress to 50%
        let result = service
            .update_download_progress(&model_id, "model.bin", 512)
            .await;

        // Assert - Should succeed and transition to Downloading
        assert!(result.is_ok(), "Progress update should succeed");
    }

    #[tokio::test]
    async fn test_update_progress_completes_file() {
        // Arrange - Create model with 1 file in Pending state
        let model_id = format!("test-model-{}", Uuid::new_v4());
        let file = create_test_model_file(
            model_id.clone(),
            "model.bin".to_string(),
            FileStatus::Pending,
            1024,
            0,
        );
        let model = create_test_model(model_id.clone(), ModelStatus::Pending, vec![file]);
        let model_clone = model.clone();

        let mut mock_repo = MockModelRepository::new();
        mock_repo
            .expect_find_by_model_id()
            .times(1)
            .returning(move |_| Ok(Some(model_clone.clone())));
        mock_repo
            .expect_update_file_progress()
            .withf(|_mid, fname, bytes, status| {
                fname == "model.bin" && *bytes == 1024 && *status == FileStatus::Completed
            })
            .times(1)
            .returning(|_, _, _, _| Ok(()));
        mock_repo
            .expect_update_status()
            .times(1)
            .returning(|_, _| Ok(()));

        let service = create_test_service(mock_repo);

        // Act - Update progress to 100%
        let result = service
            .update_download_progress(&model_id, "model.bin", 1024)
            .await;

        // Assert - Should succeed and mark file as completed
        assert!(result.is_ok(), "Progress update should succeed");
    }

    #[tokio::test]
    async fn test_update_progress_completes_model() {
        // Arrange - Create model with 2 files: one completed, one downloading
        let model_id = format!("test-model-{}", Uuid::new_v4());
        let file1 = create_test_model_file(
            model_id.clone(),
            "file1.bin".to_string(),
            FileStatus::Completed,
            1024,
            1024,
        );
        let file2 = create_test_model_file(
            model_id.clone(),
            "file2.bin".to_string(),
            FileStatus::Downloading,
            1024,
            512,
        );
        let model = create_test_model(
            model_id.clone(),
            ModelStatus::Downloading,
            vec![file1, file2],
        );
        let model_clone = model.clone();

        let mut mock_repo = MockModelRepository::new();
        mock_repo
            .expect_find_by_model_id()
            .times(1)
            .returning(move |_| Ok(Some(model_clone.clone())));
        mock_repo
            .expect_update_file_progress()
            .withf(|_mid, fname, bytes, status| {
                fname == "file2.bin" && *bytes == 1024 && *status == FileStatus::Completed
            })
            .times(1)
            .returning(|_, _, _, _| Ok(()));
        mock_repo
            .expect_update_status()
            .withf(|_mid, status| *status == ModelStatus::Completed)
            .times(1)
            .returning(|_, _| Ok(()));

        let service = create_test_service(mock_repo);

        // Act - Complete the second file
        let result = service
            .update_download_progress(&model_id, "file2.bin", 1024)
            .await;

        // Assert - Should succeed and complete the model
        assert!(
            result.is_ok(),
            "Progress update should succeed and complete model"
        );
    }

    #[tokio::test]
    async fn test_update_progress_file_not_found() {
        // Arrange - Create model with 1 file
        let model_id = format!("test-model-{}", Uuid::new_v4());
        let file = create_test_model_file(
            model_id.clone(),
            "model.bin".to_string(),
            FileStatus::Pending,
            1024,
            0,
        );
        let model = create_test_model(model_id.clone(), ModelStatus::Pending, vec![file]);
        let model_clone = model.clone();

        let mut mock_repo = MockModelRepository::new();
        mock_repo
            .expect_find_by_model_id()
            .times(1)
            .returning(move |_| Ok(Some(model_clone.clone())));

        let service = create_test_service(mock_repo);

        // Act - Try to update progress for non-existent file
        let result = service
            .update_download_progress(&model_id, "nonexistent.bin", 512)
            .await;

        // Assert - Should fail with NotFound error
        assert!(result.is_err(), "Update should fail for non-existent file");
        match result {
            Err(AppError::NotFound(msg)) => {
                assert!(
                    msg.contains("File not found"),
                    "Error should mention file not found: {}",
                    msg
                );
                assert!(
                    msg.contains("nonexistent.bin"),
                    "Error should mention file name: {}",
                    msg
                );
            }
            _ => panic!("Expected NotFound error, got: {:?}", result),
        }
    }

    #[tokio::test]
    async fn test_update_progress_model_not_found() {
        // Arrange - Create mock repository that returns None
        let model_id = "nonexistent-model-id";

        let mut mock_repo = MockModelRepository::new();
        mock_repo
            .expect_find_by_model_id()
            .times(1)
            .returning(|_| Ok(None));

        let service = create_test_service(mock_repo);

        // Act - Try to update progress for non-existent model
        let result = service
            .update_download_progress(model_id, "model.bin", 512)
            .await;

        // Assert - Should fail with NotFound error
        assert!(result.is_err(), "Update should fail for non-existent model");
        match result {
            Err(AppError::NotFound(msg)) => {
                assert!(
                    msg.contains("Model not found"),
                    "Error should mention model not found: {}",
                    msg
                );
            }
            _ => panic!("Expected NotFound error, got: {:?}", result),
        }
    }

    // ========================================
    // get_download_progress Tests
    // ========================================

    #[tokio::test]
    async fn test_get_progress_calculates_correctly() {
        // Arrange - Create model with 2 files: one complete (1024/1024), one half done (512/1024)
        let model_id = format!("test-model-{}", Uuid::new_v4());
        let file1 = create_test_model_file(
            model_id.clone(),
            "file1.bin".to_string(),
            FileStatus::Completed,
            1024,
            1024,
        );
        let file2 = create_test_model_file(
            model_id.clone(),
            "file2.bin".to_string(),
            FileStatus::Downloading,
            1024,
            512,
        );
        let model = create_test_model(
            model_id.clone(),
            ModelStatus::Downloading,
            vec![file1, file2],
        );
        let model_clone = model.clone();

        let mut mock_repo = MockModelRepository::new();
        mock_repo
            .expect_find_by_model_id()
            .times(1)
            .returning(move |_| Ok(Some(model_clone.clone())));

        let service = create_test_service(mock_repo);

        // Act - Get download progress
        let result = service.get_download_progress(&model_id).await;

        // Assert - Verify progress calculation
        assert!(result.is_ok(), "Get progress should succeed");
        let progress = result.unwrap();
        assert_eq!(progress.total_bytes, 2048, "Total should be 2048");
        assert_eq!(progress.downloaded_bytes, 1536, "Downloaded should be 1536");
        assert!(
            (progress.progress_percent - 75.0).abs() < 0.01,
            "Progress should be 75%, got {}",
            progress.progress_percent
        );
        assert_eq!(
            progress.file_progress.len(),
            2,
            "Should have 2 file progress entries"
        );
    }

    #[tokio::test]
    async fn test_get_progress_zero_total_size() {
        // Arrange - Create model with zero-byte files
        let model_id = format!("test-model-{}", Uuid::new_v4());
        let file = create_test_model_file(
            model_id.clone(),
            "empty.bin".to_string(),
            FileStatus::Pending,
            0,
            0,
        );
        let model = create_test_model(model_id.clone(), ModelStatus::Pending, vec![file]);
        let model_clone = model.clone();

        let mut mock_repo = MockModelRepository::new();
        mock_repo
            .expect_find_by_model_id()
            .times(1)
            .returning(move |_| Ok(Some(model_clone.clone())));

        let service = create_test_service(mock_repo);

        // Act - Get download progress
        let result = service.get_download_progress(&model_id).await;

        // Assert - Should not divide by zero, should return 0% progress
        assert!(
            result.is_ok(),
            "Get progress should succeed even with zero size"
        );
        let progress = result.unwrap();
        assert_eq!(progress.total_bytes, 0);
        assert_eq!(progress.downloaded_bytes, 0);
        assert_eq!(
            progress.progress_percent, 0.0,
            "Progress should be 0% for zero-byte model"
        );
    }

    #[tokio::test]
    async fn test_get_progress_model_not_found() {
        // Arrange - Create mock repository that returns None
        let model_id = "nonexistent-model-id";

        let mut mock_repo = MockModelRepository::new();
        mock_repo
            .expect_find_by_model_id()
            .times(1)
            .returning(|_| Ok(None));

        let service = create_test_service(mock_repo);

        // Act - Try to get progress for non-existent model
        let result = service.get_download_progress(model_id).await;

        // Assert - Should fail with NotFound error
        assert!(
            result.is_err(),
            "Get progress should fail for non-existent model"
        );
        match result {
            Err(AppError::NotFound(msg)) => {
                assert!(
                    msg.contains("Model not found"),
                    "Error should mention model not found: {}",
                    msg
                );
            }
            _ => panic!("Expected NotFound error, got: {:?}", result),
        }
    }

    // ========================================
    // P1 Tests
    // ========================================

    #[tokio::test]
    async fn test_mark_download_failed() {
        // Arrange - Create model with 1 file in Downloading status
        let model_id = format!("test-model-{}", Uuid::new_v4());
        let file = create_test_model_file(
            model_id.clone(),
            "model.bin".to_string(),
            FileStatus::Downloading,
            1024,
            512,
        );
        let model = create_test_model(model_id.clone(), ModelStatus::Downloading, vec![file]);
        let model_id_clone = model_id.clone();

        let mut mock_repo = MockModelRepository::new();
        mock_repo
            .expect_update_file_progress()
            .withf(move |mid, fname, bytes, status| {
                mid == &model_id_clone
                    && fname == "model.bin"
                    && *bytes == 0
                    && *status == FileStatus::Failed
            })
            .times(1)
            .returning(|_, _, _, _| Ok(()));
        mock_repo
            .expect_update_status()
            .withf(|_mid, status| *status == ModelStatus::Failed)
            .times(1)
            .returning(|_, _| Ok(()));

        let service = create_test_service(mock_repo);

        // Act - Mark download as failed
        let result = service
            .mark_download_failed(&model_id, Some("model.bin"))
            .await;

        // Assert - Should succeed and mark file and model as failed
        assert!(result.is_ok(), "Mark download failed should succeed");
    }

    #[tokio::test]
    async fn test_mark_download_failed_file_not_found() {
        // Arrange - Create model with 1 file named "model.bin"
        let model_id = format!("test-model-{}", Uuid::new_v4());

        let mut mock_repo = MockModelRepository::new();
        mock_repo
            .expect_update_file_progress()
            .withf(|_mid, fname, bytes, status| {
                fname == "nonexistent.bin" && *bytes == 0 && *status == FileStatus::Failed
            })
            .times(1)
            .returning(|_, _, _, _| Err(AppError::NotFound("File not found in model".to_string())));

        let service = create_test_service(mock_repo);

        // Act - Try to mark non-existent file as failed
        let result = service
            .mark_download_failed(&model_id, Some("nonexistent.bin"))
            .await;

        // Assert - Should fail with NotFound error
        assert!(
            result.is_err(),
            "Mark download failed should fail for non-existent file"
        );
        match result {
            Err(AppError::NotFound(msg)) => {
                assert!(
                    msg.contains("File not found"),
                    "Error should mention file not found: {}",
                    msg
                );
            }
            _ => panic!("Expected NotFound error, got: {:?}", result),
        }
    }

    #[tokio::test]
    async fn test_get_model_happy_path() {
        // Arrange - Create test model
        let model_id = format!("test-model-{}", Uuid::new_v4());
        let file = create_test_model_file(
            model_id.clone(),
            "model.bin".to_string(),
            FileStatus::Completed,
            1024,
            1024,
        );
        let test_model = create_test_model(model_id.clone(), ModelStatus::Completed, vec![file]);
        let model_clone = test_model.clone();
        let model_id_clone = model_id.clone();

        let mut mock_repo = MockModelRepository::new();
        mock_repo
            .expect_find_by_model_id()
            .withf(move |id| id == &model_id_clone)
            .times(1)
            .returning(move |_| Ok(Some(model_clone.clone())));

        let service = create_test_service(mock_repo);

        // Act - Get model
        let result = service.get_model(&model_id).await;

        // Assert - Should return the model with correct properties
        assert!(result.is_ok(), "Get model should succeed");
        let model = result.unwrap();
        assert_eq!(model.model_id, model_id, "Model ID should match");
        assert_eq!(model.name, "Test Model", "Model name should match");
        assert_eq!(
            model.status,
            ModelStatus::Completed,
            "Model status should be Completed"
        );
        assert_eq!(model.files.len(), 1, "Model should have 1 file");
        assert_eq!(
            model.files[0].file_name, "model.bin",
            "File name should match"
        );
    }

    #[tokio::test]
    async fn test_get_model_not_found() {
        // Arrange - Configure mock to return None
        let model_id = "nonexistent-model-id";

        let mut mock_repo = MockModelRepository::new();
        mock_repo
            .expect_find_by_model_id()
            .withf(|id| id == "nonexistent-model-id")
            .times(1)
            .returning(|_| Ok(None));

        let service = create_test_service(mock_repo);

        // Act - Try to get non-existent model
        let result = service.get_model(model_id).await;

        // Assert - Should fail with NotFound error
        assert!(
            result.is_err(),
            "Get model should fail for non-existent model"
        );
        match result {
            Err(AppError::NotFound(msg)) => {
                assert!(
                    msg.contains("Model not found"),
                    "Error should mention model not found: {}",
                    msg
                );
                assert!(
                    msg.contains("nonexistent-model-id"),
                    "Error should mention model ID: {}",
                    msg
                );
            }
            _ => panic!("Expected NotFound error, got: {:?}", result),
        }
    }

    // ========================================
    // P2 Tests
    // ========================================

    #[tokio::test]
    async fn test_list_models() {
        // Arrange - Create 3 test models
        let model1 = create_test_model(
            "model-1".to_string(),
            ModelStatus::Completed,
            vec![create_test_model_file(
                "model-1".to_string(),
                "file1.bin".to_string(),
                FileStatus::Completed,
                1024,
                1024,
            )],
        );
        let model2 = create_test_model(
            "model-2".to_string(),
            ModelStatus::Downloading,
            vec![create_test_model_file(
                "model-2".to_string(),
                "file2.bin".to_string(),
                FileStatus::Downloading,
                2048,
                1024,
            )],
        );
        let model3 = create_test_model(
            "model-3".to_string(),
            ModelStatus::Pending,
            vec![create_test_model_file(
                "model-3".to_string(),
                "file3.bin".to_string(),
                FileStatus::Pending,
                4096,
                0,
            )],
        );

        let models = vec![model1.clone(), model2.clone(), model3.clone()];

        let mut mock_repo = MockModelRepository::new();
        mock_repo
            .expect_find_all()
            .times(1)
            .returning(move || Ok(models.clone()));

        let service = create_test_service(mock_repo);

        // Act - List models
        let result = service.list_models().await;

        // Assert - Should return 3 models with expected properties
        assert!(result.is_ok(), "List models should succeed");
        let returned_models = result.unwrap();
        assert_eq!(returned_models.len(), 3, "Should return 3 models");

        // Verify first model
        assert_eq!(returned_models[0].model_id, "model-1");
        assert_eq!(returned_models[0].status, ModelStatus::Completed);
        assert_eq!(returned_models[0].files.len(), 1);

        // Verify second model
        assert_eq!(returned_models[1].model_id, "model-2");
        assert_eq!(returned_models[1].status, ModelStatus::Downloading);
        assert_eq!(returned_models[1].files.len(), 1);

        // Verify third model
        assert_eq!(returned_models[2].model_id, "model-3");
        assert_eq!(returned_models[2].status, ModelStatus::Pending);
        assert_eq!(returned_models[2].files.len(), 1);
    }

    #[tokio::test]
    async fn test_delete_model() {
        // Arrange - Create test service
        let model_id = "test-model-to-delete";
        let model_id_clone = model_id.to_string();

        let mut mock_repo = MockModelRepository::new();
        mock_repo
            .expect_delete()
            .withf(move |id| id == &model_id_clone)
            .times(1)
            .returning(|_| Ok(()));

        let service = create_test_service(mock_repo);

        // Act - Delete model
        let result = service.delete_model(model_id).await;

        // Assert - Should succeed
        assert!(result.is_ok(), "Delete model should succeed");
    }
}
