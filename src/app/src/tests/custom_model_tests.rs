#![allow(clippy::panic)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::assertions_on_constants)]
#![allow(clippy::useless_vec)]
#![allow(clippy::indexing_slicing)]
#![allow(unused_variables)]
#![allow(unused_imports)]
#![allow(deprecated)]

//! Unit tests for custom model domain logic
//!
//! Tests the CustomModel aggregate and value objects.
use std::path::PathBuf;
use uuid::Uuid;
use lattice::features::custom_model::domain::{
    CustomModel, FileInfo, ModelArchitecture, ModelId, ModelMetadata, ModelName, ModelSource,
    SourceType, TaskType, ValidationStatus, MAX_MODEL_FILE_SIZE_BYTES,
};
use lattice::features::custom_model::repository::{
    CustomModelRepositoryTrait, MockCustomModelRepository,
};

// ============================================================================
// Value Object Tests
// ============================================================================

#[test]
fn test_model_id_validation() {
    // Valid model IDs
    assert!(ModelId::new("user/my-model".to_string()).is_ok());
    assert!(ModelId::new("org/llama-3.2-1b".to_string()).is_ok());

    // Empty model ID
    assert!(ModelId::new("".to_string()).is_err());
    assert!(ModelId::new("   ".to_string()).is_err());

    // Too long model ID
    let long_id = "a".repeat(201);
    assert!(ModelId::new(long_id).is_err());

    // Valid at max length
    let max_id = "a".repeat(200);
    assert!(ModelId::new(max_id).is_ok());
}

#[test]
fn test_model_name_validation() {
    // Valid names
    assert!(ModelName::new("My Custom Model".to_string()).is_ok());
    assert!(ModelName::new("Llama 3.2 1B".to_string()).is_ok());

    // Empty name
    assert!(ModelName::new("".to_string()).is_err());
    assert!(ModelName::new("   ".to_string()).is_err());

    // Too long name
    let long_name = "a".repeat(101);
    assert!(ModelName::new(long_name).is_err());

    // Valid at max length
    let max_name = "a".repeat(100);
    assert!(ModelName::new(max_name).is_ok());
}

#[test]
fn test_source_type_conversions() {
    // to_db_string
    assert_eq!(SourceType::Url.to_db_string(), "url");
    assert_eq!(SourceType::LocalFile.to_db_string(), "local_file");

    // from_db_string
    assert_eq!(SourceType::from_db_string("url").unwrap(), SourceType::Url);
    assert_eq!(
        SourceType::from_db_string("local_file").unwrap(),
        SourceType::LocalFile
    );

    // Invalid
    assert!(SourceType::from_db_string("invalid").is_err());
}

#[test]
fn test_model_source() {
    // URL source
    let url_source = ModelSource::Url("https://example.com/model.gguf".to_string());
    assert_eq!(url_source.source_type(), SourceType::Url);
    assert_eq!(url_source.source_value(), "https://example.com/model.gguf");

    // LocalFile source
    let file_source = ModelSource::LocalFile(PathBuf::from("/tmp/model.gguf"));
    assert_eq!(file_source.source_type(), SourceType::LocalFile);
    assert!(file_source.source_value().contains("model.gguf"));
}

#[test]
fn test_model_source_validation() {
    // URL validation
    assert!(ModelSource::validate_url("https://example.com").is_ok());
    assert!(ModelSource::validate_url("http://example.com").is_ok());
    assert!(ModelSource::validate_url("").is_err());
    assert!(ModelSource::validate_url("ftp://example.com").is_err());
}

