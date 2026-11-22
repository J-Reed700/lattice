use super::*;
use sqlx::SqlitePool;

#[cfg(windows)]
#[test]
fn a_note_id_windows_reads_as_a_parent_or_a_drive_is_not_safe() {
    for id in ["...", ".. ", "C:x"] {
        assert!(!safe_note_id(id), "{id:?}");
    }
}

#[test]
fn non_empty_write_is_indexed() {
    assert_eq!(
        note_index_action(NoteSyncOutcome::Written {
            body: "Read three papers on retrieval today."
        }),
        NoteIndexAction::Index
    );
}

#[test]
fn empty_write_removes_from_index() {
    assert_eq!(
        note_index_action(NoteSyncOutcome::Written { body: "" }),
        NoteIndexAction::Remove
    );
}

#[test]
fn whitespace_only_write_removes_from_index() {
    assert_eq!(
        note_index_action(NoteSyncOutcome::Written {
            body: "   \n\t  \r\n "
        }),
        NoteIndexAction::Remove
    );
}

#[test]
fn delete_removes_from_the_index() {
    assert_eq!(
        note_index_action(NoteSyncOutcome::Deleted),
        NoteIndexAction::Remove
    );
}

#[test]
fn frontmatter_with_tags_roundtrips() {
    let fm = build_frontmatter(
        "abc-123",
        "Hello world",
        "2026-05-01T18:00:00Z",
        "2026-05-01T18:05:00Z",
        &["foo".to_string(), "bar".to_string()],
    );
    assert!(fm.starts_with("---\n"));
    assert!(fm.contains("id: abc-123"));
    assert!(fm.contains("title: \"Hello world\""));
    assert!(fm.contains("tags: [\"foo\", \"bar\"]"));
    assert!(fm.ends_with("---"));
}

#[test]
fn frontmatter_escapes_quotes_in_title() {
    let fm = build_frontmatter("id1", "She said \"hi\"", "now", "now", &[]);
    assert!(fm.contains("title: \"She said \\\"hi\\\"\""));
}

#[test]
fn frontmatter_escapes_newline_in_title() {
    let fm = build_frontmatter("id1", "Line one\nLine two", "now", "now", &[]);
    assert!(fm.contains("title: \"Line one\\nLine two\""));
    let title_line = fm.lines().find(|l| l.starts_with("title:")).unwrap();
    assert!(!title_line.contains('\n'));
}

#[test]
fn frontmatter_strips_carriage_returns() {
    let fm = build_frontmatter("id1", "Windows\r\nlineending", "now", "now", &[]);
    assert!(fm.contains("title: \"Windows\\nlineending\""));
}

#[test]
fn empty_tags_render_as_empty_array() {
    let fm = build_frontmatter("id", "title", "now", "now", &[]);
    assert!(fm.contains("tags: []"));
}

#[tokio::test]
async fn atomic_write_creates_parent_dirs_and_file() {
    let tmp = tempfile::tempdir().unwrap();
    let target = tmp.path().join("nested/dir/file.md");
    atomic_write(&target, "hello").await.unwrap();
    let read = tokio::fs::read_to_string(&target).await.unwrap();
    assert_eq!(read, "hello");
}

#[tokio::test]
async fn atomic_write_handles_concurrent_writers() {
    let tmp = tempfile::tempdir().unwrap();
    let target = tmp.path().join("note.md");

    let target_a = target.clone();
    let target_b = target.clone();
    let a = tokio::spawn(async move { atomic_write(&target_a, "version A").await });
    let b = tokio::spawn(async move { atomic_write(&target_b, "version B").await });
    a.await.unwrap().unwrap();
    b.await.unwrap().unwrap();

    let final_contents = tokio::fs::read_to_string(&target).await.unwrap();
    assert!(
        final_contents == "version A" || final_contents == "version B",
        "final file must be one of the inputs verbatim, got: {:?}",
        final_contents
    );

    let mut entries = tokio::fs::read_dir(tmp.path()).await.unwrap();
    let mut count = 0;
    while let Some(entry) = entries.next_entry().await.unwrap() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        assert!(
            !name.contains(".tmp"),
            "unexpected leftover tmp file: {}",
            name
        );
        count += 1;
    }
    assert_eq!(count, 1);
}

