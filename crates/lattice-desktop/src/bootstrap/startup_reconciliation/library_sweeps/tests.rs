//! The startup sweeps over real files in a tempdir and a real migrated
//! database: which files they leave alone and which they collect, at start and
//! after a chat is deleted, and the model cleanup that reads the same decision.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use reqwest::header::HeaderMap;
use sha2::{Digest, Sha256};
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::SqlitePool;
use tempfile::TempDir;

use super::super::reconcile_orphaned_files;
use super::{open_gate_if_owner, LibrarySweeps};
use crate::features::conversation::repository::ConversationRepository;
use crate::features::indexing::library_owner::LibraryFiles;
use crate::features::indexing::LibraryGc;
use crate::features::web::images::{ChatImageService, ImageNetwork};
use crate::features::web::mocks::MockPageCapture;
use crate::features::web::services::web::{BoundedBody, BoundedFetchError};
use crate::features::web::snapshots::store::new_id;
use crate::features::web::snapshots::{AppEvents, WebPageSnapshotService};
use crate::infrastructure::persistence::database::DatabaseConnection;
use crate::infrastructure::persistence::repositories::downloaded_models::DownloadedModelRepository;
use crate::infrastructure::persistence::repositories::DocumentRepositoryImpl;
use crate::infrastructure::setup::setup_database;
use crate::infrastructure::storage::{ContentAddressedStorage, FileIntents};
use lattice_core::application::ports::ChatImagesPort;
use lattice_core::shared::constants::RERANKER_MODEL_DIR;
use lattice_core::shared::fs::SweepGate;
use lattice_core::shared::runtime::background::BackgroundTasks;
use lattice_core::shared::runtime::jobs::JobRuntime;

const A: &str = "6f1c0d1e-3b5a-4c52-9d0b-0a1b2c3d4e5f";
const B: &str = "b7e2f4a9-8c61-4d3e-a5f0-9e8d7c6b5a40";

/// A network that refuses every request; the sweeps never reach for one.
struct NoNetwork;

#[async_trait]
impl ImageNetwork for NoNetwork {
    async fn get(
        &self,
        _url: &str,
        _headers: HeaderMap,
        _max_bytes: usize,
        _deadline: Option<Instant>,
    ) -> Result<BoundedBody, BoundedFetchError> {
        Err(BoundedFetchError::Network("no network in this test".into()))
    }
}

async fn migrated_pool() -> SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .idle_timeout(None)
        .max_lifetime(None)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    pool
}

fn files_in(home: &Path) -> LibraryFiles {
    LibraryFiles {
        owner_file: home.join("owner"),
        roots: ["files", "web-snapshots", "chat-images"]
            .map(|name| home.join(name))
            .to_vec(),
    }
}

/// The three collectors over `pool`'s database and the files under `home`,
/// the two services holding `gate` as the container gives it to them.
fn sweeps_over(
    pool: &SqlitePool,
    home: &Path,
    gate: &Arc<SweepGate>,
    background: &Arc<BackgroundTasks>,
) -> LibrarySweeps {
    LibrarySweeps {
        library_gc: Arc::new(
            LibraryGc::new(
                Arc::new(ContentAddressedStorage::with_root(home.join("files"))),
                Arc::new(DocumentRepositoryImpl::new(pool.clone())),
            )
            .with_intents(FileIntents::new(pool.clone())),
        ),
        snapshots: Arc::new(WebPageSnapshotService::new(
            pool.clone(),
            home.join("web-snapshots"),
            Arc::new(MockPageCapture::new()),
            JobRuntime::new(pool.clone()),
            Arc::new(AppEvents::default()),
            Arc::clone(background),
            Arc::clone(gate),
        )),
        images: Arc::new(ChatImageService::new(
            pool.clone(),
            home.join("chat-images"),
            Arc::new(NoNetwork),
            Arc::clone(background),
            Arc::clone(gate),
        )),
    }
}

/// One run of the app: its services and the gate they share.
struct Run {
    gate: Arc<SweepGate>,
    sweeps: LibrarySweeps,
}

/// A start over `pool`'s database and the files under `home`: decide who owns
/// the files, start the sweeps, and wait for them to finish.
async fn start_up(pool: &SqlitePool, home: &Path) -> Run {
    let gate = Arc::new(SweepGate::default());
    let background = Arc::new(BackgroundTasks::default());
    let sweeps = sweeps_over(pool, home, &gate, &background);
    open_gate_if_owner(pool, &files_in(home), &gate).await;
    sweeps.start(&gate, &background, async { Ok(HashSet::new()) });
    background.close();
    background.wait().await;
    Run { gate, sweeps }
}

