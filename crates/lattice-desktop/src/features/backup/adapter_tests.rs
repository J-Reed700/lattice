//! Tests for the backup adapter.
use super::*;
use lattice_core::shared::testing::links::{self, LinkKind};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use tempfile::tempdir;

#[tokio::test]
async fn rollback_failure_preserves_the_safety_copy_and_reports_its_location() {
    let dir = tempdir().unwrap();
    let safety = dir.path().join("original.db.pre_restore");
    fs::write(&safety, b"original database").unwrap();
    // A non-empty directory is a deterministic rename failure on every
    // platform, including elevated processes that bypass permission bits.
    let destination = dir.path().join("blocked.db");
    fs::create_dir(&destination).unwrap();
    fs::write(destination.join("keep"), b"existing file").unwrap();

    let note = roll_back_database(&safety, &destination).await.unwrap();
    assert!(note.contains("could not be put back automatically"));
    assert!(note.contains(safety.to_str().unwrap()));
    assert_eq!(fs::read(&safety).unwrap(), b"original database");
    assert_eq!(
        fs::read(destination.join("keep")).unwrap(),
        b"existing file"
    );
}

#[tokio::test]
async fn rollback_success_restores_original_bytes_without_a_failure_message() {
    let dir = tempdir().unwrap();
    let safety = dir.path().join("original.db.pre_restore");
    let destination = dir.path().join("live.db");
    fs::write(&safety, b"original database").unwrap();
    fs::write(&destination, b"replacement database").unwrap();

    assert!(roll_back_database(&safety, &destination).await.is_none());
    assert_eq!(fs::read(&destination).unwrap(), b"original database");
    assert!(!safety.exists());
}

/// Open a read-write pool on an existing fixture file.
///
/// Options rather than a `sqlite://…` string for the same reason production
/// uses them (see [`read_only_backup_options`]): several of these fixtures
/// are paths that came back out of the adapter, which canonicalizes them,
/// and a canonicalized Windows path carries a `\\?\` prefix that no DSN can
/// survive. Asserting through an options-built pool also keeps the tests
/// from quietly passing on a DSN habit we have just removed.
async fn connect_fixture_pool(path: &Path) -> SqlitePool {
    SqlitePoolOptions::new()
        .connect_with(SqliteConnectOptions::new().filename(path))
        .await
        .unwrap()
}

async fn create_test_pool() -> (SqlitePool, PathBuf, tempfile::TempDir) {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test.db");

    std::fs::File::create(&db_path).unwrap();

    let pool = connect_fixture_pool(&db_path).await;

    sqlx::query(
        r#"
            CREATE TABLE documents (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                content TEXT NOT NULL
            )
            "#,
    )
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query("INSERT INTO documents (id, title, content) VALUES (?, ?, ?)")
        .bind("1")
        .bind("Test Doc")
        .bind("Test content")
        .execute(&pool)
        .await
        .unwrap();

    (pool, db_path, dir)
}

#[tokio::test]

async fn test_create_backup() {
    let (pool, db_path, _dir) = create_test_pool().await;
    let adapter = BackupAdapter::new(Database::new(pool.clone()), db_path.clone());

    let backup_path = adapter.create_backup(None).await.unwrap();

    assert!(PathBuf::from(&backup_path).exists());
    assert!(backup_path.contains("recall_backup_"));

    pool.close().await;
}

#[tokio::test]

async fn test_create_backup_custom_path() {
    let (pool, db_path, _dir) = create_test_pool().await;
    let adapter = BackupAdapter::new(Database::new(pool.clone()), db_path.clone());

    let backup_root = db_path.parent().unwrap().join("backups");
    fs::create_dir_all(&backup_root).unwrap();
    let custom_path = backup_root.join("my_backup.db");

    let backup_path = adapter
        .create_backup(Some(custom_path.clone()))
        .await
        .unwrap();

    // Canonicalize both paths for comparison (macOS /var symlinks to /private/var)
    let expected_path = custom_path.canonicalize().unwrap();
    let actual_path = PathBuf::from(&backup_path).canonicalize().unwrap();
    assert_eq!(actual_path, expected_path);
    assert!(custom_path.exists());

    pool.close().await;
}