#[tokio::test]
async fn worker_preserves_submit_order_for_same_id() {
    let tmp = tempfile::tempdir().unwrap();
    let vault_root = tmp.path().to_path_buf();

    let (tx, mut rx) = mpsc::unbounded_channel::<(String, String)>();
    let worker_root = vault_root.clone();
    let worker = tokio::spawn(async move {
        while let Some((id, body)) = rx.recv().await {
            let target = worker_root.join("notes").join(format!("{}.md", id));
            atomic_write(&target, &body).await.unwrap();
        }
    });

    let id = "ordering-test".to_string();
    for n in 0..50 {
        tx.send((id.clone(), format!("version-{n}"))).unwrap();
    }
    drop(tx);
    worker.await.unwrap();

    let final_contents =
        tokio::fs::read_to_string(vault_root.join("notes").join("ordering-test.md"))
            .await
            .unwrap();
    assert_eq!(
        final_contents, "version-49",
        "FIFO worker must end at the last submitted version"
    );
}

#[tokio::test]
async fn failed_index_action_keeps_the_durable_write_for_retry() {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
    let db = Database::new(pool.clone());
    sqlx::query(
        "CREATE TABLE daily_notes_workspace(id TEXT PRIMARY KEY, title TEXT, content TEXT)",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::raw_sql(include_str!(
        "../../../migrations/20260927000000_vault_write_outbox.sql"
    ))
    .execute(&pool)
    .await
    .unwrap();
    sqlx::raw_sql(include_str!(
        "../../../migrations/20261119000000_vault_note_index_and_mirror_state.sql"
    ))
    .execute(&pool)
    .await
    .unwrap();
    sqlx::raw_sql(include_str!(
        "../../../migrations/20261121010000_vault_mirror_writing.sql"
    ))
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO daily_notes_workspace VALUES ('note', 'Title', 'body')")
        .execute(&pool)
        .await
        .unwrap();
    let write = super::super::repository::pending(&db)
        .await
        .unwrap()
        .remove(0);

    super::super::repository::mark_mirrored(&db, &write, Some("sha"))
        .await
        .unwrap();
    assert!(
        settle_write(&db, &write, Err("simulated index failure".into()))
            .await
            .is_err()
    );
    assert!(!super::super::repository::has_pending(&db, "note")
        .await
        .unwrap());
    assert_eq!(
        super::super::repository::pending(&db).await.unwrap().len(),
        0
    );
    sqlx::query("UPDATE daily_notes_workspace SET content = 'new edit' WHERE id = 'note'")
        .execute(&pool)
        .await
        .unwrap();
    assert!(super::super::repository::has_pending(&db, "note")
        .await
        .unwrap());
    assert_eq!(
        super::super::repository::pending(&db).await.unwrap().len(),
        1
    );
}

async fn own_write_fixture() -> (
    SqlitePool,
    Database,
    tempfile::TempDir,
    std::path::PathBuf,
    super::super::watcher::WriteSuppressionRegistry,
    Arc<std::sync::RwLock<Option<tauri::AppHandle>>>,
) {
    let pool = lattice_core::shared::testing::sqlite::migrated_in_memory()
        .await
        .unwrap();
    let db = Database::new(pool.clone());
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("notes/n1.md");
    let suppression = super::super::watcher::WriteSuppressionRegistry::new();
    let app_handle = Arc::new(std::sync::RwLock::new(None));
    sqlx::query(
        "INSERT INTO daily_notes_workspace (id, title, content, created_at, updated_at) \
         VALUES ('n1', 'T1', 'first', '2026-11-19T00:00:00Z', '2026-11-19T00:01:00Z')",
    )
    .execute(&pool)
    .await
    .unwrap();
    (pool, db, dir, target, suppression, app_handle)
}

async fn pass_over_n1(
    db: &Database,
    suppression: &super::super::watcher::WriteSuppressionRegistry,
    app_handle: &Arc<std::sync::RwLock<Option<tauri::AppHandle>>>,
    target: &Path,
) -> Option<String> {
    let write = super::super::repository::pending(db)
        .await
        .unwrap()
        .remove(0);
    let note = super::super::repository::workspace_notes(db)
        .get("n1")
        .await
        .unwrap();
    let outside = mirror_note_to_disk(db, suppression, app_handle, &write, &note, target)
        .await
        .unwrap();
    settle_write(db, &write, Ok(())).await.unwrap();
    outside
}

#[tokio::test]
async fn a_save_during_a_write_keeps_the_newer_text() {
    let (pool, db, _dir, target, suppression, app_handle) = own_write_fixture().await;
    pass_over_n1(&db, &suppression, &app_handle, &target).await;

    sqlx::query("UPDATE daily_notes_workspace SET content = 'second' WHERE id = 'n1'")
        .execute(&pool)
        .await
        .unwrap();
    let w2 = super::super::repository::pending(&db)
        .await
        .unwrap()
        .remove(0);
    let n2 = super::super::repository::workspace_notes(&db)
        .get("n1")
        .await
        .unwrap();
    sqlx::query("UPDATE daily_notes_workspace SET content = 'third' WHERE id = 'n1'")
        .execute(&pool)
        .await
        .unwrap();
    mirror_note_to_disk(&db, &suppression, &app_handle, &w2, &n2, &target)
        .await
        .unwrap();
    settle_write(&db, &w2, Ok(())).await.unwrap();

    let outside = pass_over_n1(&db, &suppression, &app_handle, &target).await;

    let note = super::super::repository::workspace_notes(&db)
        .get("n1")
        .await
        .unwrap();
    assert_eq!(note.content, "third", "the page must keep the newer save");
    let on_disk = tokio::fs::read_to_string(&target).await.unwrap();
    assert!(on_disk.contains("third"));
    assert_eq!(outside, None);
    assert!(super::super::repository::pending(&db)
        .await
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn a_write_whose_baseline_did_not_commit_is_still_lattices_own() {
    let (pool, db, _dir, target, suppression, app_handle) = own_write_fixture().await;
    pass_over_n1(&db, &suppression, &app_handle, &target).await;

    sqlx::query("UPDATE daily_notes_workspace SET content = 'second' WHERE id = 'n1'")
        .execute(&pool)
        .await
        .unwrap();
    let w2 = super::super::repository::pending(&db)
        .await
        .unwrap()
        .remove(0);
    let n2 = super::super::repository::workspace_notes(&db)
        .get("n1")
        .await
        .unwrap();
    sqlx::query(
        "CREATE TRIGGER fail_mirror_record BEFORE UPDATE ON vault_write_outbox \
         BEGIN SELECT RAISE(ABORT, 'injected'); END",
    )
    .execute(&pool)
    .await
    .unwrap();
    assert!(
        mirror_note_to_disk(&db, &suppression, &app_handle, &w2, &n2, &target)
            .await
            .is_err()
    );
    let on_disk = tokio::fs::read_to_string(&target).await.unwrap();
    assert!(on_disk.contains("second"));
    sqlx::query("DROP TRIGGER fail_mirror_record")
        .execute(&pool)
        .await
        .unwrap();

    sqlx::query("UPDATE daily_notes_workspace SET content = 'third' WHERE id = 'n1'")
        .execute(&pool)
        .await
        .unwrap();
    pass_over_n1(&db, &suppression, &app_handle, &target).await;

    let note = super::super::repository::workspace_notes(&db)
        .get("n1")
        .await
        .unwrap();
    assert_eq!(note.content, "third");
    let on_disk = tokio::fs::read_to_string(&target).await.unwrap();
    assert!(on_disk.contains("third"));
}

#[tokio::test]
async fn mirror_preserves_existing_frontmatter_tags_and_skips_clobbering_external_edits() {
    let pool = lattice_core::shared::testing::sqlite::migrated_in_memory()
        .await
        .unwrap();
    let db = Database::new(pool.clone());
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("notes/n1.md");
    tokio::fs::create_dir_all(target.parent().unwrap())
        .await
        .unwrap();

    let initial_disk = "---\nid: n1\ntitle: \"T1\"\ncreated_at: 2026-11-19T00:00:00Z\nupdated_at: 2026-11-19T00:00:00Z\ntags: [\"research\", \"ai\"]\n---\n\nold body";
    tokio::fs::write(&target, initial_disk).await.unwrap();
    let initial_sha = lattice_core::shared::hash::sha256_hex(initial_disk.as_bytes());

    sqlx::query(
        "INSERT INTO daily_notes_workspace (id, title, content, created_at, updated_at) \
         VALUES ('n1', 'T1', 'new body from app', '2026-11-19T00:00:00Z', '2026-11-19T00:01:00Z')",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO vault_mirror_state (note_id, last_mirrored_sha256) VALUES ('n1', ?)")
        .bind(&initial_sha)
        .execute(&pool)
        .await
        .unwrap();

    let write = super::super::repository::pending(&db)
        .await
        .unwrap()
        .remove(0);
    let note = super::super::repository::workspace_notes(&db)
        .get("n1")
        .await
        .unwrap();
    let suppression = super::super::watcher::WriteSuppressionRegistry::new();
    let app_handle = Arc::new(std::sync::RwLock::new(None));

    // Matching SHA-256: mirrors new body AND preserves external tags!
    let preserved = mirror_note_to_disk(&db, &suppression, &app_handle, &write, &note, &target)
        .await
        .unwrap();
    assert_eq!(preserved, None);
    let on_disk = tokio::fs::read_to_string(&target).await.unwrap();
    assert!(on_disk.contains("tags: [\"research\", \"ai\"]"));
    assert!(on_disk.contains("new body from app"));

    // Settle the first write (deleting the outbox row) to match production
    // flow: the persisted SHA-256 baseline must survive acknowledge().
    settle_write(&db, &write, Ok(())).await.unwrap();
    assert!(super::super::repository::pending(&db)
        .await
        .unwrap()
        .is_empty());

    // Now simulate external edit on disk while a new outbox write is queued with stale expected sha.
    let external_disk = "---\nid: n1\ntitle: \"T1\"\ncreated_at: 2026-11-19T00:00:00Z\nupdated_at: 2026-11-19T00:05:00Z\ntags: [\"research\", \"ai\"]\n---\n\nobsidian edit v2";
    tokio::fs::write(&target, external_disk).await.unwrap();
    sqlx::query("UPDATE daily_notes_workspace SET content = 'stale app edit' WHERE id = 'n1'")
        .execute(&pool)
        .await
        .unwrap();
    let stale_write = super::super::repository::pending(&db)
        .await
        .unwrap()
        .remove(0);
    let stale_note = super::super::repository::workspace_notes(&db)
        .get("n1")
        .await
        .unwrap();

    let preserved = mirror_note_to_disk(
        &db,
        &suppression,
        &app_handle,
        &stale_write,
        &stale_note,
        &target,
    )
    .await
    .unwrap();
    assert_eq!(preserved.as_deref(), Some("obsidian edit v2"));
    let disk_after = tokio::fs::read_to_string(&target).await.unwrap();
    assert!(
        disk_after.contains("obsidian edit v2"),
        "external edit on disk must not be overwritten"
    );
    // SQLite must also converge to the preserved external content and settle cleanly.
    let converged_note = super::super::repository::workspace_notes(&db)
        .get("n1")
        .await
        .unwrap();
    assert_eq!(converged_note.content, "obsidian edit v2");
    settle_write(&db, &stale_write, Ok(())).await.unwrap();
    assert!(super::super::repository::pending(&db)
        .await
        .unwrap()
        .is_empty());
}

/// A note edited in its file and in Lattice before Lattice's write landed
/// keeps both: the note takes the file's text, and Lattice's text becomes a
/// copy in the same journal, queued to be written as its own file.
#[tokio::test]
async fn an_edit_on_both_sides_keeps_both_versions() {
    let (pool, db, _dir, target, suppression, app_handle) = own_write_fixture().await;
    pass_over_n1(&db, &suppression, &app_handle, &target).await;

    let outside = "---\nid: n1\ntitle: \"T1\"\ncreated_at: 2026-11-19T00:00:00Z\nupdated_at: 2026-11-19T00:05:00Z\ntags: []\n---\n\nedited outside";
    tokio::fs::write(&target, outside).await.unwrap();
    sqlx::query("UPDATE daily_notes_workspace SET content = 'edited in Lattice' WHERE id = 'n1'")
        .execute(&pool)
        .await
        .unwrap();

    pass_over_n1(&db, &suppression, &app_handle, &target).await;

    let notes: Vec<(String, String, String, Option<String>)> = sqlx::query_as(
        "SELECT id, title, content, journal_id FROM daily_notes_workspace \
         ORDER BY id = 'n1' DESC, id",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(notes.len(), 2, "{notes:?}");
    assert_eq!(notes[0].0, "n1");
    assert_eq!(notes[0].2, "edited outside");
    let copy = &notes[1];
    assert_eq!(copy.2, "edited in Lattice");
    assert!(copy.1.starts_with("T1 ("), "{copy:?}");
    assert_eq!(copy.3, notes[0].3);
    let on_disk = tokio::fs::read_to_string(&target).await.unwrap();
    assert!(on_disk.contains("edited outside"), "{on_disk}");
    let queued: Vec<String> = super::super::repository::pending(&db)
        .await
        .unwrap()
        .into_iter()
        .map(|write| write.note_id)
        .collect();
    assert_eq!(queued, [copy.0.clone()]);
}

/// A file replaced outside Lattice by something that is no longer this note
/// is set aside beside it, untouched, and the note is written back to its own
/// file.
#[tokio::test]
async fn a_file_that_is_no_longer_the_note_is_set_aside() {
    let (pool, db, dir, target, suppression, app_handle) = own_write_fixture().await;
    pass_over_n1(&db, &suppression, &app_handle, &target).await;

    let pasted = "pasted over, no front matter";
    tokio::fs::write(&target, pasted).await.unwrap();
    sqlx::query("UPDATE daily_notes_workspace SET content = 'edited in Lattice' WHERE id = 'n1'")
        .execute(&pool)
        .await
        .unwrap();

    pass_over_n1(&db, &suppression, &app_handle, &target).await;

    let on_disk = tokio::fs::read_to_string(&target).await.unwrap();
    assert!(on_disk.contains("edited in Lattice"), "{on_disk}");
    let mut kept = Vec::new();
    let mut entries = tokio::fs::read_dir(dir.path().join("notes")).await.unwrap();
    while let Some(entry) = entries.next_entry().await.unwrap() {
        let path = entry.path();
        if path != target && tokio::fs::read_to_string(&path).await.unwrap() == pasted {
            kept.push(path);
        }
    }
    assert_eq!(kept.len(), 1, "the replaced file is kept beside the note");
    let note = super::super::repository::workspace_notes(&db)
        .get("n1")
        .await
        .unwrap();
    assert_eq!(note.content, "edited in Lattice");
}

/// Lattice's write of one save lands but is never recorded, and the write of
/// the next save is begun but never lands. The file then holds Lattice's own
/// earlier text, not an outside edit, so the newer save is written over it
/// rather than reverted to it.
#[cfg(unix)]
#[tokio::test]
async fn an_unrecorded_write_of_lattices_own_is_not_an_outside_edit() {
    use std::os::unix::fs::PermissionsExt;

    let (pool, db, dir, target, suppression, app_handle) = own_write_fixture().await;
    pass_over_n1(&db, &suppression, &app_handle, &target).await;

    sqlx::query("UPDATE daily_notes_workspace SET content = 'second' WHERE id = 'n1'")
        .execute(&pool)
        .await
        .unwrap();
    let w2 = super::super::repository::pending(&db)
        .await
        .unwrap()
        .remove(0);
    let n2 = super::super::repository::workspace_notes(&db)
        .get("n1")
        .await
        .unwrap();
    sqlx::query(
        "CREATE TRIGGER fail_mirror_record BEFORE UPDATE ON vault_write_outbox \
         BEGIN SELECT RAISE(ABORT, 'injected'); END",
    )
    .execute(&pool)
    .await
    .unwrap();
    assert!(
        mirror_note_to_disk(&db, &suppression, &app_handle, &w2, &n2, &target)
            .await
            .is_err()
    );
    sqlx::query("DROP TRIGGER fail_mirror_record")
        .execute(&pool)
        .await
        .unwrap();
    assert!(tokio::fs::read_to_string(&target)
        .await
        .unwrap()
        .contains("second"));

    sqlx::query("UPDATE daily_notes_workspace SET content = 'third' WHERE id = 'n1'")
        .execute(&pool)
        .await
        .unwrap();
    let w3 = super::super::repository::pending(&db)
        .await
        .unwrap()
        .remove(0);
    let n3 = super::super::repository::workspace_notes(&db)
        .get("n1")
        .await
        .unwrap();
    let notes_dir = dir.path().join("notes");
    std::fs::set_permissions(&notes_dir, std::fs::Permissions::from_mode(0o555)).unwrap();
    let landed = mirror_note_to_disk(&db, &suppression, &app_handle, &w3, &n3, &target).await;
    std::fs::set_permissions(&notes_dir, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(landed.is_err(), "the write of 'third' must not land");

    pass_over_n1(&db, &suppression, &app_handle, &target).await;

    let notes: Vec<(String, String)> =
        sqlx::query_as("SELECT id, content FROM daily_notes_workspace ORDER BY id")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(notes, [("n1".to_string(), "third".to_string())]);
    assert!(tokio::fs::read_to_string(&target)
        .await
        .unwrap()
        .contains("third"));
}
