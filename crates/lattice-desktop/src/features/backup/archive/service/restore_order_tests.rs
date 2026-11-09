use super::*;

/// A restore whose database swap fails has already placed the files, and left
/// the old database as it was: the files of a backup land before the rows
/// that name them.
#[tokio::test]
async fn a_restore_that_stops_at_the_database_swap_has_already_placed_its_files() {
    let source = tempfile::tempdir().unwrap();
    let app_data = source.path().join("app-data");
    let files_root = source.path().join("files-library");
    std::fs::create_dir_all(&app_data).unwrap();
    std::fs::create_dir_all(files_root.join(REFERENCED_BLOB)).unwrap();
    std::fs::write(
        files_root.join(REFERENCED_BLOB).join("paper.pdf"),
        b"pdf bytes",
    )
    .unwrap();
    let db_path = app_data.join("lattice.db");
    let pool = seeded_pool(&db_path, 3).await;
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
            None,
        )),
        Arc::new(ArchiveRestorer::new(
            pool.clone(),
            db_path.clone(),
            app_data.clone(),
            settings.clone(),
            key_store.clone(),
            Some(files_root.clone()),
            None,
        )),
        key_store,
        app_data.clone(),
        app_data.clone(),
    );
    let setup = service.begin_setup().await.unwrap();
    let confirmations: Vec<WordConfirmationDto> = setup
        .confirm_indices
        .iter()
        .map(|index| WordConfirmationDto {
            index: *index,
            word: setup.recovery_words[*index as usize].clone(),
        })
        .collect();
    service.confirm_setup(confirmations, None).await.unwrap();
    service
        .set_destination(destination.path().to_path_buf())
        .await
        .unwrap();
    let run = service.create_now().await.unwrap();

    let target = tempfile::tempdir().unwrap();
    let target_app_data = target.path().join("app-data");
    let target_files_root = target.path().join("files-library");
    std::fs::create_dir_all(&target_app_data).unwrap();
    let target_db = target_app_data.join("lattice.db");
    let target_pool = seeded_pool(&target_db, 1).await;
    std::fs::create_dir_all(target_app_data.join("lattice.db.restoring")).unwrap();
    let restorer = ArchiveRestorer::new(
        target_pool,
        target_db.clone(),
        target_app_data.clone(),
        Arc::new(MockSettingsRepository::new()),
        isolated_key_store(&target_app_data),
        Some(target_files_root.clone()),
        None,
    );

    let phrase = setup.recovery_words.join(" ");
    let outcome = restorer.restore(&run.path, Some(&phrase)).await;

    assert!(outcome.is_err(), "the swap is blocked");
    assert_eq!(
        std::fs::read(target_files_root.join(REFERENCED_BLOB).join("paper.pdf")).unwrap(),
        b"pdf bytes",
        "the files are in place before the database is swapped"
    );
    assert_eq!(
        document_count(&target_db).await,
        1,
        "the old database is untouched"
    );
    pool.close().await;
}
