use super::rescan::ghost_sweep_block_reason;
use super::*;

#[tokio::test]
async fn vault_note_larger_than_four_mib_is_read_in_full() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("large.md");
    let body = "a".repeat(5 * 1024 * 1024);
    std::fs::write(&path, &body).unwrap();
    assert_eq!(read_note(&path).await.unwrap().len(), body.len());
}

fn note(id: &str) -> String {
    format!("---\nid: {id}\ntitle: \"t\"\ncreated_at: t\nupdated_at: t\ntags: []\n---\n\nbody")
}

#[tokio::test]
async fn a_note_renamed_in_finder_is_found_under_its_new_name() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("other.md"), note("other")).unwrap();
    std::fs::write(dir.path().join("My title.md"), note("abc")).unwrap();
    let removed = dir.path().join("abc.md");

    assert_eq!(
        find_renamed_note(&removed, "abc").await,
        Some(dir.path().join("My title.md"))
    );
    assert_eq!(find_renamed_note(&removed, "gone").await, None);
}

#[tokio::test]
async fn a_renamed_file_is_importable_but_a_copy_is_not() {
    let dir = tempfile::tempdir().unwrap();
    let renamed = dir.path().join("My title.md");
    std::fs::write(&renamed, note("abc")).unwrap();
    assert!(is_importable_name(&renamed, "abc").await);
    assert!(is_importable_name(&dir.path().join("abc.md"), "abc").await);

    // The original is still there: this is a copy, not a rename.
    std::fs::write(dir.path().join("abc.md"), note("abc")).unwrap();
    assert!(!is_importable_name(&renamed, "abc").await);
}

#[test]
fn ghost_sweep_allows_ordinary_deletion() {
    // 10 notes on disk, 1 row unmatched — a normal delete.
    assert_eq!(ghost_sweep_block_reason(true, 10, 1, 11), None);
}

#[test]
fn ghost_sweep_allows_everything_when_nothing_to_delete() {
    // Empty vault with an empty table is not suspicious.
    assert_eq!(ghost_sweep_block_reason(true, 0, 0, 0), None);
}

#[test]
fn ghost_sweep_blocks_on_empty_but_present_directory() {
    // The cloud-placeholder case: directory exists, no files materialised,
    // every row looks like a ghost.
    assert!(ghost_sweep_block_reason(true, 0, 25, 25).is_some());
}

#[test]
fn ghost_sweep_blocks_on_incomplete_walk() {
    // A read error part-way through must never authorise deletions.
    assert!(ghost_sweep_block_reason(false, 12, 3, 15).is_some());
}

#[test]
fn ghost_sweep_blocks_on_mass_disappearance() {
    // Half the corpus vanished between two focus events — not an edit.
    assert!(ghost_sweep_block_reason(true, 10, 10, 20).is_some());
}

#[test]
fn ghost_sweep_ratio_does_not_apply_to_tiny_vaults() {
    // 1 of 2 notes deleted is 50%, but entirely ordinary.
    assert_eq!(ghost_sweep_block_reason(true, 1, 1, 2), None);
}

#[tokio::test]
async fn suppression_marks_and_clears_by_filename() {
    let reg = WriteSuppressionRegistry::new();
    let path_a = PathBuf::from("/tmp/abc-123.md");
    assert!(!reg.was_just_written(&path_a).await);
    reg.mark_written(path_a.clone()).await;
    assert!(reg.was_just_written(&path_a).await);
}

#[tokio::test]
async fn suppression_keys_by_filename_across_path_shapes() {
    let reg = WriteSuppressionRegistry::new();
    let written = PathBuf::from("/Users/me/Lattice/notes/abc-123.md");
    let observed = PathBuf::from("/Users/Me/lattice/notes/abc-123.md");
    reg.mark_written(written).await;
    assert!(reg.was_just_written(&observed).await);
}

#[tokio::test]
async fn suppression_is_negative_for_unknown_path() {
    let reg = WriteSuppressionRegistry::new();
    let path = PathBuf::from("/tmp/never-written.md");
    assert!(!reg.was_just_written(&path).await);
}

