//! The startup sweeps over real files in a tempdir and a real migrated
//! database: which files they leave alone and which they collect.

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

use super::LibrarySweeps;
use crate::features::indexing::library_owner::LibraryFiles;
use crate::features::indexing::LibraryGc;
use crate::features::web::images::{ChatImageService, ImageNetwork};
use crate::features::web::mocks::MockPageCapture;
use crate::features::web::services::web::{BoundedBody, BoundedFetchError};
use crate::features::web::snapshots::store::new_id;
use crate::features::web::snapshots::{AppEvents, WebPageSnapshotService};
use crate::infrastructure::persistence::repositories::DocumentRepositoryImpl;
use crate::infrastructure::setup::setup_database;
use crate::infrastructure::storage::ContentAddressedStorage;
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

/// The three collectors over `pool`'s database and the files under `home`.
fn sweeps_over(pool: &SqlitePool, home: &Path, background: &Arc<BackgroundTasks>) -> LibrarySweeps {
    LibrarySweeps {
        library_gc: Arc::new(LibraryGc::new(
            Arc::new(ContentAddressedStorage::with_root(home.join("files"))),
            Arc::new(DocumentRepositoryImpl::new(pool.clone())),
        )),
        snapshots: Arc::new(WebPageSnapshotService::new(
            pool.clone(),
            home.join("web-snapshots"),
            Arc::new(MockPageCapture::new()),
            JobRuntime::new(pool.clone()),
            Arc::new(AppEvents::default()),
            Arc::clone(background),
        )),
        images: Arc::new(ChatImageService::new(
            pool.clone(),
            home.join("chat-images"),
            Arc::new(NoNetwork),
            Arc::clone(background),
        )),
    }
}

/// A start over `pool`'s database and the files under `home`: decide, start
/// the sweeps, and wait for them to finish.
async fn start_up(pool: &SqlitePool, home: &Path) {
    let background = Arc::new(BackgroundTasks::default());
    sweeps_over(pool, home, &background)
        .start(pool, &files_in(home), &background, async {
            Ok(HashSet::new())
        })
        .await;
    background.close();
    background.wait().await;
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
    database.close().await.unwrap();

    // The file is corrupted, moved aside, and an empty database takes its place.
    std::fs::write(&db_path, vec![0x42u8; 8192]).unwrap();
    for sidecar in ["-wal", "-shm"] {
        let _ = std::fs::remove_file(format!("{}{sidecar}", db_path.display()));
    }
    let database = setup_database(db_path).await.unwrap();
    assert!(
        database.recovered_from().is_some(),
        "the corrupt database was moved aside"
    );

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

#[tokio::test]
async fn the_owning_database_still_collects_what_nothing_references() {
    let home = TempDir::new().unwrap();
    let pool = migrated_pool().await;
    record(&pool, A).await;
    std::fs::write(home.path().join("owner"), A).unwrap();
    let (hash, referenced) = imported_file(home.path(), "referenced here");
    reference(&pool, &hash, &referenced).await;
    // An import that crashed before it committed, a frozen page and a picture
    // whose rows are gone.
    let (_, crashed) = imported_file(home.path(), "copied, never committed");
    let orphans = [crashed, snapshot(home.path()), picture(home.path())];

    start_up(&pool, home.path()).await;

    let left: Vec<_> = orphans.iter().filter(|path| path.exists()).collect();
    assert!(left.is_empty(), "the owner's sweep left {left:?}");
    assert!(referenced.exists(), "a referenced file survives the sweep");
}

#[tokio::test]
async fn an_existing_install_with_documents_takes_its_files_on_upgrade_and_sweeps() {
    let home = TempDir::new().unwrap();
    let pool = migrated_pool().await;
    // Before this version nothing recorded who owns the files.
    let (hash, referenced) = imported_file(home.path(), "referenced here");
    reference(&pool, &hash, &referenced).await;
    let (_, crashed) = imported_file(home.path(), "copied, never committed");

    start_up(&pool, home.path()).await;

    let id = recorded(&pool).await.expect("the database took the files");
    assert_eq!(filed(home.path()).as_deref(), Some(id.as_str()));
    assert!(!crashed.exists(), "what nothing references is collected");
    assert!(referenced.exists(), "a referenced file survives the sweep");
}
