use super::DownloadSaga;
use chrono::Utc;
use lattice_core::domain::entities::model_file::ModelFile;
use lattice_core::domain::models::downloaded::ModelLocation;
use lattice_core::domain::value_objects::model_status::FileStatus;
use std::path::PathBuf;

fn make_file(file_name: &str, file_path: &str) -> ModelFile {
    ModelFile {
        id: "id".to_string(),
        model_id: "model".to_string(),
        file_name: file_name.to_string(),
        file_path: file_path.to_string(),
        relative_path: file_name.to_string(),
        size_bytes: 0,
        downloaded_bytes: 0,
        checksum_sha256: None,
        download_url: "https://example.com".to_string(),
        status: FileStatus::Completed,
        created_at: Utc::now(),
        updated_at: Utc::now(),
        downloaded_at: None,
    }
}

#[test]
fn select_primary_prefers_model_onnx() {
    let files = vec![
        make_file("config.json", "/tmp/config.json"),
        make_file("model.onnx", "/tmp/model.onnx"),
        make_file("tokenizer.json", "/tmp/tokenizer.json"),
    ];

    let selected = DownloadSaga::select_primary_model_file(&files).expect("file selected");
    assert_eq!(selected.file_name, "model.onnx");
}

#[test]
fn select_primary_prefers_onnx_in_subdirectory() {
    let files = vec![
        make_file("config.json", "/tmp/config.json"),
        make_file("onnx/model.onnx", "/tmp/onnx/model.onnx"),
        make_file("tokenizer.json", "/tmp/tokenizer.json"),
    ];

    let selected = DownloadSaga::select_primary_model_file(&files).expect("file selected");
    assert_eq!(selected.file_name, "onnx/model.onnx");
}

#[test]
fn select_primary_falls_back_to_gguf() {
    let files = vec![
        make_file("config.json", "/tmp/config.json"),
        make_file("phi.gguf", "/tmp/phi.gguf"),
    ];

    let selected = DownloadSaga::select_primary_model_file(&files).expect("file selected");
    assert_eq!(selected.file_name, "phi.gguf");
}

#[test]
fn select_primary_picks_config_for_safetensors_layout() {
    // No GGUF/ONNX present; HF safetensors LLM layout. config.json
    // should win over a random shard so the downstream
    // `primary_path_for_downloaded_model` helper can derive the
    // directory path.
    let files = vec![
        make_file(
            "model-00001-of-00004.safetensors",
            "/tmp/m/model-00001-of-00004.safetensors",
        ),
        make_file(
            "model-00002-of-00004.safetensors",
            "/tmp/m/model-00002-of-00004.safetensors",
        ),
        make_file("config.json", "/tmp/m/config.json"),
        make_file("tokenizer.json", "/tmp/m/tokenizer.json"),
    ];

    let selected = DownloadSaga::select_primary_model_file(&files).expect("file selected");
    assert_eq!(selected.file_name, "config.json");
}

#[test]
fn model_location_returns_directory_for_config_json() {
    let primary = make_file("config.json", "/tmp/gemma-2-9b-it/config.json");
    let location = DownloadSaga::model_location_for_download(&primary).unwrap();
    assert_eq!(
        location,
        ModelLocation::LocalDirectory {
            path: PathBuf::from("/tmp/gemma-2-9b-it")
        }
    );
}

#[tokio::test]
async fn first_embedding_download_is_activated_with_its_identity() {
    use crate::features::download::artifact_identity;
    use crate::features::download::downloaded_model_repository::DownloadedModelRepository;
    use crate::features::download::events::model_download_events::FileDownloadCompletedEvent;
    use crate::infrastructure::events::event_bus::EventBus;
    use crate::infrastructure::persistence::repositories::model_file::SqliteModelFileRepository;
    use crate::infrastructure::persistence::repositories::unit_of_work::SqliteUnitOfWorkFactory;
    use std::sync::Arc;

    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect(":memory:")
        .await
        .expect("create in-memory pool");
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("run migrations");

    let dir = tempfile::tempdir().expect("create model directory");
    let files = [
        ("config.json", &b"{}"[..]),
        ("tokenizer.json", &b"{}"[..]),
        ("model.safetensors", &b"weights"[..]),
    ];
    sqlx::query(
        "INSERT INTO models (id, model_name, model_id, base_path, model_type, architecture, status)
             VALUES ('row-1', 'test-embed-model', 'test-embed-model', ?1, 'embedding', 'bert', 'downloading')",
    )
    .bind(dir.path().to_string_lossy().into_owned())
    .execute(&pool)
    .await
    .expect("insert model row");
    for (name, contents) in files {
        let path = dir.path().join(name);
        std::fs::write(&path, contents).expect("write model file");
        // config.json is the file whose completion finishes the download.
        let status = if name == "config.json" {
            "downloading"
        } else {
            "completed"
        };
        sqlx::query(
            "INSERT INTO model_files
                     (id, model_id, file_name, file_path, relative_path, size_bytes, download_url, status)
                 VALUES (?1, 'test-embed-model', ?2, ?3, ?2, ?4, 'https://example.com', ?5)",
        )
        .bind(format!("file-{name}"))
        .bind(name)
        .bind(path.to_string_lossy().into_owned())
        .bind(contents.len() as i64)
        .bind(status)
        .execute(&pool)
        .await
        .expect("insert model file row");
    }

    let downloaded_models = Arc::new(DownloadedModelRepository::new(pool.clone()));
    let saga = DownloadSaga::new(
        Arc::new(EventBus::new()),
        Arc::new(SqliteModelFileRepository::new(pool.clone())),
        Arc::new(SqliteUnitOfWorkFactory::new(pool.clone())),
        Arc::clone(&downloaded_models),
    );

    saga.handle_file_completed(FileDownloadCompletedEvent {
        model_id: "test-embed-model".to_string(),
        file_id: "file-config.json".to_string(),
        file_name: "config.json".to_string(),
        file_size: 2,
        timestamp: Utc::now(),
    })
    .await
    .expect("handle final file completion");

    let active = downloaded_models
        .get_active_embedding_model()
        .await
        .expect("read active embedding model")
        .expect("the empty embedding slot is filled by the first download");
    assert_eq!(active.model_id(), "test-embed-model");
    assert_eq!(
        active.embedding_artifact_identity(),
        Some(&artifact_identity::compute(dir.path()).expect("compute identity")),
        "a local embedding model is activated with the identity of its files"
    );
}

