use super::*;
use crate::infrastructure::persistence::database::connection::DatabaseConnection;

async fn insert_model(pool: &sqlx::SqlitePool, model_id: &str) {
    sqlx::query(
        "INSERT INTO models (id, model_name, model_id, model_type, architecture) \
             VALUES (?, ?, ?, 'embedding', 'test')",
    )
    .bind(format!("row-{model_id}"))
    .bind(model_id)
    .bind(model_id)
    .execute(pool)
    .await
    .unwrap();
}

async fn insert_model_file(pool: &sqlx::SqlitePool, model_id: &str, status: &str) {
    sqlx::query(
        "INSERT INTO model_files \
             (id, model_id, file_name, file_path, relative_path, size_bytes, download_url, status) \
             VALUES (?, ?, 'weights.bin', '/tmp/weights.bin', 'weights.bin', 1, \
                     'https://example.com/weights.bin', ?)",
    )
    .bind(format!("file-{model_id}"))
    .bind(model_id)
    .bind(status)
    .execute(pool)
    .await
    .unwrap();
}

async fn insert_session(
    repository: &SqliteDownloadRepository,
    session_id: &str,
    model_id: &str,
    state: DownloadState,
) {
    let mut session = DownloadSession::new(
        session_id.to_string(),
        "https://example.com/weights.bin".to_string(),
        std::path::PathBuf::from(format!("/tmp/{session_id}.bin")),
        Some(1),
        None,
    )
    .unwrap()
    .with_model_metadata(model_id.to_string(), model_id.to_string())
    .with_model_file_name("weights.bin".to_string());
    session.start().unwrap();
    match state {
        DownloadState::Completed => session.complete().unwrap(),
        DownloadState::Failed => session.fail("test failure".to_string()).unwrap(),
        other => panic!("unsupported fixture state: {other:?}"),
    }
    repository.create(&session).await.unwrap();
}

#[tokio::test]
async fn reconciliation_query_returns_only_latest_durable_mismatches() {
    let dir = tempfile::tempdir().unwrap();
    let connection = Arc::new(
        DatabaseConnection::new(dir.path().join("downloads.db"))
            .await
            .unwrap(),
    );
    sqlx::migrate!("./migrations")
        .run(connection.pool())
        .await
        .unwrap();
    let repository = SqliteDownloadRepository::new(connection.clone());

    insert_model(connection.pool(), "failed-mismatch").await;
    insert_model_file(connection.pool(), "failed-mismatch", "downloading").await;
    insert_session(
        &repository,
        "failed-session",
        "failed-mismatch",
        DownloadState::Failed,
    )
    .await;

    insert_model(connection.pool(), "completed-model-mismatch").await;
    insert_model_file(connection.pool(), "completed-model-mismatch", "completed").await;
    insert_session(
        &repository,
        "completed-session",
        "completed-model-mismatch",
        DownloadState::Completed,
    )
    .await;

    insert_model(connection.pool(), "already-consistent").await;
    sqlx::query(
        "UPDATE models SET status = 'completed', storage_path = '/tmp/already-complete' \
             WHERE model_id = ?",
    )
    .bind("already-consistent")
    .execute(connection.pool())
    .await
    .unwrap();
    insert_model_file(connection.pool(), "already-consistent", "completed").await;
    insert_session(
        &repository,
        "consistent-session",
        "already-consistent",
        DownloadState::Completed,
    )
    .await;

    insert_model(connection.pool(), "registry-missing").await;
    sqlx::query("UPDATE models SET status = 'completed' WHERE model_id = ?")
        .bind("registry-missing")
        .execute(connection.pool())
        .await
        .unwrap();
    insert_model_file(connection.pool(), "registry-missing", "completed").await;
    insert_session(
        &repository,
        "registry-missing-session",
        "registry-missing",
        DownloadState::Completed,
    )
    .await;

    let candidates = repository
        .list_terminal_sessions_needing_reconciliation(None, 64)
        .await
        .unwrap();
    let ids: Vec<_> = candidates
        .iter()
        .map(|session| session.id().to_string())
        .collect();
    assert_eq!(
        ids,
        vec![
            "completed-session".to_string(),
            "failed-session".to_string(),
            "registry-missing-session".to_string(),
        ]
    );
}

#[tokio::test]
async fn reconciliation_query_uses_terminal_candidate_and_covering_status_indexes() {
    let dir = tempfile::tempdir().unwrap();
    let connection = DatabaseConnection::new(dir.path().join("downloads.db"))
        .await
        .unwrap();
    sqlx::migrate!("./migrations")
        .run(connection.pool())
        .await
        .unwrap();

    let plan: Vec<(i64, i64, i64, String)> = sqlx::query_as(&format!(
        "EXPLAIN QUERY PLAN {TERMINAL_RECONCILIATION_QUERY}"
    ))
    .bind(Option::<String>::None)
    .bind(Option::<String>::None)
    .bind(64_i64)
    .fetch_all(connection.pool())
    .await
    .unwrap();
    let details: Vec<_> = plan.iter().map(|row| row.3.as_str()).collect();

    assert!(
        details
            .iter()
            .any(|detail| { detail.contains("idx_download_sessions_terminal_reconciliation") }),
        "terminal candidates should use their partial index: {details:?}"
    );
    assert!(
        details.iter().any(|detail| {
            detail.contains("idx_model_files_model_status") && detail.contains("COVERING INDEX")
        }),
        "incomplete-file checks should read status from a covering index: {details:?}"
    );
}
