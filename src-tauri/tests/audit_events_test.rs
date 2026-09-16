#![allow(clippy::panic)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::assertions_on_constants)]
#![allow(clippy::useless_vec)]
#![allow(clippy::indexing_slicing)]
#![allow(unused_variables)]
#![allow(unused_imports)]

use insta::assert_json_snapshot;
use lattice::audit::event::{AuditAction, AuditEvent, AuditResult};
use std::collections::HashMap;
#[test]
fn test_audit_event_file_indexed_success() {
    let event = AuditEvent::new(AuditAction::FileIndexed, AuditResult::success())
        .with_resource_id("/documents/research/paper.pdf")
        .with_user_id("user123")
        .with_metadata("chunks", "15")
        .with_metadata("size_bytes", "102400")
        .with_metadata("mime_type", "application/pdf");

    assert_json_snapshot!("audit_file_indexed_success", event, {
        ".id" => "[event_id]",
        ".timestamp" => "[timestamp]",
        ".resource_id" => "[resource_path]"
    });
}

#[test]
fn test_audit_event_file_indexed_failure() {
    let event = AuditEvent::new(
        AuditAction::FileIndexed,
        AuditResult::failure("Failed to extract text from PDF"),
    )
    .with_resource_id("/documents/corrupted.pdf")
    .with_metadata("error_code", "EXTRACT_001");

    assert_json_snapshot!("audit_file_indexed_failure", event, {
        ".id" => "[event_id]",
        ".timestamp" => "[timestamp]",
        ".resource_id" => "[resource_path]"
    });
}

#[test]
fn test_audit_event_search_performed() {
    let event = AuditEvent::builder()
        .action(AuditAction::SearchPerformed)
        .result(AuditResult::success())
        .user_id("user456")
        .metadata("query", "machine learning neural networks")
        .metadata("results_count", "25")
        .metadata("search_mode", "hybrid")
        .metadata("duration_ms", "150")
        .build()
        .unwrap();

    assert_json_snapshot!("audit_search_performed", event, {
        ".id" => "[event_id]",
        ".timestamp" => "[timestamp]"
    });
}

#[test]
fn test_audit_event_credential_accessed() {
    let event = AuditEvent::new(AuditAction::CredentialAccessed, AuditResult::success())
        .with_user_id("user789")
        .with_resource_id("api_key_openai")
        .with_metadata("service", "OpenAI API")
        .with_metadata("purpose", "embedding_generation");

    assert_json_snapshot!("audit_credential_accessed", event, {
        ".id" => "[event_id]",
        ".timestamp" => "[timestamp]",
        ".resource_id" => "[credential_id]"
    });
}

#[test]
fn test_audit_event_credential_denied() {
    let event = AuditEvent::new(
        AuditAction::CredentialAccessed,
        AuditResult::denied("Insufficient permissions"),
    )
    .with_user_id("guest_user")
    .with_resource_id("api_key_anthropic")
    .with_metadata("attempted_service", "Claude API");

    assert_json_snapshot!("audit_credential_denied", event, {
        ".id" => "[event_id]",
        ".timestamp" => "[timestamp]",
        ".resource_id" => "[credential_id]"
    });
}

#[test]
fn test_audit_event_config_changed() {
    let mut metadata = HashMap::new();
    metadata.insert("setting_key".to_string(), "embedding_model".to_string());
    metadata.insert("old_value".to_string(), "all-MiniLM-L6-v2".to_string());
    metadata.insert("new_value".to_string(), "bge-large-en-v1.5".to_string());
    metadata.insert("changed_by".to_string(), "user_settings".to_string());

    let event = AuditEvent::new(AuditAction::ConfigChanged, AuditResult::success())
        .with_user_id("user123")
        .with_metadata_map(metadata);

    assert_json_snapshot!("audit_config_changed", event, {
        ".id" => "[event_id]",
        ".timestamp" => "[timestamp]"
    });
}

#[test]
fn test_audit_event_backup_created() {
    let event = AuditEvent::builder()
        .action(AuditAction::BackupCreated)
        .result(AuditResult::success())
        .resource_id("/backups/recall_backup_20240115.db")
        .metadata("size_bytes", "52428800")
        .metadata("compression", "gzip")
        .metadata("duration_ms", "5000")
        .build()
        .unwrap();

    assert_json_snapshot!("audit_backup_created", event, {
        ".id" => "[event_id]",
        ".timestamp" => "[timestamp]",
        ".resource_id" => "[backup_path]"
    });
}

#[test]
fn test_audit_event_system_startup() {
    let event = AuditEvent::new(AuditAction::SystemStartup, AuditResult::success())
        .with_metadata("version", "1.0.0")
        .with_metadata("platform", "linux")
        .with_metadata("rust_version", "1.75.0");

    assert_json_snapshot!("audit_system_startup", event, {
        ".id" => "[event_id]",
        ".timestamp" => "[timestamp]"
    });
}

#[test]
fn test_audit_event_custom_action() {
    let event = AuditEvent::new(
        AuditAction::Custom("Data Migration Completed".to_string()),
        AuditResult::success(),
    )
    .with_metadata("records_migrated", "10000")
    .with_metadata("source_version", "0.9.0")
    .with_metadata("target_version", "1.0.0");

    assert_json_snapshot!("audit_custom_action", event, {
        ".id" => "[event_id]",
        ".timestamp" => "[timestamp]"
    });
}