impl Run {
    /// The user deletes a chat: its rows go by cascade, and the app sweeps the
    /// frozen pages and, as after a truncate, the pictures nothing names.
    async fn delete_a_chat(&self, pool: &SqlitePool) {
        sqlx::query(
            "INSERT INTO conversations (id, title, space_id, created_at, updated_at) \
             VALUES ('chat', 'A chat', 'space_general', '2026-11-01T00:00:00Z', '2026-11-01T00:00:00Z')",
        )
        .execute(pool)
        .await
        .unwrap();
        sqlx::query("DELETE FROM conversations WHERE id = 'chat'")
            .execute(pool)
            .await
            .unwrap();
        self.sweeps.snapshots.sweep().await.unwrap();
        self.sweeps.images.remove_unclaimed_files().await.unwrap();
    }
}

/// The database file is corrupted, moved aside, and an empty database takes
/// its place.
async fn corrupt_and_recover(database: DatabaseConnection, db_path: &Path) -> DatabaseConnection {
    database.close().await.unwrap();
    std::fs::write(db_path, vec![0x42u8; 8192]).unwrap();
    for sidecar in ["-wal", "-shm"] {
        let _ = std::fs::remove_file(format!("{}{sidecar}", db_path.display()));
    }
    let database = setup_database(db_path.to_path_buf()).await.unwrap();
    assert!(
        database.recovered_from().is_some(),
        "the corrupt database was moved aside"
    );
    database
}

/// An imported file as the library keeps it: `files/<sha256>/paper.txt`.
fn imported_file(home: &Path, content: &str) -> (String, PathBuf) {
    let hash = format!("{:x}", Sha256::digest(content.as_bytes()));
    let path = home.join("files").join(&hash).join("paper.txt");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, content).unwrap();
    (hash, path)
}

/// A frozen page no row claims.
fn snapshot(home: &Path) -> PathBuf {
    let path = home
        .join("web-snapshots")
        .join(format!("{}.html", new_id()));
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, "<html></html>").unwrap();
    path
}

/// A chat picture no row claims.
fn picture(home: &Path) -> PathBuf {
    let path = home.join("chat-images").join(format!("{}.png", new_id()));
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, [0x89, b'P', b'N', b'G']).unwrap();
    path
}

/// A model as the downloader leaves it: `<models>/<id>/weights`.
fn model_directory(models: &Path, id: &str) -> PathBuf {
    let directory = models.join(id);
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(directory.join("weights"), b"model data").unwrap();
    directory
}

/// The row of a model that finished downloading into `<models>/<id>`.
async fn installed(pool: &SqlitePool, models: &Path, id: &str) {
    sqlx::query(
        "INSERT INTO models (id, model_id, model_name, base_path, total_size_bytes, status, \
         model_type, architecture, storage_kind, storage_path) \
         VALUES (?, ?, ?, '', 10, 'completed', 'chat', 'llama', 'local_dir', ?)",
    )
    .bind(id)
    .bind(id)
    .bind(id)
    .bind(models.join(id).to_string_lossy().to_string())
    .execute(pool)
    .await
    .unwrap();
}

/// The row of the document that imported the file with `hash`.
async fn reference(pool: &SqlitePool, hash: &str, path: &Path) {
    sqlx::query(
        "INSERT INTO documents (id, file_path, file_name, size_bytes, modified_at, checksum) \
         VALUES (?, ?, 'paper.txt', 1, '2026-11-01T00:00:00Z', ?)",
    )
    .bind(format!("doc-{hash}"))
    .bind(path.to_string_lossy().to_string())
    .bind(hash)
    .execute(pool)
    .await
    .unwrap();
}

async fn record(pool: &SqlitePool, id: &str) {
    sqlx::query("INSERT INTO library_owner (id, owner_id) VALUES (1, ?)")
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
}

/// This database owns the library: it holds `id` and so does the owner file.
async fn own(pool: &SqlitePool, home: &Path, id: &str) {
    record(pool, id).await;
    std::fs::write(home.join("owner"), id).unwrap();
}

async fn recorded(pool: &SqlitePool) -> Option<String> {
    sqlx::query_scalar("SELECT owner_id FROM library_owner WHERE id = 1")
        .fetch_optional(pool)
        .await
        .unwrap()
}

/// What the owner file holds, if it exists.
fn filed(home: &Path) -> Option<String> {
    std::fs::read_to_string(home.join("owner"))
        .ok()
        .map(|text| text.trim().to_string())
}

