//! Model and model-file SQL operations against the real migrated schema.
use super::ops;
use chrono::{TimeZone, Utc};
use lattice_core::domain::entities::model::Model;
use lattice_core::domain::entities::model_file::ModelFile;
use lattice_core::domain::value_objects::model_status::{FileStatus, ModelStatus};
use lattice_core::shared::error::AppError;
use sqlx::pool::PoolConnection;
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::Sqlite;

async fn connection() -> PoolConnection<Sqlite> {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(":memory:")
        .await
        .expect("in-memory pool");
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("migrations");
    let mut conn = pool.acquire().await.expect("connection");
    sqlx::query("PRAGMA foreign_keys=ON")
        .execute(&mut *conn)
        .await
        .expect("foreign keys on");
    // These operation tests count their own models. The migration's built-in
    // Ollama entry is covered by schema/downloaded-model tests separately.
    sqlx::query("DELETE FROM models WHERE model_id = '__ollama_server__'")
        .execute(&mut *conn)
        .await
        .expect("remove built-in model from the fixture");
    conn
}

fn model(model_id: &str, name: &str) -> Model {
    let mut model = Model::new(
        uuid::Uuid::new_v4().to_string(),
        name.to_owned(),
        model_id.to_owned(),
        std::path::PathBuf::from(format!("/models/{model_id}/weights.gguf")),
        4_096,
        Some(serde_json::json!({"quantization": "Q4_K_M"})),
    )
    .expect("valid model");
    model.created_at = Utc.with_ymd_and_hms(2026, 1, 2, 3, 4, 5).unwrap();
    model.updated_at = model.created_at;
    // Model::new describes an existing file; this fixture represents a model
    // whose download has not started yet.
    model.downloaded_at = None;
    model
}

fn file(model_id: &str, name: &str, minute: u32) -> ModelFile {
    let at = Utc.with_ymd_and_hms(2026, 1, 2, 3, minute, 0).unwrap();
    ModelFile {
        id: uuid::Uuid::new_v4().to_string(),
        model_id: model_id.to_owned(),
        file_name: name.to_owned(),
        file_path: format!("/models/{model_id}/{name}"),
        relative_path: name.to_owned(),
        size_bytes: 2_048,
        downloaded_bytes: 0,
        checksum_sha256: Some("ab".repeat(32)),
        download_url: String::new(),
        status: FileStatus::Pending,
        created_at: at,
        updated_at: at,
        downloaded_at: None,
    }
}