#[tokio::test]

async fn test_list_backups() {
    let (pool, db_path, _dir) = create_test_pool().await;
    let adapter = BackupAdapter::new(Database::new(pool.clone()), db_path.clone());

    let _ = adapter.create_backup(None).await.unwrap();
    // Sleep 1 second to avoid timestamp collision (backups use second-precision timestamps)
    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
    let _ = adapter.create_backup(None).await.unwrap();

    let data_dir = db_path.parent().unwrap().to_path_buf();
    let backups = adapter.list_backups(data_dir).await.unwrap();

    assert_eq!(backups.len(), 2);
    assert!(backups[0].size > 0);
    assert!(backups[0].name.contains("recall_backup_"));

    pool.close().await;
}

#[tokio::test]

async fn test_validate_backup() {
    let (pool, db_path, _dir) = create_test_pool().await;
    let adapter = BackupAdapter::new(Database::new(pool.clone()), db_path.clone());

    let backup_path = adapter.create_backup(None).await.unwrap();
    let backup_pathbuf = PathBuf::from(backup_path);

    let result = adapter.validate_backup(&backup_pathbuf).await;
    assert!(result.is_ok());

    pool.close().await;
}

#[tokio::test]

async fn test_validate_invalid_backup() {
    let (pool, db_path, _dir) = create_test_pool().await;
    let adapter = BackupAdapter::new(Database::new(pool.clone()), db_path.clone());

    let invalid_path = PathBuf::from("/nonexistent/backup.db");
    let result = adapter.validate_backup(&invalid_path).await;

    assert!(result.is_err());

    pool.close().await;
}

#[tokio::test]

async fn test_backup_file_count() {
    let (pool, db_path, _dir) = create_test_pool().await;
    let adapter = BackupAdapter::new(Database::new(pool.clone()), db_path.clone());

    let backup_path = adapter.create_backup(None).await.unwrap();
    let backup_pathbuf = PathBuf::from(backup_path);

    let count = adapter.get_backup_file_count(&backup_pathbuf).await;
    assert_eq!(count, Some(1)); // We inserted 1 document in setup

    pool.close().await;
}

#[tokio::test]

async fn test_create_backup_rejects_parent_directory_traversal() {
    let (pool, db_path, _dir) = create_test_pool().await;
    let adapter = BackupAdapter::new(Database::new(pool.clone()), db_path.clone());

    // Attempt to traverse to parent directory
    let malicious_path = db_path
        .parent()
        .unwrap()
        .join("../../../etc/cron.d/backdoor.db");

    let result = adapter.create_backup(Some(malicious_path)).await;

    // Should be rejected with security error
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        matches!(err, AppError::Security(_))
            || matches!(err, AppError::InvalidInput(_))
            || matches!(err, AppError::PermissionDenied(_)),
        "Expected a path security error, got: {:?}",
        err
    );

    pool.close().await;
}

#[tokio::test]

async fn test_create_backup_rejects_absolute_system_path() {
    let (pool, db_path, _dir) = create_test_pool().await;
    let adapter = BackupAdapter::new(Database::new(pool.clone()), db_path.clone());

    // Attempt to write to system directory
    #[cfg(unix)]
    let malicious_path = PathBuf::from("/etc/cron.d/backdoor.db");
    #[cfg(windows)]
    let malicious_path = PathBuf::from("C:\\Windows\\System32\\backdoor.db");

    let result = adapter.create_backup(Some(malicious_path)).await;

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        matches!(err, AppError::Security(_))
            || matches!(err, AppError::InvalidInput(_))
            || matches!(err, AppError::PermissionDenied(_)),
        "Expected Security/InvalidInput/PermissionDenied error, got: {:?}",
        err
    );

    pool.close().await;
}

