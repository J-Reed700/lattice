use crate::features::download::download_repository::{
    DownloadRepository, SqliteDownloadRepository,
};
use crate::features::download::engine::mock::MockDownloadEngine;
use crate::features::download::manager::DownloadManagerService;
use crate::infrastructure::persistence::database::connection::DatabaseConnection;
use lattice_core::application::ports::download_manager::{
    DownloadBatchItem, DownloadManager, DownloadRequest,
};
use lattice_core::domain::download::DownloadSession;
use lattice_core::shared::persistence::driver::DriverPool;
use std::path::{Path, PathBuf};
use std::sync::Arc;

struct Fixture {
    _dir: tempfile::TempDir,
    root: PathBuf,
    connection: Arc<DatabaseConnection>,
    repository: Arc<SqliteDownloadRepository>,
    manager: DownloadManagerService,
}

async fn fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    let connection = Arc::new(
        DatabaseConnection::new(root.join("downloads.db"))
            .await
            .unwrap(),
    );
    sqlx::migrate!("./migrations")
        .run(connection.database().sqlx_pool())
        .await
        .unwrap();
    let repository = Arc::new(SqliteDownloadRepository::new(connection.clone()));
    let manager = DownloadManagerService::new(
        repository.clone(),
        Arc::new(MockDownloadEngine::new()),
        root.clone(),
        Arc::default(),
    );
    Fixture {
        _dir: dir,
        root,
        connection,
        repository,
        manager,
    }
}

fn request(destination: PathBuf) -> DownloadRequest {
    DownloadRequest {
        url: format!(
            "https://example.com/{}",
            destination.file_name().unwrap().to_string_lossy()
        ),
        destination,
        checksum: None,
        auth_token: None,
        model_name: None,
        model_id: None,
        model_file_name: None,
    }
}

async fn refuse(connection: &DatabaseConnection, event: &str) {
    sqlx::query(&format!(
        "CREATE TRIGGER refuse_{event} BEFORE {event} ON download_sessions
         BEGIN SELECT RAISE(ABORT, 'injected'); END"
    ))
    .execute(connection.database().sqlx_pool())
    .await
    .unwrap();
}

fn earlier_session(id: &str, destination: &Path) -> DownloadSession {
    let mut session = DownloadSession::new(
        id.to_string(),
        "https://example.com/earlier.bin".to_string(),
        destination.to_path_buf(),
        Some(1),
        None,
    )
    .unwrap();
    session.start().unwrap();
    session.complete().unwrap();
    session
}

#[tokio::test]
async fn a_batch_with_a_refused_item_saves_none_of_its_items() {
    let fixture = fixture().await;
    refuse(&fixture.connection, "DELETE").await;
    let items = vec![
        DownloadBatchItem {
            requests: vec![request(fixture.root.join("first.bin"))],
        },
        DownloadBatchItem {
            requests: vec![request(
                fixture.root.join("sub").join("..").join("second.bin"),
            )],
        },
    ];

    let outcome = fixture.manager.start_download_batch(items).await;

    assert!(outcome.is_err(), "the second item is refused");
    assert!(
        fixture.repository.list().await.unwrap().is_empty(),
        "no session of a refused batch is left behind"
    );
}

#[tokio::test]
async fn a_failed_registration_keeps_the_session_it_would_have_superseded() {
    let fixture = fixture().await;
    let destination = fixture.root.join("model.bin");
    fixture
        .repository
        .create(&earlier_session("earlier", &destination))
        .await
        .unwrap();
    refuse(&fixture.connection, "INSERT").await;

    let outcome = fixture
        .manager
        .enqueue_download(request(destination.clone()))
        .await;

    assert!(outcome.is_err(), "the new session cannot be saved");
    assert!(
        fixture.repository.get("earlier").await.unwrap().is_some(),
        "the earlier session is deleted only together with the new one's save"
    );
}