#[test]
fn test_audit_log_format_multiple_events() {
    let events = vec![
        AuditEvent::new(AuditAction::SystemStartup, AuditResult::success())
            .with_metadata("version", "1.0.0"),
        AuditEvent::new(AuditAction::FileIndexed, AuditResult::success())
            .with_resource_id("doc1.txt")
            .with_metadata("chunks", "5"),
        AuditEvent::new(AuditAction::SearchPerformed, AuditResult::success())
            .with_metadata("query", "test search")
            .with_metadata("results", "10"),
        AuditEvent::new(
            AuditAction::FileIndexed,
            AuditResult::failure("Parse error"),
        )
        .with_resource_id("doc2.pdf"),
    ];

    assert_json_snapshot!("audit_log_sequence", events, {
        ".**.id" => "[event_id]",
        ".**.timestamp" => "[timestamp]",
        ".**.resource_id" => insta::sorted_redaction()
    });
}

#[test]
fn test_audit_result_variants() {
    let results = vec![
        ("success", AuditResult::success()),
        (
            "failure",
            AuditResult::failure("Operation failed due to network error"),
        ),
        (
            "denied",
            AuditResult::denied("Access denied: insufficient permissions"),
        ),
    ];

    let snapshot_data: Vec<_> = results
        .iter()
        .map(|(name, result)| {
            serde_json::json!({
                "type": name,
                "result": result
            })
        })
        .collect();

    assert_json_snapshot!("audit_result_variants", snapshot_data);
}

#[test]
fn test_audit_action_descriptions() {
    let actions = vec![
        AuditAction::FileIndexed,
        AuditAction::FileUpdated,
        AuditAction::FileDeleted,
        AuditAction::SearchPerformed,
        AuditAction::QuestionAnswered,
        AuditAction::CredentialAccessed,
        AuditAction::CredentialStored,
        AuditAction::ConfigChanged,
        AuditAction::BackupCreated,
        AuditAction::SystemStartup,
    ];

    let snapshot_data: Vec<_> = actions
        .iter()
        .map(|action| {
            serde_json::json!({
                "action": format!("{:?}", action),
                "description": action.description()
            })
        })
        .collect();

    assert_json_snapshot!("audit_action_descriptions", snapshot_data);
}

#[test]
fn test_audit_event_minimal() {
    let event = AuditEvent::new(AuditAction::CacheCleared, AuditResult::success());

    assert_json_snapshot!("audit_event_minimal", event, {
        ".id" => "[event_id]",
        ".timestamp" => "[timestamp]"
    });
}

#[test]
fn test_audit_event_with_full_metadata() {
    let mut metadata = HashMap::new();
    metadata.insert("key1".to_string(), "value1".to_string());
    metadata.insert("key2".to_string(), "value2".to_string());
    metadata.insert("key3".to_string(), "value3".to_string());
    metadata.insert("key4".to_string(), "value4".to_string());
    metadata.insert("key5".to_string(), "value5".to_string());

    let event = AuditEvent::builder()
        .action(AuditAction::DataExported)
        .result(AuditResult::success())
        .user_id("admin")
        .resource_id("/exports/data_export_20240115.json")
        .metadata_map(metadata)
        .build()
        .unwrap();

    assert_json_snapshot!("audit_event_full_metadata", event, {
        ".id" => "[event_id]",
        ".timestamp" => "[timestamp]",
        ".resource_id" => "[export_path]"
    });
}

#[test]
fn test_audit_event_serialization_roundtrip() {
    let original = AuditEvent::new(AuditAction::QuestionAnswered, AuditResult::success())
        .with_user_id("user123")
        .with_resource_id("query_456")
        .with_metadata("question", "What is machine learning?")
        .with_metadata("answer_tokens", "150")
        .with_metadata("model", "gpt-3.5-turbo");

    let json = serde_json::to_string(&original).unwrap();
    let deserialized: AuditEvent = serde_json::from_str(&json).unwrap();

    assert_json_snapshot!("audit_serialization_roundtrip", deserialized, {
        ".id" => "[event_id]",
        ".timestamp" => "[timestamp]"
    });
}

#[test]
fn test_audit_security_events() {
    let events = vec![
        AuditEvent::new(AuditAction::AuthAttempt, AuditResult::success())
            .with_user_id("user@example.com")
            .with_metadata("method", "password")
            .with_metadata("ip_address", "192.168.1.100"),
        AuditEvent::new(
            AuditAction::AuthAttempt,
            AuditResult::failure("Invalid credentials"),
        )
        .with_metadata("attempted_user", "unknown@example.com")
        .with_metadata("ip_address", "192.168.1.200"),
        AuditEvent::new(
            AuditAction::CredentialAccessed,
            AuditResult::denied("MFA required"),
        )
        .with_user_id("user@example.com")
        .with_resource_id("sensitive_api_key"),
    ];

    assert_json_snapshot!("audit_security_events", events, {
        ".**.id" => "[event_id]",
        ".**.timestamp" => "[timestamp]"
    });
}

#[test]
fn test_audit_file_operations() {
    let events = vec![
        AuditEvent::new(AuditAction::FileIndexed, AuditResult::success())
            .with_resource_id("/docs/paper.pdf")
            .with_metadata("chunks", "20"),
        AuditEvent::new(AuditAction::FileUpdated, AuditResult::success())
            .with_resource_id("/docs/paper.pdf")
            .with_metadata("chunks_updated", "3"),
        AuditEvent::new(AuditAction::FileDeleted, AuditResult::success())
            .with_resource_id("/docs/old_file.txt")
            .with_metadata("reason", "user_requested"),
    ];

    assert_json_snapshot!("audit_file_operations", events, {
        ".**.id" => "[event_id]",
        ".**.timestamp" => "[timestamp]",
        ".**.resource_id" => insta::sorted_redaction()
    });
}