/// A record a process that has since ended wrote.
async fn earlier_record(pool: &SqlitePool, store: &str, kind: &str, name: &str) {
    sqlx::query(
        "INSERT INTO file_intents (store, kind, name, run_id) VALUES (?, ?, ?, 'an earlier run')",
    )
    .bind(store)
    .bind(kind)
    .bind(name)
    .execute(pool)
    .await
    .unwrap();
}

/// The file name's stem: the id a page or picture is stored under.
fn id_of(path: &Path) -> String {
    path.file_stem().unwrap().to_string_lossy().into_owned()
}

/// A chat with one frozen page and one picture, rows and files.
async fn chat_with_files(pool: &SqlitePool, home: &Path, chat: &str) -> [PathBuf; 2] {
    sqlx::query(
        "INSERT INTO conversations (id, title, space_id, created_at, updated_at)          VALUES (?, 'A chat', 'space_general', '2026-11-01T00:00:00Z', '2026-11-01T00:00:00Z')",
    )
    .bind(chat)
    .execute(pool)
    .await
    .unwrap();
    let page = snapshot(home);
    sqlx::query(
        "INSERT INTO web_page_snapshots (id, conversation_id, normalized_url, requested_url,          status, created_at) VALUES (?, ?, 'https://example.com/', 'https://example.com/',          'ready', '2026-11-01T00:00:00Z')",
    )
    .bind(id_of(&page))
    .bind(chat)
    .execute(pool)
    .await
    .unwrap();
    let picture = picture(home);
    sqlx::query(
        "INSERT INTO chat_images (id, conversation_id, request_id, ref, ext, mime, sha256,          byte_size, width, height, caption, origin, created_at)          VALUES (?, ?, 'request', 'img-1', 'png', 'image/png', '', 4, 1, 1, '', 'page',          '2026-11-01T00:00:00Z')",
    )
    .bind(id_of(&picture))
    .bind(chat)
    .execute(pool)
    .await
    .unwrap();
    [page, picture]
}

fn deleted(paths: &[PathBuf]) -> Vec<&PathBuf> {
    paths.iter().filter(|path| !path.exists()).collect()
}

#[tokio::test]
async fn a_fresh_database_beside_a_library_leaves_every_file_alone() {
    let home = TempDir::new().unwrap();
    let pool = migrated_pool().await;
    let (_, paper) = imported_file(home.path(), "an imported paper");
    let library = [paper, snapshot(home.path()), picture(home.path())];

    start_up(&pool, home.path()).await;

    let gone = deleted(&library);
    assert!(gone.is_empty(), "the startup sweep deleted {gone:?}");
    assert_eq!(recorded(&pool).await, None);
    assert_eq!(filed(home.path()), None);
}

#[tokio::test]
async fn a_fresh_database_beside_another_installs_library_leaves_every_file_alone() {
    let home = TempDir::new().unwrap();
    let pool = migrated_pool().await;
    std::fs::create_dir_all(home.path()).unwrap();
    std::fs::write(home.path().join("owner"), A).unwrap();
    let (_, paper) = imported_file(home.path(), "an imported paper");
    let library = [paper, snapshot(home.path()), picture(home.path())];

    start_up(&pool, home.path()).await;

    let gone = deleted(&library);
    assert!(gone.is_empty(), "the startup sweep deleted {gone:?}");
    assert_eq!(recorded(&pool).await, None);
    assert_eq!(filed(home.path()).as_deref(), Some(A));
}

#[tokio::test]
async fn the_library_survives_corruption_recovery_even_once_the_new_database_has_a_document() {
    let home = TempDir::new().unwrap();
    let app = TempDir::new().unwrap();
    let db_path = app.path().join("lattice.db");

    // The database that imported the library takes it at its first start.
    let database = setup_database(db_path.clone()).await.unwrap();
    start_up(database.pool(), home.path()).await;
    let (_, paper) = imported_file(home.path(), "an imported paper");
    let library = [paper, snapshot(home.path()), picture(home.path())];
    let owner = filed(home.path());

    let database = corrupt_and_recover(database, &db_path).await;

    start_up(database.pool(), home.path()).await;
    let gone = deleted(&library);
    assert!(gone.is_empty(), "the sweep after recovery deleted {gone:?}");

    // A document is added to the new database; it is not one of the library's.
    let (hash, newer) = imported_file(home.path(), "a paper imported since");
    reference(database.pool(), &hash, &newer).await;
    start_up(database.pool(), home.path()).await;
    let gone = deleted(&library);
    assert!(
        gone.is_empty(),
        "the sweep after a document was added deleted {gone:?}"
    );
    assert!(newer.exists());
    assert_eq!(filed(home.path()), owner, "the owner file is untouched");
    assert_eq!(recorded(database.pool()).await, None);
    database.close().await.unwrap();
}