/// A model row with one file, in the given states, and a saga over it.
async fn saga_with_one_file(
    model_status: &str,
    file_status: &str,
) -> (DownloadSaga, sqlx::SqlitePool) {
    use crate::features::download::downloaded_model_repository::DownloadedModelRepository;
    use crate::infrastructure::events::event_bus::EventBus;
    use crate::infrastructure::persistence::repositories::model_file::SqliteModelFileRepository;
    use crate::infrastructure::persistence::repositories::unit_of_work::SqliteUnitOfWorkFactory;
    use std::sync::Arc;

    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect(":memory:")
        .await
        .expect("create in-memory pool");
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("run migrations");
    sqlx::query(
        "INSERT INTO models (id, model_name, model_id, base_path, model_type, architecture, status)
             VALUES ('row-1', 'embed', 'embed', '/tmp/embed', 'embedding', 'bert', ?1)",
    )
    .bind(model_status)
    .execute(&pool)
    .await
    .expect("insert model row");
    sqlx::query(
        "INSERT INTO model_files
                 (id, model_id, file_name, file_path, relative_path, size_bytes, download_url, status)
             VALUES ('file-1', 'embed', 'model.safetensors', '/tmp/embed/model.safetensors',
                     'model.safetensors', 7, 'https://example.com', ?1)",
    )
    .bind(file_status)
    .execute(&pool)
    .await
    .expect("insert model file row");

    let saga = DownloadSaga::new(
        Arc::new(EventBus::new()),
        Arc::new(SqliteModelFileRepository::new(pool.clone())),
        Arc::new(SqliteUnitOfWorkFactory::new(pool.clone())),
        Arc::new(DownloadedModelRepository::new(pool.clone())),
    );
    (saga, pool)
}

async fn statuses(pool: &sqlx::SqlitePool) -> (String, String) {
    let model: String = sqlx::query_scalar("SELECT status FROM models WHERE id = 'row-1'")
        .fetch_one(pool)
        .await
        .expect("read model status");
    let file: String = sqlx::query_scalar("SELECT status FROM model_files WHERE id = 'file-1'")
        .fetch_one(pool)
        .await
        .expect("read file status");
    (model, file)
}

fn cancelled(
    file_name: &str,
) -> crate::features::download::events::model_download_events::FileDownloadFailedEvent {
    crate::features::download::events::model_download_events::FileDownloadFailedEvent {
        model_id: "embed".to_string(),
        file_id: "some-other-session".to_string(),
        file_name: file_name.to_string(),
        error: "Download cancelled".to_string(),
        timestamp: Utc::now(),
    }
}

/// Cancelling a row left over from an interrupted attempt reports a failure
/// for a file that a later attempt already finished. It used to mark the
/// finished, active model as failed, which removed it from the downloaded
/// list.
#[tokio::test]
async fn a_failure_for_a_file_that_already_completed_changes_nothing() {
    let (saga, pool) = saga_with_one_file("completed", "completed").await;

    saga.handle_file_failed(cancelled("model.safetensors"))
        .await
        .expect("handle the stray failure");

    assert_eq!(
        statuses(&pool).await,
        ("completed".to_string(), "completed".to_string())
    );
}

#[tokio::test]
async fn a_failure_for_a_file_still_downloading_fails_the_model() {
    let (saga, pool) = saga_with_one_file("downloading", "downloading").await;

    saga.handle_file_failed(cancelled("model.safetensors"))
        .await
        .expect("handle the failure");

    assert_eq!(
        statuses(&pool).await,
        ("failed".to_string(), "failed".to_string())
    );
}

#[test]
fn model_location_returns_local_file_for_gguf() {
    // GGUF stays a single-file path under LocalFile.
    let primary = make_file("phi.gguf", "/tmp/phi/phi.gguf");
    let location = DownloadSaga::model_location_for_download(&primary).unwrap();
    assert_eq!(
        location,
        ModelLocation::LocalFile {
            path: PathBuf::from("/tmp/phi/phi.gguf")
        }
    );
}