#[test]
fn test_architecture_inference() {
    // GGUF
    assert_eq!(
        ModelArchitecture::infer_from_path(&PathBuf::from("model.gguf")),
        Some(ModelArchitecture::Gguf)
    );
    assert_eq!(
        ModelArchitecture::infer_from_path(&PathBuf::from("/path/to/model.GGUF")),
        Some(ModelArchitecture::Gguf)
    );

    // ONNX
    assert_eq!(
        ModelArchitecture::infer_from_path(&PathBuf::from("model.onnx")),
        Some(ModelArchitecture::Onnx)
    );

    // SafeTensors
    assert_eq!(
        ModelArchitecture::infer_from_path(&PathBuf::from("model.safetensors")),
        Some(ModelArchitecture::SafeTensors)
    );

    // PyTorch
    assert_eq!(
        ModelArchitecture::infer_from_path(&PathBuf::from("model.pt")),
        Some(ModelArchitecture::PyTorch)
    );
    assert_eq!(
        ModelArchitecture::infer_from_path(&PathBuf::from("model.pth")),
        Some(ModelArchitecture::PyTorch)
    );
    assert_eq!(
        ModelArchitecture::infer_from_path(&PathBuf::from("model.bin")),
        Some(ModelArchitecture::PyTorch)
    );

    // Unknown
    assert_eq!(
        ModelArchitecture::infer_from_path(&PathBuf::from("model.unknown")),
        None
    );
}

#[test]
fn test_file_info_validation() {
    // Valid file info
    assert!(FileInfo::new(PathBuf::from("/models/test.gguf"), 1_000_000).is_ok());

    // Zero size (invalid)
    assert!(FileInfo::new(PathBuf::from("/models/test.gguf"), 0).is_err());

    // Negative size (invalid)
    assert!(FileInfo::new(PathBuf::from("/models/test.gguf"), -1).is_err());

    // Too large (> 5GB)
    assert!(FileInfo::new(
        PathBuf::from("/models/test.gguf"),
        MAX_MODEL_FILE_SIZE_BYTES + 1
    )
    .is_err());

    // Exactly at limit (valid)
    assert!(FileInfo::new(
        PathBuf::from("/models/test.gguf"),
        MAX_MODEL_FILE_SIZE_BYTES
    )
    .is_ok());
}

#[test]
fn test_task_type_conversions() {
    // to_db_string
    assert_eq!(TaskType::Embedding.to_db_string(), "embedding");
    assert_eq!(TaskType::Chat.to_db_string(), "chat");
    assert_eq!(TaskType::Ocr.to_db_string(), "ocr");
    assert_eq!(TaskType::Vision.to_db_string(), "vision");

    // from_db_string
    assert_eq!(
        TaskType::from_db_string("embedding").unwrap(),
        TaskType::Embedding
    );
    assert_eq!(TaskType::from_db_string("chat").unwrap(), TaskType::Chat);
    assert_eq!(TaskType::from_db_string("ocr").unwrap(), TaskType::Ocr);
    assert_eq!(
        TaskType::from_db_string("vision").unwrap(),
        TaskType::Vision
    );

    // Invalid
    assert!(TaskType::from_db_string("invalid").is_err());
}

#[test]
fn test_validation_status_conversions() {
    // to_db_string
    assert_eq!(ValidationStatus::Pending.to_db_string(), "pending");
    assert_eq!(ValidationStatus::Validating.to_db_string(), "validating");
    assert_eq!(ValidationStatus::Valid.to_db_string(), "valid");
    assert_eq!(ValidationStatus::Invalid.to_db_string(), "invalid");

    // from_db_string
    assert_eq!(
        ValidationStatus::from_db_string("pending").unwrap(),
        ValidationStatus::Pending
    );
    assert_eq!(
        ValidationStatus::from_db_string("validating").unwrap(),
        ValidationStatus::Validating
    );
    assert_eq!(
        ValidationStatus::from_db_string("valid").unwrap(),
        ValidationStatus::Valid
    );
    assert_eq!(
        ValidationStatus::from_db_string("invalid").unwrap(),
        ValidationStatus::Invalid
    );

    // Invalid
    assert!(ValidationStatus::from_db_string("invalid_status").is_err());
}

// ============================================================================
// CustomModel Aggregate Tests
// ============================================================================