#[tokio::test]
async fn deleting_a_chat_after_corruption_recovery_leaves_the_old_snapshots_and_pictures() {
    let home = TempDir::new().unwrap();
    let app = TempDir::new().unwrap();
    let db_path = app.path().join("lattice.db");

    // The database that imported the library takes it at its first start.
    let database = setup_database(db_path.clone()).await.unwrap();
    start_up(database.pool(), home.path()).await;
    let library = [snapshot(home.path()), picture(home.path())];

    let database = corrupt_and_recover(database, &db_path).await;

    let run = start_up(database.pool(), home.path()).await;
    let gone = deleted(&library);
    assert!(gone.is_empty(), "the start after recovery deleted {gone:?}");

    run.delete_a_chat(database.pool()).await;

    let gone = deleted(&library);
    assert!(gone.is_empty(), "deleting a chat deleted {gone:?}");
    database.close().await.unwrap();
}

#[tokio::test]
async fn a_database_whose_files_name_another_owner_sweeps_nothing() {
    let home = TempDir::new().unwrap();
    let pool = migrated_pool().await;
    record(&pool, B).await;
    std::fs::write(home.path().join("owner"), A).unwrap();
    let (hash, referenced) = imported_file(home.path(), "referenced here");
    reference(&pool, &hash, &referenced).await;
    let (_, unreferenced) = imported_file(home.path(), "referenced nowhere");
    let library = [
        referenced,
        unreferenced,
        snapshot(home.path()),
        picture(home.path()),
    ];

    start_up(&pool, home.path()).await;

    let gone = deleted(&library);
    assert!(gone.is_empty(), "the startup sweep deleted {gone:?}");
    assert_eq!(recorded(&pool).await.as_deref(), Some(B));
    assert_eq!(filed(home.path()).as_deref(), Some(A));
}

/// The owning database no longer has the row of a file it imported: a backup
/// from before the import was restored, or a migration lost the row. Nothing
/// it wrote says the file is garbage, so the file stays.
#[tokio::test]
async fn the_owning_database_keeps_an_imported_file_its_rows_no_longer_name() {
    let home = TempDir::new().unwrap();
    let pool = migrated_pool().await;
    own(&pool, home.path(), A).await;
    let (hash, referenced) = imported_file(home.path(), "referenced here");
    reference(&pool, &hash, &referenced).await;
    let (_, unnamed) = imported_file(home.path(), "imported after the backup");

    start_up(&pool, home.path()).await;

    assert!(
        unnamed.exists(),
        "the startup sweep deleted a file no row names"
    );
    assert!(referenced.exists());
}

#[tokio::test]
async fn the_owning_database_keeps_a_frozen_page_its_rows_no_longer_name() {
    let home = TempDir::new().unwrap();
    let pool = migrated_pool().await;
    own(&pool, home.path(), A).await;
    let page = snapshot(home.path());

    start_up(&pool, home.path()).await;

    assert!(
        page.exists(),
        "the startup sweep deleted a page no row names"
    );
}

#[tokio::test]
async fn the_owning_database_keeps_a_picture_its_rows_no_longer_name() {
    let home = TempDir::new().unwrap();
    let pool = migrated_pool().await;
    own(&pool, home.path(), A).await;
    let picture = picture(home.path());

    start_up(&pool, home.path()).await;

    assert!(
        picture.exists(),
        "the startup sweep deleted a picture no row names"
    );
}

#[tokio::test]
async fn deleting_a_chat_keeps_the_pages_and_pictures_no_row_names() {
    let home = TempDir::new().unwrap();
    let pool = migrated_pool().await;
    own(&pool, home.path(), A).await;
    let run = start_up(&pool, home.path()).await;
    // Files whose rows went some other way than this chat's delete.
    let unnamed = [snapshot(home.path()), picture(home.path())];

    run.delete_a_chat(&pool).await;

    let gone = deleted(&unnamed);
    assert!(gone.is_empty(), "deleting a chat deleted {gone:?}");
}

