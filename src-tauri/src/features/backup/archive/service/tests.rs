use super::*;

fn words() -> Vec<String> {
    [
        "abandon", "ability", "able", "about", "above", "absent", "absorb", "abstract", "absurd",
        "abuse", "access", "accident", "account", "accuse", "achieve", "acid", "acoustic",
        "acquire", "across", "act", "action", "actor", "actress", "actual",
    ]
    .iter()
    .map(|w| w.to_string())
    .collect()
}

fn answer(index: usize, word: &str) -> WordConfirmationDto {
    WordConfirmationDto {
        index: index as u32,
        word: word.to_string(),
    }
}

#[test]
fn right_words_are_accepted() {
    let indices = [0usize, 5, 23];
    let given = vec![
        answer(0, "abandon"),
        answer(5, "absent"),
        answer(23, "actual"),
    ];
    assert!(verify_confirmations(&words(), &indices, &given).is_ok());
}

#[test]
fn case_and_padding_are_forgiven() {
    let indices = [0usize, 5, 23];
    let given = vec![
        answer(0, "  ABANDON "),
        answer(5, "Absent"),
        answer(23, "aCtUaL"),
    ];
    assert!(verify_confirmations(&words(), &indices, &given).is_ok());
}

#[test]
fn a_wrong_word_is_rejected() {
    let indices = [0usize, 5, 23];
    let given = vec![
        answer(0, "abandon"),
        answer(5, "absurd"), // a real BIP-39 word, but not word 6
        answer(23, "actual"),
    ];
    let err = verify_confirmations(&words(), &indices, &given).unwrap_err();
    assert!(matches!(err, AppError::InvalidInput(_)));
    assert!(err.to_string().contains("word 6"));
}

#[test]
fn a_missing_answer_is_rejected() {
    let indices = [0usize, 5, 23];
    let given = vec![answer(0, "abandon"), answer(5, "absent")];
    assert!(verify_confirmations(&words(), &indices, &given).is_err());
}

#[test]
fn answers_for_words_we_did_not_ask_about_are_rejected() {
    let indices = [0usize, 5, 23];
    let given = vec![
        answer(0, "abandon"),
        answer(5, "absent"),
        answer(7, "abstract"),
    ];
    assert!(verify_confirmations(&words(), &indices, &given).is_err());
}

#[test]
fn confirm_indices_are_three_distinct_positions_in_range() {
    for _ in 0..200 {
        let picked = pick_confirm_indices();
        assert!(picked.iter().all(|i| *i < RECOVERY_WORD_COUNT));
        assert!(picked[0] < picked[1] && picked[1] < picked[2]);
    }
}

#[test]
fn passphrase_length_is_enforced_and_blank_means_none() {
    assert_eq!(normalize_passphrase(None).unwrap(), None);
    assert_eq!(normalize_passphrase(Some("   ".into())).unwrap(), None);
    assert!(normalize_passphrase(Some("short".into())).is_err());
    assert_eq!(
        normalize_passphrase(Some("longenough".into())).unwrap(),
        Some("longenough".to_string())
    );
}
// -- end to end --------------------------------------------------------

use lattice_core::application::ports::settings_port::MockSettingsRepository;
use lattice_core::application::ports::SettingsRepositoryPort;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;

/// The blob every seeded document points at: it is in the archive.
const REFERENCED_BLOB: &str = "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08";
/// A blob no document points at: an import that never committed. It is
/// on disk, and it is not in the archive.
const ORPHAN_BLOB: &str = "5feceb66ffc86f38d952786c6d696c79c2dbc239dd4e91b46729d73a27fb57e9";

/// Open a read-write pool on a fixture file.
///
/// Built from `SqliteConnectOptions` rather than a `sqlite://…` string for
/// the reason `snapshot.rs` and `adapter.rs` do: a DSN is a URL and a
/// Windows path is not a URL component, so a drive letter becomes the
/// authority and a `\\?\` verbatim prefix truncates the filename at its
/// `?`. Passing the `Path` through sidesteps the whole question.
async fn connect_fixture_pool(db_path: &Path) -> SqlitePool {
    SqlitePoolOptions::new()
        .connect_with(SqliteConnectOptions::new().filename(db_path))
        .await
        .unwrap()
}

/// A file-backed pool with one row in it, built the way `adapter.rs`
/// builds its fixtures.
async fn seeded_pool(db_path: &Path, rows: u32) -> SqlitePool {
    std::fs::File::create(db_path).unwrap();
    let pool = connect_fixture_pool(db_path).await;
    sqlx::query(
        "CREATE TABLE documents (id TEXT PRIMARY KEY, title TEXT NOT NULL, checksum TEXT NOT NULL)",
    )
    .execute(&pool)
    .await
    .unwrap();
    for n in 0..rows {
        sqlx::query("INSERT INTO documents (id, title, checksum) VALUES (?, ?, ?)")
            .bind(n.to_string())
            .bind(format!("Doc {n}"))
            // Every document shares the one referenced blob, which is
            // what the library's deduplication would produce anyway.
            .bind(REFERENCED_BLOB)
            .execute(&pool)
            .await
            .unwrap();
    }
    pool
}

