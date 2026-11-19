use super::merge::copy_dir_recursive;
use super::*;

fn write(path: &Path, body: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, body).unwrap();
}

#[test]
fn files_merge_skips_digests_already_present() {
    let dir = tempfile::tempdir().unwrap();
    let extracted = dir.path().join("files");
    let root = dir.path().join("library");

    write(&extracted.join("aaa").join("one.md"), "new");
    write(&extracted.join("bbb").join("two.md"), "also new");
    // Already in the library, with different bytes: must not be replaced.
    write(&root.join("aaa").join("one.md"), "existing");

    let restored = copy_new_files(&extracted, &root);

    assert_eq!(restored, 1);
    assert_eq!(
        std::fs::read_to_string(root.join("aaa").join("one.md")).unwrap(),
        "existing"
    );
    assert_eq!(
        std::fs::read_to_string(root.join("bbb").join("two.md")).unwrap(),
        "also new"
    );
}

#[test]
fn web_archive_merge_restores_articles_without_overwriting_existing_ones() {
    let dir = tempfile::tempdir().unwrap();
    let extracted = dir.path().join("web-archive");
    let root = dir.path().join("home-web-archive");

    write(
        &extracted
            .join("example.com")
            .join("lost-abc")
            .join("page.html"),
        "restored snapshot",
    );
    write(
        &extracted
            .join("example.com")
            .join("kept-def")
            .join("page.html"),
        "older copy",
    );
    write(
        &root.join("example.com").join("kept-def").join("page.html"),
        "current copy",
    );

    let restored = copy_missing_tree(&extracted, &root);

    assert_eq!(restored, 1);
    assert_eq!(
        std::fs::read_to_string(root.join("example.com").join("lost-abc").join("page.html"))
            .unwrap(),
        "restored snapshot"
    );
    assert_eq!(
        std::fs::read_to_string(root.join("example.com").join("kept-def").join("page.html"))
            .unwrap(),
        "current copy"
    );
}

#[test]
fn files_merge_tolerates_a_missing_source() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(copy_new_files(&dir.path().join("nope"), dir.path()), 0);
}

#[test]
fn vault_lands_in_place_when_the_target_is_missing() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("extracted-vault");
    write(&source.join("note.md"), "hello");
    let target = dir.path().join("Lattice");

    let placed = place_vault(&source, &target).unwrap();

    assert_eq!(placed, target);
    assert_eq!(
        std::fs::read_to_string(target.join("note.md")).unwrap(),
        "hello"
    );
}

#[test]
fn vault_lands_in_place_when_the_target_is_an_empty_directory() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("extracted-vault");
    write(&source.join("note.md"), "hello");
    let target = dir.path().join("Lattice");
    std::fs::create_dir_all(&target).unwrap();

    assert_eq!(place_vault(&source, &target).unwrap(), target);
    assert!(target.join("note.md").exists());
}

#[test]
fn a_vault_with_content_is_never_written_into() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("extracted-vault");
    write(&source.join("note.md"), "from backup");
    let target = dir.path().join("Lattice");
    write(&target.join("mine.md"), "my work");

    let placed = place_vault(&source, &target).unwrap();

    assert_ne!(placed, target);
    assert!(placed
        .to_string_lossy()
        .contains(&format!("{}-restored-", target.display())));
    assert_eq!(
        std::fs::read_to_string(target.join("mine.md")).unwrap(),
        "my work"
    );
    assert!(!target.join("note.md").exists());
    assert_eq!(
        std::fs::read_to_string(placed.join("note.md")).unwrap(),
        "from backup"
    );
}

#[test]
fn move_dir_falls_back_to_a_recursive_copy() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("src");
    write(&source.join("a").join("b.md"), "deep");
    let target = dir.path().join("dst");

    copy_dir_recursive(&source, &target).unwrap();

    assert_eq!(
        std::fs::read_to_string(target.join("a").join("b.md")).unwrap(),
        "deep"
    );
}

