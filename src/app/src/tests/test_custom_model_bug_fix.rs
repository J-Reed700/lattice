#![allow(clippy::panic)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::assertions_on_constants)]
#![allow(clippy::useless_vec)]
#![allow(clippy::indexing_slicing)]
#![allow(unused_variables)]
#![allow(unused_imports)]
#![allow(deprecated)]

use std::path::PathBuf;
/// Test for custom model bug fixes
///
/// This test verifies that:
/// 1. FileInfo accepts file_size_bytes = 1 (placeholder value)
/// 2. CustomModel::new() accepts file_size_bytes = 1
/// 3. Database constraint CHECK(file_size_bytes > 0) is satisfied
use vault::features::custom_model::domain::{CustomModel, ModelId, ModelName, ModelSource, TaskType};

#[test]
fn test_placeholder_size_satisfies_constraint() {
    // This is the placeholder size used in add_from_url_use_case
    let placeholder_size = 1i64;

    // Create value objects
    let name = ModelName::new("Test Model".to_string()).expect("valid name");
    let model_id = ModelId::new("user/test".to_string()).expect("valid model_id");
    let source = ModelSource::Url("https://example.com/model.gguf".to_string());
    let path = PathBuf::from("/tmp/models/test.downloading");

    // Test: CustomModel::new() should accept placeholder_size = 1
    let result = CustomModel::new(
        name,
        model_id,
        source,
        path,
        placeholder_size,
        TaskType::Embedding,
        None,
    );

    // Assert: Should succeed
    assert!(
        result.is_ok(),
        "CustomModel::new() should accept file_size_bytes = 1, got error: {:?}",
        result.err()
    );

    let model = result.unwrap();

    // Verify file size is stored correctly
    assert_eq!(model.file_info().size_bytes(), 1);

    // Verify invariants hold
    assert!(
        model.check_invariants().is_ok(),
        "Model invariants should hold with file_size_bytes = 1"
    );
}

#[test]
fn test_zero_size_fails() {
    // This is the old buggy code that used 0
    let zero_size = 0i64;

    let name = ModelName::new("Test Model".to_string()).expect("valid name");
    let model_id = ModelId::new("user/test".to_string()).expect("valid model_id");
    let source = ModelSource::Url("https://example.com/model.gguf".to_string());
    let path = PathBuf::from("/tmp/models/test.downloading");

    // Test: CustomModel::new() should REJECT zero_size
    let result = CustomModel::new(
        name,
        model_id,
        source,
        path,
        zero_size,
        TaskType::Embedding,
        None,
    );

    // Assert: Should fail
    assert!(
        result.is_err(),
        "CustomModel::new() should reject file_size_bytes = 0"
    );

    let error = result.unwrap_err();
    assert!(
        error.contains("must be greater than 0"),
        "Error message should mention size constraint, got: {}",
        error
    );
}

#[test]
fn test_max_size_boundary() {
    // Test maximum size (5GB)
    let max_size = 5_368_709_120i64; // 5GB

    let name = ModelName::new("Large Model".to_string()).expect("valid name");
    let model_id = ModelId::new("user/large".to_string()).expect("valid model_id");
    let source = ModelSource::Url("https://example.com/large.gguf".to_string());
    let path = PathBuf::from("/tmp/models/large.gguf");

    // Test: Should accept exactly 5GB
    let result = CustomModel::new(
        name.clone(),
        model_id.clone(),
        source.clone(),
        path.clone(),
        max_size,
        TaskType::Chat,
        None,
    );

    assert!(
        result.is_ok(),
        "CustomModel::new() should accept max_size = 5GB, got error: {:?}",
        result.err()
    );

    // Test: Should reject 5GB + 1 byte
    let over_max = max_size + 1;

    let name2 = ModelName::new("Too Large".to_string()).expect("valid name");
    let model_id2 = ModelId::new("user/toolarge".to_string()).expect("valid model_id");

    let result = CustomModel::new(
        name2,
        model_id2,
        source,
        path,
        over_max,
        TaskType::Chat,
        None,
    );

    assert!(
        result.is_err(),
        "CustomModel::new() should reject size > 5GB"
    );
}
