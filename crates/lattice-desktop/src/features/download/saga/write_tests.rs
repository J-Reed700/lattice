use super::DownloadSaga;
use crate::features::download::events::model_download_events::{
    FileDownloadCompletedEvent, FileDownloadFailedEvent,
};
use crate::infrastructure::events::event_bus::EventBus;
use crate::infrastructure::persistence::repositories::downloaded_models::DownloadedModelRepository;
use crate::infrastructure::persistence::repositories::model_file::SqliteModelFileRepository;
use crate::infrastructure::persistence::repositories::unit_of_work::SqliteUnitOfWorkFactory;
use chrono::Utc;
use std::sync::Arc;

async fn saga_with_downloading_file() -> (DownloadSaga, sqlx::SqlitePool) {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect(":memory:")
        .await
        .unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    sqlx::query(
        "INSERT INTO models (id, model_name, model_id, base_path, model_type, architecture, status)
         VALUES ('row-1', 'embed', 'embed', '/tmp/embed', 'embedding', 'bert', 'downloading')",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO model_files
             (id, model_id, file_name, file_path, relative_path, size_bytes, download_url, status)
         VALUES ('file-1', 'embed', 'model.safetensors', '/tmp/embed/model.safetensors',
                 'model.safetensors', 7, 'https://example.com', 'downloading')",
    )
    .execute(&pool)
    .await
    .unwrap();
    let saga = DownloadSaga::new(
        Arc::new(EventBus::new()),
        Arc::new(SqliteModelFileRepository::new(pool.clone())),
        Arc::new(SqliteUnitOfWorkFactory::new(pool.clone())),
        Arc::new(DownloadedModelRepository::new(pool.clone())),
    );
    (saga, pool)
}

async fn statuses(pool: &sqlx::SqlitePool) -> (String, String) {
    let model = sqlx::query_scalar("SELECT status FROM models WHERE id = 'row-1'")
        .fetch_one(pool)
        .await
        .unwrap();
    let file = sqlx::query_scalar("SELECT status FROM model_files WHERE id = 'file-1'")
        .fetch_one(pool)
        .await
        .unwrap();
    (model, file)
}

#[tokio::test]
async fn a_completion_whose_record_cannot_be_saved_leaves_the_download_unfinished() {
    let (saga, pool) = saga_with_downloading_file().await;
    sqlx::raw_sql(
        "CREATE TRIGGER refuse_record BEFORE UPDATE ON models
         WHEN NEW.storage_path IS NOT OLD.storage_path
         BEGIN SELECT RAISE(ABORT, 'injected'); END;",
    )
    .execute(&pool)
    .await
    .unwrap();

    let result = saga
        .handle_file_completed(FileDownloadCompletedEvent {
            model_id: "embed".to_string(),
            file_id: "file-1".to_string(),
            file_name: "model.safetensors".to_string(),
            file_size: 7,
            timestamp: Utc::now(),
        })
        .await;

    assert!(result.is_err());
    assert_eq!(
        statuses(&pool).await,
        ("downloading".to_string(), "downloading".to_string()),
        "neither the file nor the model may be marked completed without the record"
    );
}

#[tokio::test]
async fn a_failure_that_cannot_fail_the_model_leaves_its_file_unchanged() {
    let (saga, pool) = saga_with_downloading_file().await;
    sqlx::raw_sql(
        "CREATE TRIGGER refuse_failed BEFORE UPDATE OF status ON models
         WHEN NEW.status = 'failed'
         BEGIN SELECT RAISE(ABORT, 'injected'); END;",
    )
    .execute(&pool)
    .await
    .unwrap();

    let result = saga
        .handle_file_failed(FileDownloadFailedEvent {
            model_id: "embed".to_string(),
            file_id: "file-1".to_string(),
            file_name: "model.safetensors".to_string(),
            error: "network".to_string(),
            timestamp: Utc::now(),
        })
        .await;

    assert!(result.is_err());
    assert_eq!(
        statuses(&pool).await,
        ("downloading".to_string(), "downloading".to_string())
    );
}
