//! Smoke tests for HuggingFace plugin DTOs

use lattice::features::huggingface::commands::HfTokenStatus;

#[test]
fn test_hf_token_status_set() {
    let status = HfTokenStatus { is_set: true };

    assert!(status.is_set);
}

#[test]
fn test_hf_token_status_not_set() {
    let status = HfTokenStatus { is_set: false };

    assert!(!status.is_set);
}

#[test]
fn test_hf_token_status_serialization() {
    let status = HfTokenStatus { is_set: true };

    let json = serde_json::to_string(&status).expect("Failed to serialize");
    assert!(json.contains("\"is_set\":true"));
}

#[test]
fn test_hf_token_status_deserialization() {
    let json = r#"{"is_set":false}"#;
    let status: HfTokenStatus = serde_json::from_str(json).expect("Failed to deserialize");

    assert!(!status.is_set);
}