#[tokio::test]
async fn a_backup_name_with_two_dots_or_percent_signs_is_a_plain_name() {
    for name in ["notes..db", "%2e%2e%2fetc.db"] {
        let (pool, db_path, _dir) = create_test_pool().await;
        let adapter = BackupAdapter::new(Database::new(pool.clone()), db_path.clone());

        let backup_dir = db_path.parent().unwrap().join("backups");
        fs::create_dir_all(&backup_dir).unwrap();
        let path = backup_dir.join(name);

        let result = adapter.create_backup(Some(path.clone())).await;

        assert!(result.is_ok(), "{name}: {result:?}");
        assert!(path.exists(), "{name}");

        pool.close().await;
    }
}

#[tokio::test]

async fn test_create_backup_allows_valid_path_in_backup_dir() {
    let (pool, db_path, _dir) = create_test_pool().await;
    let adapter = BackupAdapter::new(Database::new(pool.clone()), db_path.clone());

    let backup_dir = db_path.parent().unwrap().join("backups");
    fs::create_dir_all(&backup_dir).unwrap();

    let valid_path = backup_dir.join("my_custom_backup.db");

    let result = adapter.create_backup(Some(valid_path.clone())).await;

    assert!(result.is_ok());
    assert!(valid_path.exists());

    pool.close().await;
}

async fn create_backup_rejects_a_link_escape(kind: LinkKind) {
    let (pool, db_path, _dir) = create_test_pool().await;
    let adapter = BackupAdapter::new(Database::new(pool.clone()), db_path.clone());

    let backup_dir = db_path.parent().unwrap().join("backups");
    fs::create_dir_all(&backup_dir).unwrap();

    let link_path = backup_dir.join("escape_link");
    let target_path = db_path.parent().unwrap().parent().unwrap();

    if links::link_dir(kind, target_path, &link_path).unwrap() {
        let malicious_path = link_path.join("evil.db");

        let result = adapter.create_backup(Some(malicious_path)).await;

        assert!(
            result.is_err(),
            "a {kind} out of the backup folder must be refused"
        );

        fs::remove_file(&link_path).ok();
    }

    pool.close().await;
}

#[tokio::test]
async fn test_create_backup_rejects_symlink_escape() {
    create_backup_rejects_a_link_escape(LinkKind::Symlink).await;
}

#[cfg(windows)]
#[tokio::test]
async fn test_create_backup_rejects_junction_escape() {
    create_backup_rejects_a_link_escape(LinkKind::Junction).await;
}

#[tokio::test]

async fn test_create_backup_handles_directory_input() {
    let (pool, db_path, _dir) = create_test_pool().await;
    let adapter = BackupAdapter::new(Database::new(pool.clone()), db_path.clone());

    let backup_dir = db_path.parent().unwrap().join("backups");
    fs::create_dir_all(&backup_dir).unwrap();

    // Provide directory instead of file path
    let result = adapter.create_backup(Some(backup_dir.clone())).await;

    assert!(result.is_ok());

    let backup_path = result.unwrap();
    assert!(PathBuf::from(&backup_path).exists());
    assert!(backup_path.contains("recall_backup_"));

    pool.close().await;
}

#[tokio::test]

async fn test_create_backup_defense_in_depth() {
    let (pool, db_path, _dir) = create_test_pool().await;
    let adapter = BackupAdapter::new(Database::new(pool.clone()), db_path.clone());

    // Even if initial validation were bypassed, canonicalization
    // and final check should catch traversal
    let backup_dir = db_path.parent().unwrap().join("backups");
    fs::create_dir_all(&backup_dir).unwrap();

    // Attempt various traversal patterns
    let patterns = vec![
        "../../../etc/passwd",
        "..\\..\\..\\windows\\system32\\config",
        "./../../../tmp/evil.db",
        "backup/../../../etc/shadow",
    ];

    for pattern in patterns {
        let malicious_path = backup_dir.join(pattern);
        let result = adapter.create_backup(Some(malicious_path)).await;

        // All should be rejected
        assert!(
            result.is_err(),
            "Pattern '{}' should have been rejected",
            pattern
        );
    }

    pool.close().await;
}

#[tokio::test]