#[tokio::test]
async fn suppression_marking_prunes_expired_entries() {
    let reg = WriteSuppressionRegistry::new();
    {
        let mut guard = reg.inner.lock().await;
        let stale = Instant::now()
            .checked_sub(SUPPRESSION_TTL + Duration::from_secs(1))
            .expect("clock can roll back this far in test");
        guard.insert("expired.md".to_string(), stale);
    }
    reg.mark_written(PathBuf::from("/tmp/fresh.md")).await;
    let guard = reg.inner.lock().await;
    assert!(!guard.contains_key("expired.md"));
    assert!(guard.contains_key("fresh.md"));
}

#[test]
fn filename_key_extracts_basename() {
    assert_eq!(filename_key(Path::new("/a/b/c.md")).unwrap(), "c.md");
}

#[tokio::test]
async fn import_one_allows_rapid_consecutive_external_edits_without_echo_loop() {
    let pool = lattice_core::shared::testing::sqlite::migrated_in_memory()
        .await
        .unwrap();
    let db = Database::new(pool.clone());
    let dir = tempfile::tempdir().unwrap();
    let note_path = dir.path().join("notes/rapid-note.md");
    std::fs::create_dir_all(note_path.parent().unwrap()).unwrap();
    let app_handle: AppHandleCell = Arc::new(std::sync::RwLock::new(None));

    let v1 = "---\nid: rapid-note\ntitle: \"Title\"\ncreated_at: 2026-11-19T00:00:00Z\nupdated_at: 2026-11-19T00:00:01Z\ntags: [\"tag1\"]\n---\n\nfirst draft";
    std::fs::write(&note_path, v1).unwrap();
    assert_eq!(
        import_one(&note_path, &db, &app_handle, None)
            .await
            .as_deref(),
        Some("rapid-note")
    );

    let v2 = "---\nid: rapid-note\ntitle: \"Title\"\ncreated_at: 2026-11-19T00:00:00Z\nupdated_at: 2026-11-19T00:00:02Z\ntags: [\"tag1\"]\n---\n\nsecond draft";
    std::fs::write(&note_path, v2).unwrap();
    assert_eq!(
        import_one(&note_path, &db, &app_handle, None)
            .await
            .as_deref(),
        Some("rapid-note")
    );

    let saved_body: String =
        sqlx::query_scalar("SELECT content FROM daily_notes_workspace WHERE id = 'rapid-note'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(saved_body, "second draft");
}

#[tokio::test]
async fn vault_watcher_handle_reconfigures_and_stops_cleanly() {
    use lattice_core::application::contracts::settings::SettingsDto;

    struct MutableSettings(parking_lot::Mutex<SettingsDto>);

    #[async_trait::async_trait]
    impl SettingsReaderPort for MutableSettings {
        async fn current_settings(&self) -> lattice_core::shared::error::Result<SettingsDto> {
            Ok(self.0.lock().clone())
        }
    }

    let pool = lattice_core::shared::testing::sqlite::migrated_in_memory()
        .await
        .unwrap();
    let db = Database::new(pool);
    let vault_dir = tempfile::tempdir().unwrap();

    let mut initial = SettingsDto::default();
    initial.vault.enabled = false;
    initial.vault.watch_external_changes = true;
    initial.vault.vault_path = vault_dir.path().to_string_lossy().to_string();

    let settings = Arc::new(MutableSettings(parking_lot::Mutex::new(initial)));
    let background = Arc::new(BackgroundTasks::default());
    let suppression = WriteSuppressionRegistry::new();
    let writer = super::super::writeback::start_vault_writer(
        Arc::clone(&settings),
        suppression.clone(),
        db.clone(),
        &background,
    );
    let watcher = VaultWatcherHandle::new(
        Arc::clone(&settings),
        db,
        suppression,
        Arc::clone(&background),
        RescanLock::default(),
        writer,
    );

    watcher.reconfigure().await;
    assert!(!watcher.is_active().await);

    {
        let mut guard = settings.0.lock();
        guard.vault.enabled = true;
    }
    watcher.reconfigure().await;
    assert!(watcher.is_active().await);

    {
        let mut guard = settings.0.lock();
        guard.vault.watch_external_changes = false;
    }
    watcher.reconfigure().await;
    assert!(!watcher.is_active().await);

    background.close();
    tokio::time::timeout(Duration::from_secs(2), background.wait())
        .await
        .unwrap();
}

#[test]
fn set_aside_conflict_file_detector_matches_only_replaced_stamp_markdown() {
    assert!(is_set_aside_conflict_file(Path::new(
        "/vault/notes/n1.replaced-20261122-150405.md"
    )));
    assert!(is_set_aside_conflict_file(Path::new(
        "/vault/notes/n1.replaced-20261122-150405-2.md"
    )));
    assert!(!is_set_aside_conflict_file(Path::new("/vault/notes/n1.md")));
    assert!(!is_set_aside_conflict_file(Path::new(
        "/vault/notes/my.replaced-note.md"
    )));
    assert!(!is_set_aside_conflict_file(Path::new(
        "/vault/notes/n1.replaced-20261122-150405.txt"
    )));
}

#[tokio::test]
async fn import_one_and_find_renamed_note_ignore_set_aside_files() {
    let pool = lattice_core::shared::testing::sqlite::migrated_in_memory()
        .await
        .unwrap();
    let db = Database::new(pool.clone());
    let dir = tempfile::tempdir().unwrap();
    let notes_dir = dir.path().join("notes");
    std::fs::create_dir_all(&notes_dir).unwrap();
    let app_handle: AppHandleCell = Arc::new(std::sync::RwLock::new(None));

    let replaced = notes_dir.join("n1.replaced-20261122-150405.md");
    std::fs::write(&replaced, note("abc")).unwrap();

    assert_eq!(import_one(&replaced, &db, &app_handle, None).await, None);
    assert_eq!(
        find_renamed_note(&notes_dir.join("abc.md"), "abc").await,
        None
    );
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM daily_notes_workspace")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn rescan_ignores_set_aside_and_untracked_files_without_counting_imports() {
    let pool = lattice_core::shared::testing::sqlite::migrated_in_memory()
        .await
        .unwrap();
    let db = Database::new(pool.clone());
    let dir = tempfile::tempdir().unwrap();
    let notes_dir = dir.path().join("notes");
    std::fs::create_dir_all(&notes_dir).unwrap();
    let app_handle: AppHandleCell = Arc::new(std::sync::RwLock::new(None));
    let suppression = WriteSuppressionRegistry::new();
    let rescan_lock = RescanLock::default();
    let background = Arc::new(BackgroundTasks::default());

    sqlx::query(
        "INSERT INTO daily_notes_workspace (id, title, content, created_at, updated_at) \
         VALUES ('n1', 'T1', 'body', '2026-11-19T00:00:00Z', '2099-01-01T00:00:00Z')",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("UPDATE vault_write_outbox SET mirror_complete = 1 WHERE note_id = 'n1'")
        .execute(&pool)
        .await
        .unwrap();

    std::fs::write(notes_dir.join("n1.md"), note("n1")).unwrap();
    std::fs::write(
        notes_dir.join("n1.replaced-20261122-150405.md"),
        "pasted over without frontmatter",
    )
    .unwrap();
    std::fs::write(
        notes_dir.join("n2.replaced-20261122-150405-2.md"),
        note("deleted-note"),
    )
    .unwrap();
    std::fs::write(notes_dir.join("scratch.md"), "# Just markdown\n").unwrap();

    for pass in 0..2 {
        let summary = rescan_vault(
            db.clone(),
            dir.path().to_path_buf(),
            suppression.clone(),
            &rescan_lock,
            app_handle.clone(),
            Arc::clone(&background),
            None,
        )
        .await
        .unwrap();
        assert_eq!(summary.scanned, 2, "pass {pass}: set-aside files excluded");
        assert_eq!(
            summary.imported, 0,
            "pass {pass}: untracked and set-aside files must not increment imported"
        );
        assert_eq!(summary.deleted, 0, "pass {pass}");
    }

    let ids: Vec<String> = sqlx::query_scalar("SELECT id FROM daily_notes_workspace ORDER BY id")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(ids, vec!["n1".to_string()]);

    background.close();
    tokio::time::timeout(Duration::from_secs(2), background.wait())
        .await
        .unwrap();
}
