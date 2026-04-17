//! Smoke tests for Health plugin DTOs

use vault::features::health::commands::HealthStatus;

#[test]
fn test_health_status_creation() {
    let status = HealthStatus {
        status: "healthy".to_string(),
        database: true,
        embedding_model: true,
        llm: true,
        timestamp: "2024-01-01T00:00:00Z".to_string(),
    };

    assert_eq!(status.status, "healthy");
    assert!(status.database);
    assert!(status.embedding_model);
    assert!(status.llm);
}

#[test]
fn test_health_status_unhealthy() {
    let status = HealthStatus {
        status: "unhealthy".to_string(),
        database: false,
        embedding_model: false,
        llm: false,
        timestamp: "2024-01-01T00:00:00Z".to_string(),
    };

    assert_eq!(status.status, "unhealthy");
    assert!(!status.database);
    assert!(!status.embedding_model);
    assert!(!status.llm);
}

#[test]
fn test_health_status_serialization() {
    let status = HealthStatus {
        status: "healthy".to_string(),
        database: true,
        embedding_model: true,
        llm: true,
        timestamp: "2024-01-01T00:00:00Z".to_string(),
    };

    let json = serde_json::to_string(&status).expect("Failed to serialize");
    assert!(json.contains("healthy"));
    assert!(json.contains("true"));
}