#[test]
fn test_custom_model_creation() {
    let name = ModelName::new("Test Model".to_string()).unwrap();
    let model_id = ModelId::new("user/test-model".to_string()).unwrap();
    let source = ModelSource::LocalFile(PathBuf::from("/tmp/test.gguf"));
    let file_path = PathBuf::from("/models/test.gguf");

    let model = CustomModel::new(
        name,
        model_id,
        source,
        file_path,
        1_000_000,
        TaskType::Chat,
        None,
    )
    .unwrap();

    assert_eq!(model.validation_status(), ValidationStatus::Pending);
    assert!(model.validation_error().is_none());
    assert_eq!(model.architecture(), Some(ModelArchitecture::Gguf));
    assert_eq!(model.task_type(), TaskType::Chat);
    assert!(model.last_validated_at().is_none());
}

#[test]
fn test_custom_model_file_size_validation() {
    let name = ModelName::new("Test Model".to_string()).unwrap();
    let model_id = ModelId::new("user/test-model".to_string()).unwrap();
    let source = ModelSource::LocalFile(PathBuf::from("/tmp/test.gguf"));
    let file_path = PathBuf::from("/models/test.gguf");

    // Zero size
    assert!(CustomModel::new(
        name.clone(),
        model_id.clone(),
        source.clone(),
        file_path.clone(),
        0,
        TaskType::Chat,
        None
    )
    .is_err());

    // Too large
    assert!(CustomModel::new(
        name.clone(),
        model_id.clone(),
        source.clone(),
        file_path.clone(),
        MAX_MODEL_FILE_SIZE_BYTES + 1,
        TaskType::Chat,
        None
    )
    .is_err());

    // Valid
    assert!(CustomModel::new(
        name,
        model_id,
        source,
        file_path,
        1_000_000,
        TaskType::Chat,
        None
    )
    .is_ok());
}

#[test]
fn test_validation_state_transitions() {
    let name = ModelName::new("Test Model".to_string()).unwrap();
    let model_id = ModelId::new("user/test-model".to_string()).unwrap();
    let source = ModelSource::Url("https://example.com/model.gguf".to_string());
    let file_path = PathBuf::from("/models/test.gguf");

    let mut model = CustomModel::new(
        name,
        model_id,
        source,
        file_path,
        1_000_000,
        TaskType::Chat,
        None,
    )
    .unwrap();

    // Initial state
    assert_eq!(model.validation_status(), ValidationStatus::Pending);
    assert!(model.last_validated_at().is_none());

    // Pending → Validating
    model.mark_validating();
    assert_eq!(model.validation_status(), ValidationStatus::Validating);
    assert!(model.validation_error().is_none());

    // Validating → Valid
    model.mark_valid();
    assert_eq!(model.validation_status(), ValidationStatus::Valid);
    assert!(model.validation_error().is_none());
    assert!(model.last_validated_at().is_some());

    // Valid → Validating → Invalid
    model.mark_validating();
    assert_eq!(model.validation_status(), ValidationStatus::Validating);

    model.mark_invalid("Test error message".to_string());
    assert_eq!(model.validation_status(), ValidationStatus::Invalid);
    assert_eq!(model.validation_error(), Some("Test error message"));
    assert!(model.last_validated_at().is_some());
}

#[test]
fn test_invariant_invalid_status_requires_error() {
    let name = ModelName::new("Test Model".to_string()).unwrap();
    let model_id = ModelId::new("user/test-model".to_string()).unwrap();
    let source = ModelSource::LocalFile(PathBuf::from("/tmp/test.gguf"));
    let file_path = PathBuf::from("/models/test.gguf");

    let mut model = CustomModel::new(
        name,
        model_id,
        source,
        file_path,
        1_000_000,
        TaskType::Chat,
        None,
    )
    .unwrap();

    // Mark as invalid with error (valid)
    model.mark_invalid("Error message".to_string());
    assert!(model.check_invariants().is_ok());
}

