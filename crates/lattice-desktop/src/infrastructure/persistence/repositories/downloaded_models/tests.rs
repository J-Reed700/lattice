//! Tests for model rows, activation and deletion against an in-memory SQLite.

use super::rows::{directory_has_loadable_weights, parse_optional_db_timestamp};
use super::*;
use lattice_core::domain::value_objects::ArtifactIdentity;
use sqlx::sqlite::SqlitePoolOptions;

#[test]
fn parse_optional_timestamp_accepts_sqlite_format() {
    let parsed = parse_optional_db_timestamp(Some("2026-03-04 19:53:19"))
        .expect("expected parse to succeed");
    assert!(parsed.is_some());
}

#[test]
fn parse_optional_timestamp_treats_empty_as_none() {
    let parsed = parse_optional_db_timestamp(Some("   ")).expect("expected parse to succeed");
    assert!(parsed.is_none());
}

#[test]
fn parse_optional_timestamp_treats_invalid_as_none() {
    let parsed = parse_optional_db_timestamp(Some("not-a-timestamp")).expect("expected no error");
    assert!(parsed.is_none());
}

async fn setup_repo() -> DownloadedModelRepository {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(":memory:")
        .await
        .expect("create in-memory pool");
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("run migrations");
    DownloadedModelRepository::new(pool)
}

fn make_local_chat_model(model_id: &str) -> DownloadedModel {
    DownloadedModel::new(
        uuid::Uuid::new_v4().to_string(),
        format!("Test Model {}", model_id),
        model_id.to_string(),
        ModelLocation::LocalFile {
            path: std::path::PathBuf::from(format!("/tmp/{}.gguf", model_id)),
        },
        1024,
        "llama".to_string(),
        None,
    )
    .expect("create local chat model")
}

fn make_local_embedding_model(model_id: &str) -> DownloadedModel {
    DownloadedModel::new(
        uuid::Uuid::new_v4().to_string(),
        format!("Embed {}", model_id),
        model_id.to_string(),
        ModelLocation::LocalDirectory {
            path: std::path::PathBuf::from(format!("/tmp/{}", model_id)),
        },
        512,
        "bge".to_string(),
        None,
    )
    .expect("create local embedding model")
}

fn identity(byte: u8) -> ArtifactIdentity {
    ArtifactIdentity::from_digest(&[byte; 32])
}

/// A local embedding model whose directory passes `is_downloaded`, so
/// `get_active_embedding_model` returns it.
fn directory_embedding_model(model_id: &str) -> (tempfile::TempDir, DownloadedModel) {
    let directory = tempfile::tempdir().expect("create model directory");
    std::fs::write(directory.path().join("model.safetensors"), b"weights")
        .expect("write model weights");
    let model = DownloadedModel::new(
        uuid::Uuid::new_v4().to_string(),
        format!("Embed {}", model_id),
        model_id.to_string(),
        ModelLocation::LocalDirectory {
            path: directory.path().to_path_buf(),
        },
        7,
        "bge".to_string(),
        None,
    )
    .expect("create directory embedding model");
    (directory, model)
}

async fn stored_embedding_state(
    repo: &DownloadedModelRepository,
    model_id: &str,
) -> (i64, Option<String>) {
    sqlx::query_as(
        "SELECT is_active_for_embedding, embedding_artifact_identity FROM models WHERE model_id = ?1",
    )
    .bind(model_id)
    .fetch_one(&repo.pool)
    .await
    .expect("read embedding state")
}

#[tokio::test]
async fn activating_local_model_without_identity_is_invalid_state() {
    let repo = setup_repo().await;
    repo.set_active_embedding_model("__ollama_server__", None)
        .await
        .expect("activate remote model");
    let model = make_local_embedding_model("no-identity");
    repo.save(&model).await.expect("save embedding model");

    let err = repo
        .set_active_embedding_model("no-identity", None)
        .await
        .expect_err("a local model needs an identity");
    assert!(matches!(err, AppError::InvalidState(_)), "{err:?}");

    assert_eq!(
        stored_embedding_state(&repo, "no-identity").await,
        (0, None)
    );
    assert_eq!(
        stored_embedding_state(&repo, "__ollama_server__").await.0,
        1,
        "a refused activation must not deactivate the current model"
    );
}