#[tokio::test]
async fn restore_places_owner_restore_marker_beside_files_root_so_may_sweep_adopts_host_owner() {
    use super::super::config::ArchiveConfig;
    use super::super::crypto::{self, Argon2Params, MasterKey, RecoveryCode};
    use super::super::writer::ArchiveWriter;
    use crate::features::indexing::library_owner::{may_sweep, LibraryFiles};
    use lattice_core::application::ports::settings_port::MockSettingsRepository;
    use lattice_core::application::ports::SettingsRepositoryPort;
    use lattice_core::shared::persistence::Database;
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};

    let source = tempfile::tempdir().unwrap();
    let source_app_data = source.path().join("app-data");
    let source_lattice_home = source.path().join("lattice-home");
    let source_files_root = source_lattice_home.join("files");
    std::fs::create_dir_all(&source_app_data).unwrap();
    std::fs::create_dir_all(&source_files_root).unwrap();

    let source_db_path = source_app_data.join("lattice.db");
    lattice_core::shared::testing::sqlite::write_migrated(&source_db_path)
        .await
        .unwrap();
    let source_pool = SqlitePoolOptions::new()
        .connect_with(SqliteConnectOptions::new().filename(&source_db_path))
        .await
        .unwrap();
    sqlx::query("INSERT INTO library_owner (id, owner_id) VALUES (1, 'source-owner-id')")
        .execute(&source_pool)
        .await
        .unwrap();
    let source_db = Database::new(source_pool);

    let destination = tempfile::tempdir().unwrap();
    let master = MasterKey::generate();
    let recovery = RecoveryCode::generate().unwrap();
    let envelope =
        crypto::build_envelope(&master, None, &recovery, Argon2Params::for_tests()).unwrap();
    let source_key_store = Arc::new(MasterKeyStore::with_service(
        "tech.lattice.test.restore-owner-marker".to_string(),
        source_app_data.clone(),
        false,
    ));
    source_key_store.store(&master).unwrap();
    ArchiveConfig {
        envelope,
        destination: Some(destination.path().to_path_buf()),
        ..ArchiveConfig::default()
    }
    .save(&source_app_data)
    .await
    .unwrap();

    let settings: Arc<dyn SettingsRepositoryPort> = Arc::new(MockSettingsRepository::new());
    let writer = ArchiveWriter::new(
        source_db.clone(),
        source_app_data.clone(),
        settings,
        source_key_store,
        Some(source_files_root),
        None,
    );
    let archive = writer.create().await.unwrap();
    source_db.close().await;

    let target = tempfile::tempdir().unwrap();
    let target_app_data = target.path().join("app-data");
    let target_lattice_home = target.path().join("lattice-home");
    let target_files_root = target_lattice_home.join("files");
    std::fs::create_dir_all(&target_app_data).unwrap();
    std::fs::create_dir_all(&target_files_root).unwrap();
    let owner_file = target_lattice_home.join("owner");
    std::fs::write(&owner_file, "existing-host-owner-id\n").unwrap();

    let target_db_path = target_app_data.join("lattice.db");
    lattice_core::shared::testing::sqlite::write_migrated(&target_db_path)
        .await
        .unwrap();
    let target_pool = SqlitePoolOptions::new()
        .connect_with(SqliteConnectOptions::new().filename(&target_db_path))
        .await
        .unwrap();
    let target_key_store = Arc::new(MasterKeyStore::with_service(
        "tech.lattice.test.restore-owner-marker".to_string(),
        target_app_data.clone(),
        false,
    ));
    let restorer = ArchiveRestorer::new(
        Database::new(target_pool),
        target_db_path.clone(),
        target_app_data.clone(),
        Arc::new(MockSettingsRepository::new()),
        target_key_store,
        Some(target_files_root.clone()),
        None,
    );

    let phrase = recovery.phrase();
    let outcome = restorer
        .restore(&archive.path, Some(&phrase))
        .await
        .unwrap();
    assert!(matches!(outcome, RestoreOutcome::Restored { .. }));

    let expected_marker = target_lattice_home.join("owner.restore");
    assert!(
        expected_marker.exists(),
        "owner.restore must be written beside files_root in <lattice_home>"
    );
    assert!(
        !target_app_data.join("owner.restore").exists(),
        "owner.restore must not be written inside app_data_dir"
    );

    let reopened_pool = SqlitePoolOptions::new()
        .connect_with(SqliteConnectOptions::new().filename(&target_db_path))
        .await
        .unwrap();
    let reopened_db = Database::new(reopened_pool.clone());
    let files = LibraryFiles {
        owner_file,
        roots: vec![target_files_root],
    };
    assert!(
        may_sweep(&reopened_db, &files).await,
        "may_sweep must adopt the existing host owner file after backup restore"
    );
    assert!(
        !expected_marker.exists(),
        "may_sweep should consume owner.restore after adopting the host owner"
    );
    let adopted_id: Option<String> =
        sqlx::query_scalar("SELECT owner_id FROM library_owner WHERE id = 1")
            .fetch_optional(&reopened_pool)
            .await
            .unwrap();
    assert_eq!(adopted_id.as_deref(), Some("existing-host-owner-id"));
    reopened_pool.close().await;
}