async fn test_restore_backup_success() {
    let (pool, db_path, _dir) = create_test_pool().await;
    let adapter = BackupAdapter::new(Database::new(pool.clone()), db_path.clone());

    // User-created collections live in ordinary SQLite tables. Include both
    // collection metadata and normalized memberships in this database copy
    // round trip so backup coverage protects the feature's persisted state.
    sqlx::query("CREATE TABLE custom_collections (id TEXT PRIMARY KEY, name TEXT NOT NULL)")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("CREATE TABLE custom_collection_documents (collection_id TEXT NOT NULL, document_id TEXT NOT NULL, ordinal INTEGER NOT NULL, PRIMARY KEY(collection_id, document_id))")
        .execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO custom_collections (id, name) VALUES ('collection-1', 'Research')")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO custom_collection_documents (collection_id, document_id, ordinal) VALUES ('collection-1', 'doc-1', 0)")
        .execute(&pool).await.unwrap();

    let backup_path = adapter.create_backup(None).await.unwrap();
    let backup_pathbuf = PathBuf::from(backup_path);

    sqlx::query("INSERT INTO documents (id, title, content) VALUES (?, ?, ?)")
        .bind("2")
        .bind("New Doc")
        .bind("New content")
        .execute(&pool)
        .await
        .unwrap();

    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM documents")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count.0, 2);
    sqlx::query("DELETE FROM custom_collection_documents WHERE collection_id = 'collection-1'")
        .execute(&pool)
        .await
        .unwrap();

    // Restore from backup (closes pool internally)
    let result = adapter.restore_backup(backup_pathbuf).await;
    assert!(result.is_ok());

    // Verify safety backup was created
    let safety_backup = db_path.with_extension("db.pre_restore");
    assert!(safety_backup.exists());

    assert!(db_path.exists());

    let new_pool = connect_fixture_pool(&db_path).await;

    let restored_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM documents")
        .fetch_one(&new_pool)
        .await
        .unwrap();

    assert_eq!(restored_count.0, 1);
    let collection_name: String =
        sqlx::query_scalar("SELECT name FROM custom_collections WHERE id = 'collection-1'")
            .fetch_one(&new_pool)
            .await
            .unwrap();
    let membership_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM custom_collection_documents WHERE collection_id = 'collection-1' AND document_id = 'doc-1'")
        .fetch_one(&new_pool).await.unwrap();
    assert_eq!(collection_name, "Research");
    assert_eq!(membership_count, 1);

    new_pool.close().await;
}

/// What a process meant to do to its files when the backup was taken is no
/// guide to the files after a restore: they may since have been imported
/// again by the database the restore replaces.
#[tokio::test]
async fn a_restored_database_keeps_no_file_records_of_the_one_it_came_from() {
    let (pool, db_path, _dir) = create_test_pool().await;
    sqlx::query(
        "CREATE TABLE file_intents (id INTEGER PRIMARY KEY AUTOINCREMENT, store TEXT NOT NULL, \
         kind TEXT NOT NULL, name TEXT NOT NULL, run_id TEXT NOT NULL, created_at TEXT)",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO file_intents (store, kind, name, run_id) \
         VALUES ('library', 'import', ?, 'the run that took the backup')",
    )
    .bind("a".repeat(64))
    .execute(&pool)
    .await
    .unwrap();
    let adapter = BackupAdapter::new(Database::new(pool.clone()), db_path.clone());
    let backup = PathBuf::from(adapter.create_backup(None).await.unwrap());

    adapter.restore_backup(backup).await.unwrap();

    let restored = connect_fixture_pool(&db_path).await;
    let records: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM file_intents")
        .fetch_one(&restored)
        .await
        .unwrap();
    let documents: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM documents")
        .fetch_one(&restored)
        .await
        .unwrap();
    assert_eq!(records, 0);
    assert_eq!(documents, 1, "the rest of the database is restored");
    restored.close().await;
}

#[tokio::test]