#[tokio::test]
async fn upsert_round_trips_and_updates_by_model_id_keeping_the_row_id() {
    let mut conn = connection().await;
    let original = model("mistral-7b", "Mistral 7B");
    ops::upsert_model(&mut conn, &original).await.unwrap();

    let found = ops::find_by_model_id(&mut conn, "mistral-7b")
        .await
        .unwrap()
        .expect("stored model");
    assert_eq!(found.id, original.id);
    assert_eq!(found.name, "Mistral 7B");
    assert_eq!(found.base_path, "/models/mistral-7b");
    assert_eq!(found.total_size_bytes, 4_096);
    assert_eq!(found.status, ModelStatus::Pending);
    assert_eq!(found.metadata, original.metadata);
    assert_eq!(found.created_at, original.created_at);
    assert!(!found.is_active_for_chat && !found.is_active_for_embedding);

    let mut renamed = model("mistral-7b", "Mistral 7B Instruct");
    renamed.status = ModelStatus::Completed;
    renamed.use_count = 3;
    ops::upsert_model(&mut conn, &renamed).await.unwrap();
    let updated = ops::find_by_model_id(&mut conn, "mistral-7b")
        .await
        .unwrap()
        .expect("updated model");
    assert_eq!(updated.id, original.id, "the conflict target is model_id");
    assert_eq!(updated.name, "Mistral 7B Instruct");
    assert_eq!(updated.status, ModelStatus::Completed);
    assert_eq!(updated.use_count, 3);
    assert_eq!(ops::list_all(&mut conn).await.unwrap().len(), 1);
    assert!(ops::find_by_model_id(&mut conn, "absent")
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn status_updates_report_missing_models() {
    let mut conn = connection().await;
    ops::upsert_model(&mut conn, &model("phi-3", "Phi 3"))
        .await
        .unwrap();

    ops::update_model_status(&mut conn, "phi-3", ModelStatus::Downloading)
        .await
        .unwrap();
    let downloading = ops::find_by_model_id(&mut conn, "phi-3")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(downloading.status, ModelStatus::Downloading);
    assert!(downloading.downloaded_at.is_none());

    ops::update_status_completed(&mut conn, "phi-3")
        .await
        .unwrap();
    let completed = ops::find_by_model_id(&mut conn, "phi-3")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(completed.status, ModelStatus::Completed);
    assert!(
        completed.downloaded_at.is_some(),
        "CURRENT_TIMESTAMP must parse back"
    );

    ops::update_status_failed(&mut conn, "phi-3").await.unwrap();
    let failed = ops::find_by_model_id(&mut conn, "phi-3")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(failed.status, ModelStatus::Failed);

    for result in [
        ops::update_model_status(&mut conn, "absent", ModelStatus::Completed).await,
        ops::update_status_completed(&mut conn, "absent").await,
        ops::update_status_failed(&mut conn, "absent").await,
    ] {
        assert!(matches!(result, Err(AppError::NotFound(_))), "{result:?}");
    }
}

#[tokio::test]
async fn chat_activation_and_deletion_respect_the_active_flag() {
    let mut conn = connection().await;
    ops::upsert_model(&mut conn, &model("llama-3", "Llama 3"))
        .await
        .unwrap();
    ops::upsert_model(&mut conn, &model("gemma-2", "Gemma 2"))
        .await
        .unwrap();
    ops::upsert_model_file(&mut conn, &file("llama-3", "weights.gguf", 1))
        .await
        .unwrap();
    assert!(ops::get_active_chat_model(&mut conn)
        .await
        .unwrap()
        .is_none());

    ops::activate_chat_model(&mut conn, "llama-3")
        .await
        .unwrap();
    let active = ops::get_active_chat_model(&mut conn)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(active.model_id, "llama-3");
    assert!(active.is_active_for_chat);
    let missing = ops::activate_chat_model(&mut conn, "absent").await;
    assert!(matches!(missing, Err(AppError::NotFound(_))), "{missing:?}");

    ops::delete_if_not_active(&mut conn, "llama-3")
        .await
        .unwrap();
    assert!(ops::find_by_model_id(&mut conn, "llama-3")
        .await
        .unwrap()
        .is_some());

    ops::deactivate_all_chat_models(&mut conn).await.unwrap();
    assert!(ops::get_active_chat_model(&mut conn)
        .await
        .unwrap()
        .is_none());
    ops::activate_chat_model(&mut conn, "gemma-2")
        .await
        .unwrap();
    ops::clear_active_chat_model(&mut conn).await.unwrap();
    assert!(ops::get_active_chat_model(&mut conn)
        .await
        .unwrap()
        .is_none());

    ops::delete_if_not_active(&mut conn, "llama-3")
        .await
        .unwrap();
    assert!(ops::find_by_model_id(&mut conn, "llama-3")
        .await
        .unwrap()
        .is_none());
    assert!(
        ops::get_model_files(&mut conn, "llama-3")
            .await
            .unwrap()
            .is_empty(),
        "files cascade with their model"
    );
    ops::delete_if_not_active(&mut conn, "absent")
        .await
        .unwrap();
    assert_eq!(ops::list_all(&mut conn).await.unwrap().len(), 1);
}

#[tokio::test]
async fn an_active_local_embedding_model_needs_its_artifact_identity() {
    let mut conn = connection().await;
    let mut embedding = model("nomic-embed", "Nomic Embed");
    embedding.is_active_for_embedding = true;
    let refused = ops::upsert_model(&mut conn, &embedding).await;
    assert!(matches!(refused, Err(AppError::Database(_))), "{refused:?}");
    assert!(ops::get_active_embedding_model(&mut conn)
        .await
        .unwrap()
        .is_none());
    ops::clear_active_embedding_model(&mut conn).await.unwrap();
}

#[tokio::test]
async fn model_files_upsert_by_id_and_list_oldest_first() {
    let mut conn = connection().await;
    ops::upsert_model(&mut conn, &model("whisper", "Whisper"))
        .await
        .unwrap();
    let later = file("whisper", "decoder.bin", 30);
    let earlier = file("whisper", "encoder.bin", 10);
    ops::upsert_model_file(&mut conn, &later).await.unwrap();
    ops::upsert_model_file(&mut conn, &earlier).await.unwrap();

    let mut finished = earlier.clone();
    finished.status = FileStatus::Completed;
    finished.size_bytes = 9_000;
    finished.downloaded_at = Some(Utc.with_ymd_and_hms(2026, 1, 2, 4, 0, 0).unwrap());
    ops::upsert_model_file(&mut conn, &finished).await.unwrap();

    let files = ops::get_model_files(&mut conn, "whisper").await.unwrap();
    assert_eq!(
        files
            .iter()
            .map(|f| f.file_name.as_str())
            .collect::<Vec<_>>(),
        vec!["encoder.bin", "decoder.bin"]
    );
    assert_eq!(files[0].id, earlier.id);
    assert_eq!(files[0].status, FileStatus::Completed);
    assert_eq!(files[0].size_bytes, 9_000);
    assert_eq!(files[0].checksum_sha256, earlier.checksum_sha256);
    assert_eq!(files[0].downloaded_at, finished.downloaded_at);
    assert!(ops::get_model_files(&mut conn, "absent")
        .await
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn rows_the_domain_cannot_read_are_skipped_in_lists_and_refused_by_lookup() {
    let mut conn = connection().await;
    ops::upsert_model(&mut conn, &model("qwen", "Qwen"))
        .await
        .unwrap();
    ops::upsert_model(&mut conn, &model("bert", "BERT"))
        .await
        .unwrap();
    ops::upsert_model_file(&mut conn, &file("qwen", "a.gguf", 1))
        .await
        .unwrap();
    ops::upsert_model_file(&mut conn, &file("qwen", "b.gguf", 2))
        .await
        .unwrap();

    // 'verified' passes the table CHECK but is not a ModelStatus.
    sqlx::query("UPDATE models SET status='verified' WHERE model_id='qwen'")
        .execute(&mut *conn)
        .await
        .unwrap();
    sqlx::query("UPDATE model_files SET status='verified' WHERE file_name='b.gguf'")
        .execute(&mut *conn)
        .await
        .unwrap();
    sqlx::query("UPDATE models SET created_at='yesterday' WHERE model_id='bert'")
        .execute(&mut *conn)
        .await
        .unwrap();

    assert!(ops::list_all(&mut conn).await.unwrap().is_empty());
    let unreadable_status = ops::find_by_model_id(&mut conn, "qwen").await;
    assert!(
        matches!(unreadable_status, Err(AppError::InvalidData(_))),
        "{unreadable_status:?}"
    );
    let unreadable_time = ops::find_by_model_id(&mut conn, "bert").await;
    assert!(
        matches!(unreadable_time, Err(AppError::InvalidData(_))),
        "{unreadable_time:?}"
    );
    let files = ops::get_model_files(&mut conn, "qwen").await.unwrap();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].file_name, "a.gguf");
}