#[test]
fn test_needs_revalidation() {
    let name = ModelName::new("Test Model".to_string()).unwrap();
    let model_id = ModelId::new("user/test-model".to_string()).unwrap();
    let source = ModelSource::LocalFile(PathBuf::from("/tmp/test.gguf"));
    let file_path = PathBuf::from("/models/test.gguf");

    let mut model = CustomModel::new(
        name,
        model_id,
        source,
        file_path,
        1_000_000,
        TaskType::Chat,
        None,
    )
    .unwrap();

    // Pending → needs revalidation
    assert!(model.needs_revalidation());

    // Validating → doesn't need revalidation
    model.mark_validating();
    assert!(!model.needs_revalidation());

    // Valid (just validated) → doesn't need revalidation yet
    model.mark_valid();
    assert!(!model.needs_revalidation());

    // Invalid → needs revalidation
    model.mark_invalid("Error".to_string());
    assert!(model.needs_revalidation());
}

#[test]
fn test_metadata_handling() {
    use serde_json::json;

    let metadata_value = json!({
        "provider": "custom",
        "version": "1.0",
        "capabilities": ["chat", "completion"]
    });
    let metadata = ModelMetadata::new(metadata_value.clone());

    let name = ModelName::new("Test Model".to_string()).unwrap();
    let model_id = ModelId::new("user/test-model".to_string()).unwrap();
    let source = ModelSource::LocalFile(PathBuf::from("/tmp/test.gguf"));
    let file_path = PathBuf::from("/models/test.gguf");

    let model = CustomModel::new(
        name,
        model_id,
        source,
        file_path,
        1_000_000,
        TaskType::Chat,
        Some(metadata.clone()),
    )
    .unwrap();

    assert!(model.metadata().is_some());
    assert_eq!(model.metadata().unwrap().value(), &metadata_value);

    // Test JSON serialization
    let json_str = metadata.to_json();
    let parsed = ModelMetadata::from_json(&json_str).unwrap();
    assert_eq!(parsed.value(), &metadata_value);
}

// ============================================================================
// Repository Tests (Mock)
// ============================================================================

#[tokio::test]
async fn test_mock_repository_save_and_find() {
    let repo = MockCustomModelRepository::new();

    let name = ModelName::new("Test Model".to_string()).unwrap();
    let model_id = ModelId::new("user/test-model".to_string()).unwrap();
    let source = ModelSource::LocalFile(PathBuf::from("/tmp/test.gguf"));
    let file_path = PathBuf::from("/models/test.gguf");

    let model = CustomModel::new(
        name,
        model_id,
        source,
        file_path,
        1_000_000,
        TaskType::Chat,
        None,
    )
    .unwrap();

    // Save
    repo.save(&model).await.unwrap();

    // Find by ID
    let id = Uuid::parse_str(model.id()).unwrap();
    let found = repo.find_by_id(&id).await.unwrap();
    assert!(found.is_some());
    assert_eq!(found.unwrap().model_id().as_str(), "user/test-model");

    // Find by model_id
    let found_by_model_id = repo.find_by_model_id("user/test-model").await.unwrap();
    assert!(found_by_model_id.is_some());
}

#[tokio::test]
async fn test_mock_repository_find_all() {
    let repo = MockCustomModelRepository::new();

    // Create and save multiple models
    for i in 1..=3 {
        let name = ModelName::new(format!("Model {}", i)).unwrap();
        let model_id = ModelId::new(format!("user/model-{}", i)).unwrap();
        let source = ModelSource::LocalFile(PathBuf::from(format!("/tmp/model{}.gguf", i)));
        let file_path = PathBuf::from(format!("/models/model{}.gguf", i));

        let model = CustomModel::new(
            name,
            model_id,
            source,
            file_path,
            1_000_000,
            TaskType::Chat,
            None,
        )
        .unwrap();
        repo.save(&model).await.unwrap();
    }

    // Find all
    let all = repo.find_all().await.unwrap();
    assert_eq!(all.len(), 3);
}