async fn test_restore_backup_validates_integrity() {
    let (pool, db_path, _dir) = create_test_pool().await;
    let adapter = BackupAdapter::new(Database::new(pool.clone()), db_path.clone());

    let invalid_backup = tempdir().unwrap();
    let invalid_path = invalid_backup.path().join("corrupt.db");
    fs::write(&invalid_path, b"not a valid sqlite database").unwrap();

    // Attempt to restore invalid backup
    let result = adapter.restore_backup(invalid_path).await;

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        matches!(err, AppError::Database(_)),
        "Expected Database error, got: {:?}",
        err
    );

    // Original database should still exist and be valid
    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM documents")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count.0, 1);

    pool.close().await;
}

#[tokio::test]
async fn safety_backup_includes_rows_still_in_the_wal() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("live.db");
    // WAL as production opens it, with automatic checkpoints off so the
    // last commit is guaranteed to sit in -wal only.
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            SqliteConnectOptions::new()
                .filename(&db_path)
                .create_if_missing(true)
                .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
                .pragma("wal_autocheckpoint", "0"),
        )
        .await
        .unwrap();
    sqlx::query(
        "CREATE TABLE documents (id TEXT PRIMARY KEY, title TEXT NOT NULL, content TEXT NOT NULL)",
    )
    .execute(&pool)
    .await
    .unwrap();
    let adapter = BackupAdapter::new(Database::new(pool.clone()), db_path.clone());
    let backup = PathBuf::from(adapter.create_backup(None).await.unwrap());

    sqlx::query("INSERT INTO documents (id, title, content) VALUES ('late', 'Late', 'committed just before restore')")
        .execute(&pool)
        .await
        .unwrap();
    let wal = PathBuf::from(format!("{}-wal", db_path.display()));
    assert!(
        fs::metadata(&wal).unwrap().len() > 0,
        "the row must be in the WAL"
    );

    adapter.restore_backup(backup).await.unwrap();

    let safety = db_path.with_extension("db.pre_restore");
    let safety_pool = connect_fixture_pool(&safety).await;
    let rows: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM documents WHERE id = 'late'")
        .fetch_one(&safety_pool)
        .await
        .unwrap();
    assert_eq!(rows.0, 1, "the safety copy lost a committed row");
    safety_pool.close().await;
}

#[tokio::test]

async fn test_restore_backup_creates_safety_backup() {
    let (pool, db_path, _dir) = create_test_pool().await;
    let adapter = BackupAdapter::new(Database::new(pool.clone()), db_path.clone());

    let backup_path = adapter.create_backup(None).await.unwrap();
    let backup_pathbuf = PathBuf::from(backup_path);

    // Restore from backup
    adapter.restore_backup(backup_pathbuf).await.unwrap();

    // Verify safety backup exists
    let safety_backup = db_path.with_extension("db.pre_restore");
    assert!(
        safety_backup.exists(),
        "Safety backup should be created at: {}",
        safety_backup.display()
    );

    // Verify safety backup is valid SQLite database
    let safety_pool = connect_fixture_pool(&safety_backup).await;

    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM documents")
        .fetch_one(&safety_pool)
        .await
        .unwrap();
    assert_eq!(count.0, 1);

    safety_pool.close().await;
}

#[tokio::test]
async fn failed_vacuum_keeps_the_previous_recovery_copy() {
    let (pool, db_path, _dir) = create_test_pool().await;
    let adapter = BackupAdapter::new(Database::new(pool.clone()), db_path.clone());
    let safety_path = db_path.with_extension("db.pre_restore");
    fs::write(&safety_path, b"last known-good recovery copy").unwrap();
    pool.close().await;

    assert!(adapter.write_safety_copy(&safety_path).await.is_err());
    assert_eq!(
        fs::read(&safety_path).unwrap(),
        b"last known-good recovery copy"
    );
    assert_eq!(
        fs::read_dir(safety_path.parent().unwrap())
            .unwrap()
            .filter_map(std::result::Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().ends_with(".tmp"))
            .count(),
        0,
        "failed snapshot should not leave a temporary file"
    );
}