/// The rule holds for every write, not just the guarded UPDATE.
#[tokio::test]
async fn schema_rejects_an_active_local_model_without_identity() {
    let repo = setup_repo().await;
    let model = make_local_embedding_model("raw-write");
    repo.save(&model).await.expect("save embedding model");

    let result =
        sqlx::query("UPDATE models SET is_active_for_embedding = 1 WHERE model_id = 'raw-write'")
            .execute(&repo.pool)
            .await;
    assert!(
        result.is_err(),
        "CHECK accepted an active local model without identity"
    );

    let identity = ArtifactIdentity::from_digest(&[7; 32]);
    repo.set_active_embedding_model("raw-write", Some(&identity))
        .await
        .expect("activate with identity");
    let result = sqlx::query(
        "UPDATE models SET embedding_artifact_identity = NULL WHERE model_id = 'raw-write'",
    )
    .execute(&repo.pool)
    .await;
    assert!(
        result.is_err(),
        "CHECK let an active local model drop its identity"
    );
}

#[tokio::test]
async fn activating_local_model_records_identity_and_reads_it_back() {
    let repo = setup_repo().await;
    let (_directory, model) = directory_embedding_model("with-identity");
    repo.save(&model).await.expect("save embedding model");

    repo.set_active_embedding_model("with-identity", Some(&identity(1)))
        .await
        .expect("activate with identity");

    let active = repo
        .get_active_embedding_model()
        .await
        .expect("read active model")
        .expect("active embedding model");
    assert_eq!(active.model_id(), "with-identity");
    assert!(active.is_active_for_embedding());
    assert_eq!(active.embedding_artifact_identity(), Some(&identity(1)));
}

#[tokio::test]
async fn remote_model_activates_without_identity() {
    let repo = setup_repo().await;
    repo.set_active_embedding_model("__ollama_server__", None)
        .await
        .expect("remote models carry no identity");

    let active = repo
        .get_active_embedding_model()
        .await
        .expect("read active model")
        .expect("active embedding model");
    assert_eq!(active.location(), &ModelLocation::RemoteOllama);
    assert_eq!(active.embedding_artifact_identity(), None);
}

#[tokio::test]
async fn reactivating_without_identity_keeps_the_stored_one() {
    let repo = setup_repo().await;
    let model = make_local_embedding_model("reactivated");
    repo.save(&model).await.expect("save embedding model");
    repo.set_active_embedding_model("reactivated", Some(&identity(2)))
        .await
        .expect("first activation");
    repo.set_active_embedding_model("__ollama_server__", None)
        .await
        .expect("switch away");

    repo.set_active_embedding_model("reactivated", None)
        .await
        .expect("the stored identity satisfies the guard");

    assert_eq!(
        stored_embedding_state(&repo, "reactivated").await,
        (1, Some(identity(2).to_string()))
    );
}

#[tokio::test]
async fn clearing_active_embedding_model_keeps_identity() {
    let repo = setup_repo().await;
    let model = make_local_embedding_model("cleared");
    repo.save(&model).await.expect("save embedding model");
    repo.set_active_embedding_model("cleared", Some(&identity(3)))
        .await
        .expect("activate");

    repo.clear_active_embedding_model()
        .await
        .expect("clear active embedding model");

    assert_eq!(
        stored_embedding_state(&repo, "cleared").await,
        (0, Some(identity(3).to_string()))
    );
    let loaded = repo
        .find_by_model_id("cleared")
        .await
        .expect("find model")
        .expect("model exists");
    assert_eq!(loaded.embedding_artifact_identity(), Some(&identity(3)));
}

#[tokio::test]
async fn activating_unknown_model_is_not_found() {
    let repo = setup_repo().await;
    for candidate in [None, Some(identity(4))] {
        let err = repo
            .set_active_embedding_model("no-such-model", candidate.as_ref())
            .await
            .expect_err("unknown model");
        assert!(matches!(err, AppError::NotFound(_)), "{err:?}");
    }
}

#[tokio::test]
async fn schema_rejects_malformed_identity() {
    let repo = setup_repo().await;
    let hex = "ab".repeat(32);
    for malformed in [
        format!("sha256:{}", hex.to_uppercase()),
        format!("sha256:{}", &hex[..62]),
        format!("sha256:{hex}0"),
        hex.clone(),
        format!("md5:{hex}"),
        format!("sha256:{hex}\0x"),
    ] {
        let result = sqlx::query(
            "UPDATE models SET embedding_artifact_identity = ?1 WHERE model_id = '__ollama_server__'",
        )
        .bind(&malformed)
        .execute(&repo.pool)
        .await;
        assert!(result.is_err(), "CHECK accepted {malformed:?}");
    }

    let blob = sqlx::query(
        "UPDATE models SET embedding_artifact_identity = ?1 WHERE model_id = '__ollama_server__'",
    )
    .bind(format!("sha256:{hex}").into_bytes())
    .execute(&repo.pool)
    .await;
    assert!(blob.is_err(), "CHECK accepted a blob");

    sqlx::query(
        "UPDATE models SET embedding_artifact_identity = ?1 WHERE model_id = '__ollama_server__'",
    )
    .bind(format!("sha256:{hex}"))
    .execute(&repo.pool)
    .await
    .expect("CHECK accepts a well-formed identity");
}