#[tokio::test]
async fn test_mock_repository_find_by_task_type() {
    let repo = MockCustomModelRepository::new();

    // Create chat model
    let chat_model = create_test_model("chat-model", TaskType::Chat);
    repo.save(&chat_model).await.unwrap();

    // Create embedding model
    let embedding_model = create_test_model("embedding-model", TaskType::Embedding);
    repo.save(&embedding_model).await.unwrap();

    // Find chat models
    let chat_models = repo.find_by_task_type(TaskType::Chat).await.unwrap();
    assert_eq!(chat_models.len(), 1);
    assert_eq!(chat_models[0].task_type(), TaskType::Chat);

    // Find embedding models
    let embedding_models = repo.find_by_task_type(TaskType::Embedding).await.unwrap();
    assert_eq!(embedding_models.len(), 1);
    assert_eq!(embedding_models[0].task_type(), TaskType::Embedding);
}

#[tokio::test]
async fn test_mock_repository_find_by_validation_status() {
    let repo = MockCustomModelRepository::new();

    // Create pending model
    let pending_model = create_test_model("pending-model", TaskType::Chat);
    repo.save(&pending_model).await.unwrap();

    // Create valid model
    let mut valid_model = create_test_model("valid-model", TaskType::Chat);
    valid_model.mark_validating();
    valid_model.mark_valid();
    repo.save(&valid_model).await.unwrap();

    // Find pending models
    let pending = repo
        .find_by_validation_status(ValidationStatus::Pending)
        .await
        .unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].validation_status(), ValidationStatus::Pending);

    // Find valid models
    let valid = repo
        .find_by_validation_status(ValidationStatus::Valid)
        .await
        .unwrap();
    assert_eq!(valid.len(), 1);
    assert_eq!(valid[0].validation_status(), ValidationStatus::Valid);
}

#[tokio::test]
async fn test_mock_repository_delete() {
    let repo = MockCustomModelRepository::new();

    let model = create_test_model("test-model", TaskType::Chat);
    let id = Uuid::parse_str(model.id()).unwrap();

    // Save
    repo.save(&model).await.unwrap();
    assert!(repo.find_by_id(&id).await.unwrap().is_some());

    // Delete
    repo.delete(&id).await.unwrap();
    assert!(repo.find_by_id(&id).await.unwrap().is_none());
}

#[tokio::test]
async fn test_mock_repository_update_validation_status() {
    let repo = MockCustomModelRepository::new();

    let model = create_test_model("test-model", TaskType::Chat);
    let id = Uuid::parse_str(model.id()).unwrap();

    // Save
    repo.save(&model).await.unwrap();

    // Update to validating
    repo.update_validation_status(&id, ValidationStatus::Validating, None)
        .await
        .unwrap();
    let updated = repo.find_by_id(&id).await.unwrap().unwrap();
    assert_eq!(updated.validation_status(), ValidationStatus::Validating);

    // Update to valid
    repo.update_validation_status(&id, ValidationStatus::Valid, None)
        .await
        .unwrap();
    let updated = repo.find_by_id(&id).await.unwrap().unwrap();
    assert_eq!(updated.validation_status(), ValidationStatus::Valid);

    // Update to invalid with error
    repo.update_validation_status(
        &id,
        ValidationStatus::Invalid,
        Some("Test error".to_string()),
    )
    .await
    .unwrap();
    let updated = repo.find_by_id(&id).await.unwrap().unwrap();
    assert_eq!(updated.validation_status(), ValidationStatus::Invalid);
    assert_eq!(updated.validation_error(), Some("Test error"));
}

// ============================================================================
// Helper Functions
// ============================================================================

fn create_test_model(model_id_str: &str, task_type: TaskType) -> CustomModel {
    let name = ModelName::new(format!("Test {}", model_id_str)).unwrap();
    let model_id = ModelId::new(format!("user/{}", model_id_str)).unwrap();
    let source = ModelSource::LocalFile(PathBuf::from(format!("/tmp/{}.gguf", model_id_str)));
    let file_path = PathBuf::from(format!("/models/{}.gguf", model_id_str));

    CustomModel::new(
        name, model_id, source, file_path, 1_000_000, task_type, None,
    )
    .unwrap()
}