#[tokio::test]
async fn successful_safety_snapshot_replaces_the_previous_copy() {
    let (pool, db_path, _dir) = create_test_pool().await;
    let adapter = BackupAdapter::new(Database::new(pool.clone()), db_path.clone());
    let safety_path = db_path.with_extension("db.pre_restore");
    fs::write(&safety_path, b"stale recovery copy").unwrap();

    adapter.write_safety_copy(&safety_path).await.unwrap();
    assert_ne!(fs::read(&safety_path).unwrap(), b"stale recovery copy");
    adapter.validate_backup(&safety_path).await.unwrap();

    pool.close().await;
}

#[tokio::test]
#[cfg_attr(
    target_os = "macos",
    ignore = "macOS SQLite doesn't respect file permissions on open connections"
)]

async fn test_restore_backup_rolls_back_on_failure() {
    let (pool, db_path, _dir) = create_test_pool().await;
    let adapter = BackupAdapter::new(Database::new(pool.clone()), db_path.clone());

    let original_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM documents")
        .fetch_one(&pool)
        .await
        .unwrap();

    let backup_path = adapter.create_backup(None).await.unwrap();
    let backup_pathbuf = PathBuf::from(&backup_path);

    // Close the pool before restore
    pool.close().await;

    let new_pool = connect_fixture_pool(&db_path).await;

    let adapter2 = BackupAdapter::new(Database::new(new_pool.clone()), db_path.clone());

    // Restores create sibling safety and temporary files. Make the parent
    // directory read-only so the failure is portable across Unix filesystems;
    // a read-only database file can still be atomically replaced.
    #[cfg(unix)]
    let parent_permissions = {
        use std::os::unix::fs::PermissionsExt;
        let parent = db_path.parent().unwrap();
        let original = fs::metadata(parent).unwrap().permissions();
        let perms = fs::Permissions::from_mode(0o555);
        fs::set_permissions(parent, perms).unwrap();
        original
    };
    // Elsewhere a read-only folder still takes new files: a folder where the
    // restore copies the backup first makes that copy fail.
    #[cfg(not(unix))]
    fs::create_dir(db_path.with_extension("db.restoring")).unwrap();

    let result = adapter2.restore_backup(backup_pathbuf).await;

    #[cfg(unix)]
    {
        // Restore permissions for cleanup
        let parent = db_path.parent().unwrap();
        fs::set_permissions(parent, parent_permissions).unwrap();
    }

    assert!(
        result.is_err(),
        "Restore should fail when it cannot write its files"
    );

    let verify_pool = connect_fixture_pool(&db_path).await;

    let final_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM documents")
        .fetch_one(&verify_pool)
        .await
        .unwrap();

    assert_eq!(
        final_count.0, original_count.0,
        "Original database should be preserved after failed restore"
    );

    verify_pool.close().await;
    new_pool.close().await;
}

#[tokio::test]

async fn test_restore_backup_handles_missing_file() {
    let (pool, db_path, _dir) = create_test_pool().await;
    let adapter = BackupAdapter::new(Database::new(pool.clone()), db_path.clone());

    let missing_path = PathBuf::from("/nonexistent/backup.db");
    let result = adapter.restore_backup(missing_path).await;

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        matches!(err, AppError::NotFound(_)),
        "Expected NotFound error, got: {:?}",
        err
    );

    // Original database should be unchanged
    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM documents")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count.0, 1);

    pool.close().await;
}

#[tokio::test]

async fn test_restore_backup_atomic_operation() {
    let (pool, db_path, _dir) = create_test_pool().await;
    let adapter = BackupAdapter::new(Database::new(pool.clone()), db_path.clone());

    let backup_path = adapter.create_backup(None).await.unwrap();
    let backup_pathbuf = PathBuf::from(backup_path);

    // Restore from backup
    adapter.restore_backup(backup_pathbuf).await.unwrap();

    let temp_path = db_path.with_extension("db.restoring");
    assert!(
        !temp_path.exists(),
        "Temporary file should be cleaned up: {}",
        temp_path.display()
    );

    assert!(db_path.exists());

    let new_pool = connect_fixture_pool(&db_path).await;

    let integrity: (String,) = sqlx::query_as("PRAGMA integrity_check")
        .fetch_one(&new_pool)
        .await
        .unwrap();

    assert_eq!(integrity.0, "ok");

    new_pool.close().await;
}