async fn document_count(db_path: &Path) -> i64 {
    let pool = connect_fixture_pool(db_path).await;
    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM documents")
        .fetch_one(&pool)
        .await
        .unwrap();
    pool.close().await;
    count.0
}

/// Never the real keyring and never `~/.lattice/files`: this test writes
/// and restores real files, and it must only ever touch its own tempdirs.
fn isolated_key_store(app_data_dir: &Path) -> Arc<MasterKeyStore> {
    Arc::new(MasterKeyStore::with_service(
        "tech.lattice.test.archive-round-trip".to_string(),
        app_data_dir.to_path_buf(),
        false,
    ))
}

/// The whole feature in one pass: set up, write an archive, then restore
/// it on a machine that has never seen the key, using only the recovery
/// code. This is the promise the feature makes to the user.
#[tokio::test]
async fn archive_round_trip_restores_database() {
    // --- source machine ---
    let source = tempfile::tempdir().unwrap();
    let app_data = source.path().join("app-data");
    let files_root = source.path().join("files-library");
    std::fs::create_dir_all(&app_data).unwrap();
    // A library as it really looks: the blob the documents reference, an
    // orphan from an import that died, and something that is not a blob
    // at all. Only the first is the user's data.
    std::fs::create_dir_all(files_root.join(REFERENCED_BLOB)).unwrap();
    std::fs::write(
        files_root.join(REFERENCED_BLOB).join("paper.pdf"),
        b"pdf bytes",
    )
    .unwrap();
    std::fs::create_dir_all(files_root.join(ORPHAN_BLOB)).unwrap();
    std::fs::write(
        files_root.join(ORPHAN_BLOB).join("ghost.pdf"),
        b"nothing points at this",
    )
    .unwrap();
    std::fs::write(files_root.join("stray.txt"), b"not a blob").unwrap();
    // A saved web article: web documents point their file_path here.
    let web_archive_root = source.path().join("web-archive");
    let article_dir = web_archive_root
        .join("example.com")
        .join("an-article-abc123");
    std::fs::create_dir_all(&article_dir).unwrap();
    std::fs::write(article_dir.join("page.html"), b"<html>snapshot</html>").unwrap();
    // A frozen page copy, beside the web archive as in `~/.lattice`.
    let web_snapshots_root = source.path().join("web-snapshots");
    std::fs::create_dir_all(&web_snapshots_root).unwrap();
    std::fs::write(
        web_snapshots_root.join("0123456789abcdef0123456789abcdef.html"),
        b"<html>frozen</html>",
    )
    .unwrap();

    let db_path = app_data.join("lattice.db");
    let pool = seeded_pool(&db_path, 3).await;

    // Outside app_data, as `validate_destination` insists.
    let destination = tempfile::tempdir().unwrap();

    let settings: Arc<dyn SettingsRepositoryPort> = Arc::new(MockSettingsRepository::new());
    let key_store = isolated_key_store(&app_data);
    let service = ArchiveService::new(
        Arc::new(ArchiveWriter::new(
            pool.clone(),
            app_data.clone(),
            settings.clone(),
            key_store.clone(),
            Some(files_root.clone()),
            Some(web_archive_root.clone()),
        )),
        Arc::new(ArchiveRestorer::new(
            pool.clone(),
            db_path.clone(),
            app_data.clone(),
            settings.clone(),
            key_store.clone(),
            Some(files_root.clone()),
            Some(web_archive_root.clone()),
        )),
        key_store,
        app_data.clone(),
        app_data.clone(),
    );

    // Nothing configured yet.
    let status = service.status().await.unwrap();
    assert!(!status.configured);
    assert!(status.archives.is_empty());
    assert_eq!(status.keep_count, DEFAULT_KEEP_COUNT);

    // Setup: generate, confirm the three words, choose a folder.
    let setup = service.begin_setup().await.unwrap();
    assert_eq!(setup.recovery_words.len(), RECOVERY_WORD_COUNT);
    assert_eq!(setup.confirm_indices.len(), CONFIRM_WORD_COUNT);

    let confirmations: Vec<WordConfirmationDto> = setup
        .confirm_indices
        .iter()
        .map(|index| WordConfirmationDto {
            index: *index,
            // Upper case on purpose: the wizard must forgive it.
            word: setup.recovery_words[*index as usize].to_uppercase(),
        })
        .collect();
    let status = service.confirm_setup(confirmations, None).await.unwrap();
    assert!(status.configured);
    assert!(status.recovery_confirmed);
    assert!(!status.has_passphrase);

    // The frontend sets the passphrase as a separate step afterwards.
    let status = service
        .set_passphrase(Some("correct horse battery".to_string()))
        .await
        .unwrap();
    assert!(status.has_passphrase);

    let status = service
        .set_destination(destination.path().to_path_buf())
        .await
        .unwrap();
    assert!(!status.destination_missing);

    // --- write one ---
    let run = service.create_now().await.unwrap();
    assert!(run.size > 0, "archive should not be empty");
    assert!(super::super::format::is_archive_file_name(
        run.path.file_name().unwrap().to_str().unwrap()
    ));

    let status = service.status().await.unwrap();
    assert_eq!(status.archives.len(), 1);
    assert!(status.last_error.is_none());
    assert_eq!(status.last_success.as_ref().map(|s| s.size), Some(run.size));

    // The scratch directory never survives a run.
    assert!(!app_data.join("backup").join("scratch").join("x").exists());

    // --- fresh machine: no key, no config, empty database ---
    let target = tempfile::tempdir().unwrap();
    let target_app_data = target.path().join("app-data");
    let target_files_root = target.path().join("files-library");
    let target_web_archive_root = target.path().join("web-archive");
    std::fs::create_dir_all(&target_app_data).unwrap();

    let target_db = target_app_data.join("lattice.db");
    let target_pool = seeded_pool(&target_db, 0).await;
    let target_key_store = isolated_key_store(&target_app_data);
    let target_settings: Arc<dyn SettingsRepositoryPort> = Arc::new(MockSettingsRepository::new());
    let restorer = ArchiveRestorer::new(
        target_pool.clone(),
        target_db.clone(),
        target_app_data.clone(),
        target_settings,
        target_key_store.clone(),
        Some(target_files_root.clone()),
        Some(target_web_archive_root.clone()),
    );

    // Without a secret there is nothing to unwrap the key with.
    assert!(matches!(
        restorer.restore(&run.path, None).await.unwrap(),
        RestoreOutcome::NeedsSecret
    ));

    // A wrong secret is distinguishable from a corrupt file.
    let wrong = restorer
        .restore(&run.path, Some("not the passphrase"))
        .await;
    assert!(matches!(wrong, Err(ArchiveError::WrongSecret)));

    // The recovery code alone gets everything back.
    let phrase = setup.recovery_words.join(" ");
    let outcome = restorer.restore(&run.path, Some(&phrase)).await.unwrap();
    let RestoreOutcome::Restored {
        restart_required,
        reembed_required,
        files_restored,
        ..
    } = outcome
    else {
        panic!("expected a completed restore, got {outcome:?}");
    };
    assert!(restart_required);
    assert!(reembed_required);
    assert_eq!(
        files_restored, 1,
        "only the referenced blob should have been in the archive"
    );

    assert_eq!(document_count(&target_db).await, 3, "rows should be back");
    assert_eq!(
        std::fs::read(target_files_root.join(REFERENCED_BLOB).join("paper.pdf")).unwrap(),
        b"pdf bytes"
    );
    assert!(
        !target_files_root.join(ORPHAN_BLOB).exists(),
        "an orphan blob was packed and restored"
    );
    assert!(
        !target_files_root.join("stray.txt").exists(),
        "a stray file was packed and restored"
    );
    assert_eq!(
        std::fs::read(
            target_web_archive_root
                .join("example.com")
                .join("an-article-abc123")
                .join("page.html")
        )
        .unwrap(),
        b"<html>snapshot</html>",
        "saved web articles must come back with the documents that point at them"
    );
    assert_eq!(
        std::fs::read(
            target
                .path()
                .join("web-snapshots")
                .join("0123456789abcdef0123456789abcdef.html")
        )
        .unwrap(),
        b"<html>frozen</html>",
        "frozen web snapshots must come back beside the web archive"
    );
    // The source library keeps everything: a backup is not a sweep.
    assert!(files_root.join(ORPHAN_BLOB).join("ghost.pdf").exists());
    assert!(
        target_app_data
            .join(super::super::restore::REEMBED_MARKER)
            .exists(),
        "the corpus must be flagged for re-embedding"
    );
    assert!(
        target_key_store.load().unwrap().is_some(),
        "the unwrapped key should be adopted so backups can resume"
    );
    let adopted = ArchiveConfig::load(&target_app_data)
        .await
        .unwrap()
        .expect("a config should exist after a secret-based restore");
    assert!(adopted.is_configured());
    assert!(
        adopted.destination.is_none(),
        "the source machine's folder does not exist here"
    );

    pool.close().await;
}