/// The CHECK constraint admits exactly what `ArtifactIdentity::parse`
/// accepts, so a corrupt value cannot be stored; the mapping is exercised
/// on a row read back and then corrupted in memory instead.
#[tokio::test]
async fn malformed_identity_fails_the_read() {
    let repo = setup_repo().await;
    let query = build_model_select_query("WHERE m.model_id = ?1");
    let mut record = sqlx::query_as::<_, DownloadedModelRecord>(&query)
        .bind("__ollama_server__")
        .fetch_one(&repo.pool)
        .await
        .expect("read synthetic row");
    record.embedding_artifact_identity = Some(format!("sha256:{}", "AB".repeat(32)));

    let err = DownloadedModel::try_from(record).expect_err("malformed identity");
    assert!(matches!(err, AppError::InvalidData(_)), "{err:?}");
}

#[tokio::test]
async fn saving_a_fresh_model_resets_the_stored_identity() {
    let repo = setup_repo().await;
    let model = make_local_embedding_model("redownloaded");
    repo.save(&model).await.expect("save embedding model");
    repo.set_active_embedding_model("redownloaded", Some(&identity(5)))
        .await
        .expect("activate");

    // Saving the row as read keeps what it carries.
    let loaded = repo
        .find_by_model_id("redownloaded")
        .await
        .expect("find model")
        .expect("model exists");
    repo.save(&loaded).await.expect("save loaded model");
    assert_eq!(
        stored_embedding_state(&repo, "redownloaded").await,
        (1, Some(identity(5).to_string()))
    );

    // A re-download saves a freshly constructed model over the row.
    let fresh = make_local_embedding_model("redownloaded");
    repo.save(&fresh).await.expect("save fresh model");
    assert_eq!(
        stored_embedding_state(&repo, "redownloaded").await,
        (0, None)
    );
}

#[tokio::test]
async fn synthetic_ollama_row_loads_via_find_by_model_id() {
    let repo = setup_repo().await;
    let model = repo
        .find_by_model_id("__ollama_server__")
        .await
        .expect("find by model id")
        .expect("synthetic ollama row missing after migrations");

    assert_eq!(model.location(), &ModelLocation::RemoteOllama);
    assert_eq!(model.file_size_bytes(), 0);
    assert_eq!(model.model_id(), "__ollama_server__");
}

#[tokio::test]
async fn set_and_get_active_utility_model_roundtrip() {
    let repo = setup_repo().await;

    // Activate the synthetic ollama row for utility (no FS check).
    repo.set_active_utility_model("__ollama_server__")
        .await
        .expect("set active utility");

    let active = repo
        .get_active_utility_model()
        .await
        .expect("get active utility");
    let active = active.expect("expected active utility model");
    assert_eq!(active.model_id(), "__ollama_server__");
    assert!(active.is_active_for_utility());

    repo.clear_active_utility_model()
        .await
        .expect("clear active utility");
    let after_clear = repo
        .get_active_utility_model()
        .await
        .expect("get active utility post-clear");
    assert!(after_clear.is_none());
}

#[tokio::test]
async fn get_active_chat_model_returns_ollama_row_without_filesystem_check() {
    let repo = setup_repo().await;

    repo.set_active_chat_model("__ollama_server__")
        .await
        .expect("set active chat to ollama");

    let active = repo
        .get_active_chat_model()
        .await
        .expect("get active chat model");
    let active = active.expect("expected ollama row to be returned despite no local file");
    assert_eq!(active.location(), &ModelLocation::RemoteOllama);
    assert_eq!(active.model_id(), "__ollama_server__");
}

#[tokio::test]
async fn has_any_embedding_model_false_on_empty_then_true_after_save() {
    let repo = setup_repo().await;

    // Synthetic ollama row is model_type='chat', so initially no embedding rows.
    let before = repo
        .has_any_embedding_model()
        .await
        .expect("check embedding presence pre-save");
    assert!(!before, "no embedding model expected before save");

    let embedding = make_local_embedding_model("test-embed-1");
    repo.save(&embedding).await.expect("save embedding model");

    let after = repo
        .has_any_embedding_model()
        .await
        .expect("check embedding presence post-save");
    assert!(after, "embedding model should be detected after save");
}