/// `VACUUM INTO` takes the path as a bound value, so a folder named with `;`
/// and `--` is an ordinary folder.
#[tokio::test]
async fn a_backup_into_a_folder_named_with_semicolons_and_dashes_succeeds() {
    let (pool, db_path, _dir) = create_test_pool().await;
    let adapter = BackupAdapter::new(Database::new(pool.clone()), db_path.clone());

    let folder = db_path.parent().unwrap().join("backups").join("a;b--c");
    fs::create_dir_all(&folder).unwrap();
    let target = folder.join("my_backup.db");

    let backup_path = adapter.create_backup(Some(target.clone())).await.unwrap();

    assert_eq!(
        PathBuf::from(&backup_path).canonicalize().unwrap(),
        target.canonicalize().unwrap()
    );
    adapter.validate_backup(&target).await.unwrap();

    pool.close().await;
}

/// Backup names that look like SQL are only names: the backup is written
/// under exactly that name and the live database is untouched.
#[tokio::test]
async fn test_sql_looking_backup_names_are_plain_names() {
    let (pool, db_path, _dir) = create_test_pool().await;
    let adapter = BackupAdapter::new(Database::new(pool.clone()), db_path.clone());

    let backup_dir = db_path.parent().unwrap().join("backups");
    fs::create_dir_all(&backup_dir).unwrap();

    let names = [
        "backup.db'; DROP TABLE documents; --",
        "backup.db--comment",
        "backup' UNION SELECT id FROM documents; --",
        "backup' DROP TABLE documents CASCADE; --",
        "backup'; INSERT INTO documents VALUES ('evil', 'bad'); --",
        "backup'; EXEC sp_executesql; --",
        "backup'; EXECUTE ('DROP TABLE documents'); --",
        "user's_backup.db",
    ];

    for name in names {
        let target = backup_dir.join(name);
        let backup_path = adapter
            .create_backup(Some(target.clone()))
            .await
            .unwrap_or_else(|error| panic!("name '{name}' should be a plain name: {error:?}"));

        assert_eq!(
            PathBuf::from(&backup_path).file_name().unwrap(),
            std::ffi::OsStr::new(name)
        );
        adapter.validate_backup(&target).await.unwrap();
    }

    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM documents")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count.0, 1, "documents table should still have its data");

    pool.close().await;
}

/// Comment markers that contain a path separator name a folder that is not
/// there, so nothing is written.
#[tokio::test]
async fn test_backup_into_a_missing_folder_is_rejected() {
    let (pool, db_path, _dir) = create_test_pool().await;
    let adapter = BackupAdapter::new(Database::new(pool.clone()), db_path.clone());

    let backup_dir = db_path.parent().unwrap().join("backups");
    fs::create_dir_all(&backup_dir).unwrap();

    for pattern in ["backup.db/* comment */", "backup/**/file.db"] {
        let result = adapter.create_backup(Some(backup_dir.join(pattern))).await;

        assert!(result.is_err(), "Pattern '{}' should be rejected", pattern);
    }

    pool.close().await;
}

#[tokio::test]

async fn test_sql_injection_null_byte() {
    let (pool, db_path, _dir) = create_test_pool().await;
    let adapter = BackupAdapter::new(Database::new(pool.clone()), db_path.clone());

    let backup_dir = db_path.parent().unwrap().join("backups");
    fs::create_dir_all(&backup_dir).unwrap();

    // Attempt null byte injection (path truncation attack)
    let malicious_filename = "backup.db\0.txt";
    let malicious_path = backup_dir.join(malicious_filename);

    let result = adapter.create_backup(Some(malicious_path)).await;

    assert!(result.is_err());
    // Will fail in the file name validation; either way it should not succeed

    pool.close().await;
}