#[tokio::test]
async fn an_existing_install_takes_its_files_on_upgrade_and_keeps_what_no_record_names() {
    let home = TempDir::new().unwrap();
    let pool = migrated_pool().await;
    // Before this version nothing recorded who owns the files, or which of
    // them an import left behind.
    let (hash, referenced) = imported_file(home.path(), "referenced here");
    reference(&pool, &hash, &referenced).await;
    let (_, unexplained) = imported_file(home.path(), "copied, never committed");

    start_up(&pool, home.path()).await;

    let id = recorded(&pool).await.expect("the database took the files");
    assert_eq!(filed(home.path()).as_deref(), Some(id.as_str()));
    assert!(unexplained.exists(), "a file no record names is kept");
    assert!(referenced.exists());
}

#[tokio::test]
async fn a_fresh_database_beside_a_models_directory_keeps_every_model() {
    let home = TempDir::new().unwrap();
    let models = TempDir::new().unwrap();
    let pool = migrated_pool().await;
    // The install's earlier database named itself the owner; this one has no id.
    std::fs::write(home.path().join("owner"), A).unwrap();
    let kept = ["chat-model", "embedding-model"].map(|id| model_directory(models.path(), id));

    let run = start_up(&pool, home.path()).await;
    let removed = reconcile_orphaned_files(
        Arc::new(DownloadedModelRepository::new(pool)),
        models.path(),
        &run.gate,
    )
    .await
    .unwrap();

    assert_eq!(removed, 0);
    let gone = deleted(&kept);
    assert!(gone.is_empty(), "the model cleanup deleted {gone:?}");
}

#[tokio::test]
async fn the_owning_database_keeps_a_model_directory_no_row_names() {
    let home = TempDir::new().unwrap();
    let models = TempDir::new().unwrap();
    let pool = migrated_pool().await;
    own(&pool, home.path(), A).await;
    installed(&pool, models.path(), "installed").await;
    let kept = [
        model_directory(models.path(), "installed"),
        // The model manager downloads the reranker here with no row of its own.
        model_directory(models.path(), RERANKER_MODEL_DIR),
        model_directory(models.path(), "row-lost"),
    ];

    let run = start_up(&pool, home.path()).await;
    let removed = reconcile_orphaned_files(
        Arc::new(DownloadedModelRepository::new(pool)),
        models.path(),
        &run.gate,
    )
    .await
    .unwrap();

    assert_eq!(removed, 0);
    let gone = deleted(&kept);
    assert!(gone.is_empty(), "the model cleanup deleted {gone:?}");
}

/// What the owning database's own records name goes at startup: an import an
/// earlier run never committed, and a page and a picture a delete let go of
/// whose removal did not finish. What no record names stays.
#[tokio::test]
async fn the_owning_database_collects_what_its_records_name() {
    let home = TempDir::new().unwrap();
    let pool = migrated_pool().await;
    own(&pool, home.path(), A).await;
    let (hash, crashed) = imported_file(home.path(), "copied, never committed");
    earlier_record(&pool, "library", "import", &hash).await;
    let page = snapshot(home.path());
    earlier_record(&pool, "web_snapshot", "release", &id_of(&page)).await;
    let released_picture = picture(home.path());
    let name = released_picture
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    earlier_record(&pool, "chat_image", "release", &name).await;
    let collected = [crashed, page, released_picture];
    let (_, unnamed_file) = imported_file(home.path(), "no record names me");
    let unnamed = [unnamed_file, snapshot(home.path()), picture(home.path())];

    start_up(&pool, home.path()).await;

    let left: Vec<_> = collected.iter().filter(|path| path.exists()).collect();
    assert!(left.is_empty(), "the owner's sweep left {left:?}");
    let gone = deleted(&unnamed);
    assert!(gone.is_empty(), "the owner's sweep deleted {gone:?}");
    let records: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM file_intents")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(records, 0);
}

#[tokio::test]
async fn deleting_a_chat_through_the_app_frees_its_pages_and_pictures() {
    let home = TempDir::new().unwrap();
    let pool = migrated_pool().await;
    own(&pool, home.path(), A).await;
    let run = start_up(&pool, home.path()).await;
    let chats_files = chat_with_files(&pool, home.path(), "chat").await;
    let unnamed = [snapshot(home.path()), picture(home.path())];

    ConversationRepository::new(pool.clone())
        .delete("chat")
        .await
        .unwrap();
    run.sweeps.snapshots.sweep().await.unwrap();
    run.sweeps.images.remove_unclaimed_files().await.unwrap();

    let left: Vec<_> = chats_files.iter().filter(|path| path.exists()).collect();
    assert!(left.is_empty(), "the deleted chat left {left:?}");
    let gone = deleted(&unnamed);
    assert!(gone.is_empty(), "deleting a chat deleted {gone:?}");
}
