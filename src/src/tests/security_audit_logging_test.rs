//! Audit contracts exercised through public commands, the logger, and real sinks.
//! These tests use temporary storage and never access the OS credential store.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

use lattice::features::credentials::commands;
use lattice::infrastructure::audit::sinks::{memory::MemoryAuditSink, sqlite::SqliteAuditSink};
use lattice::infrastructure::audit::{
    get_audit_logger, AuditAction, AuditEvent, AuditLogger, AuditResult, AuditSink,
};
use lattice::infrastructure::persistence::database::{initialize_database, DatabaseConnection};
use lattice::interfaces::di::Container;
use std::{collections::HashSet, sync::Arc};

#[tokio::test]
async fn rejected_credential_command_records_failure_without_logging_secret() {
    let directory = tempfile::tempdir().unwrap();
    let database = Arc::new(
        DatabaseConnection::new(directory.path().join("app.db"))
            .await
            .unwrap(),
    );
    initialize_database(database.pool()).await.unwrap();
    let pool = database.pool().clone();
    let container = Container::new(
        pool.clone(),
        database,
        None,
        "http://127.0.0.1:1",
        "test-model",
        directory.path().to_path_buf(),
    )
    .await
    .unwrap();
    let audit_pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
    let sink = SqliteAuditSink::from_pool(audit_pool.clone())
        .await
        .unwrap();
    get_audit_logger().add_sink(Box::new(sink)).await;
    let service = format!("unsupported-{}", uuid::Uuid::new_v4());
    let secret = "test-secret-must-not-appear-in-audit";
    let result = commands::set_api_key_impl(&container, service.clone(), secret.into()).await;
    assert!(result.is_err());
    let events = SqliteAuditSink::from_pool(audit_pool)
        .await
        .unwrap()
        .query(10, 0)
        .await
        .unwrap();
    let event = events
        .iter()
        .find(|event| event.resource_id.as_deref() == Some(&format!("api_key:{service}")))
        .unwrap();
    assert_eq!(event.action, AuditAction::CredentialStored);
    assert!(event.result.is_failure());
    assert_eq!(
        event.metadata.get("operation").map(String::as_str),
        Some("set_api_key")
    );
    assert!(!serde_json::to_string(event).unwrap().contains(secret));
}

#[tokio::test]
async fn sqlite_round_trip_preserves_event_identity_metadata_and_failure() {
    let directory = tempfile::tempdir().unwrap();
    let database = DatabaseConnection::new(directory.path().join("audit.db"))
        .await
        .unwrap();
    let event = AuditEvent::new(
        AuditAction::CredentialAccessed,
        AuditResult::failure("denied"),
    )
    .with_resource_id("api_key:ollama")
    .with_user_id("local")
    .with_metadata("operation", "get_api_key");
    let logger = AuditLogger::new();
    logger
        .add_sink(Box::new(
            SqliteAuditSink::from_pool(database.pool().clone())
                .await
                .unwrap(),
        ))
        .await;
    logger.log(event.clone()).await.unwrap();
    logger.flush().await.unwrap();
    // Read through a separately constructed adapter, not the writer's memory.
    let reader = SqliteAuditSink::from_pool(database.pool().clone())
        .await
        .unwrap();
    let stored = reader.query(10, 0).await.unwrap();
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].id, event.id);
    assert_eq!(stored[0].timestamp, event.timestamp);
    assert_eq!(stored[0].action, event.action);
    assert_eq!(stored[0].result, event.result);
    assert_eq!(stored[0].metadata, event.metadata);
    assert_eq!(stored[0].resource_id, event.resource_id);
    assert_eq!(stored[0].user_id, event.user_id);
}

#[tokio::test]
async fn concurrent_logging_keeps_every_event_once() {
    let logger = Arc::new(AuditLogger::new());
    logger.add_sink(Box::new(MemoryAuditSink::new(100))).await;
    let mut tasks = tokio::task::JoinSet::new();
    for index in 0..40 {
        let logger = logger.clone();
        tasks.spawn(async move {
            logger
                .log(
                    AuditEvent::new(AuditAction::ConfigChanged, AuditResult::success())
                        .with_resource_id(index.to_string()),
                )
                .await
                .unwrap();
        });
    }
    while let Some(result) = tasks.join_next().await {
        result.unwrap();
    }
    let events = logger.query(100, 0).await.unwrap();
    assert_eq!(logger.count().await.unwrap(), 40);
    assert_eq!(
        events
            .iter()
            .map(|event| event.id)
            .collect::<HashSet<_>>()
            .len(),
        40
    );
    assert_eq!(
        events
            .iter()
            .map(|event| event.resource_id.clone())
            .collect::<HashSet<_>>()
            .len(),
        40
    );
}

#[tokio::test]
async fn bounded_sink_evicts_oldest_events_and_pages_remaining_history() {
    let logger = AuditLogger::new();
    logger.add_sink(Box::new(MemoryAuditSink::new(3))).await;
    for index in 0..5 {
        logger
            .log(
                AuditEvent::new(AuditAction::CredentialDeleted, AuditResult::success())
                    .with_resource_id(index.to_string()),
            )
            .await
            .unwrap();
    }
    assert_eq!(logger.count().await.unwrap(), 3);
    let events = logger.query(10, 0).await.unwrap();
    let resources: HashSet<_> = events
        .iter()
        .map(|event| event.resource_id.clone().unwrap())
        .collect();
    assert_eq!(
        resources,
        HashSet::from(["2".into(), "3".into(), "4".into()])
    );
    let page = logger.query(1, 1).await.unwrap();
    assert_eq!(page.len(), 1);
    assert_eq!(page[0].id, events[1].id);
}

#[tokio::test]
async fn disabled_logger_does_not_write_and_reenable_resumes_writes() {
    let logger = AuditLogger::new();
    logger.add_sink(Box::new(MemoryAuditSink::new(10))).await;
    logger.disable().await;
    logger
        .log(AuditEvent::new(
            AuditAction::ConfigChanged,
            AuditResult::success(),
        ))
        .await
        .unwrap();
    assert_eq!(logger.count().await.unwrap(), 0);
    logger.enable().await;
    logger
        .log(AuditEvent::new(
            AuditAction::ConfigChanged,
            AuditResult::success(),
        ))
        .await
        .unwrap();
    assert_eq!(logger.count().await.unwrap(), 1);
}