#[tokio::test]
async fn save_and_load_local_model_preserves_location() {
    let repo = setup_repo().await;
    let model = make_local_chat_model("local-chat-1");
    repo.save(&model).await.expect("save local chat model");

    let loaded = repo
        .find_by_model_id("local-chat-1")
        .await
        .expect("find local model")
        .expect("local model missing after save");
    assert!(loaded.location().is_local());
    assert!(matches!(loaded.location(), ModelLocation::LocalFile { .. }));
    assert!(!loaded.is_active_for_utility());
}

#[tokio::test]
async fn directory_model_fallback_tracks_required_weights_file() {
    let repo = setup_repo().await;
    let directory = tempfile::tempdir().expect("create model directory");
    std::fs::write(directory.path().join("model.safetensors"), b"weights")
        .expect("write model weights");

    let model = DownloadedModel::new(
        uuid::Uuid::new_v4().to_string(),
        "Directory Model".to_string(),
        "directory-model".to_string(),
        ModelLocation::LocalDirectory {
            path: directory.path().to_path_buf(),
        },
        7,
        "bge".to_string(),
        None,
    )
    .expect("create directory model");

    repo.save(&model).await.expect("save directory model");
    assert!(repo
        .is_downloaded("directory-model")
        .await
        .expect("check directory model"));

    let file_path: String =
        sqlx::query_scalar("SELECT file_path FROM model_files WHERE model_id = 'directory-model'")
            .fetch_one(&repo.pool)
            .await
            .expect("load fallback file path");
    assert!(file_path.ends_with("model.safetensors"));
}

#[test]
fn only_a_safetensors_shard_counts_as_directory_weights() {
    let directory = tempfile::tempdir().expect("create model directory");
    assert!(
        !directory_has_loadable_weights(directory.path()),
        "an empty directory holds no weights"
    );

    std::fs::write(directory.path().join("config.json"), b"{}").expect("write config");
    std::fs::write(directory.path().join("pytorch_model.bin"), b"pickle")
        .expect("write pickle weights");
    assert!(
        !directory_has_loadable_weights(directory.path()),
        "nothing loads a PyTorch pickle"
    );

    std::fs::write(directory.path().join("model.safetensors"), b"tensors").expect("write weights");
    assert!(directory_has_loadable_weights(directory.path()));
    assert_eq!(
        fallback_artifact_path(&ModelLocation::LocalDirectory {
            path: directory.path().to_path_buf(),
        }),
        Some(directory.path().join("model.safetensors")),
    );
}

#[tokio::test]
async fn downloaded_inventory_tracks_files_on_disk_without_deleting_records() {
    let repo = setup_repo().await;
    let directory = tempfile::tempdir().expect("create model directory");
    let weights = directory.path().join("model.safetensors");
    std::fs::write(&weights, b"weights").expect("write weights");
    let model = DownloadedModel::new(
        uuid::Uuid::new_v4().to_string(),
        "Qwen3 Embedding".into(),
        "inventory-embedding".into(),
        ModelLocation::LocalDirectory {
            path: directory.path().to_path_buf(),
        },
        7,
        "bert".into(),
        None,
    )
    .expect("create model");
    repo.save(&model).await.expect("save model");
    repo.set_active_embedding_model(model.model_id(), Some(&identity(0)))
        .await
        .expect("select model");

    assert!(repo
        .list_all()
        .await
        .expect("list downloads")
        .iter()
        .any(|m| m.model_id() == model.model_id()));
    assert!(repo
        .get_active_embedding_model()
        .await
        .expect("active model")
        .is_some());

    // A truncated artifact is not a usable download, even with a completed row.
    std::fs::write(&weights, b"part").expect("truncate weights");
    assert!(!repo
        .list_all()
        .await
        .expect("list downloads")
        .iter()
        .any(|m| m.model_id() == model.model_id()));
    std::fs::remove_file(&weights).expect("remove weights");
    let listed = repo.list_all().await.expect("list downloads");
    assert!(!listed.iter().any(|m| m.model_id() == model.model_id()));
    assert!(listed.iter().any(|m| m.model_id() == "__ollama_server__"));
    assert!(repo
        .get_active_embedding_model()
        .await
        .expect("active model")
        .is_none());
    assert!(repo
        .find_by_model_id(model.model_id())
        .await
        .expect("retained record")
        .is_some());

    std::fs::write(&weights, b"weights").expect("restore weights");
    assert!(repo
        .list_all()
        .await
        .expect("list downloads")
        .iter()
        .any(|m| m.model_id() == model.model_id()));
    assert!(repo
        .get_active_embedding_model()
        .await
        .expect("active model")
        .is_some());
}
