use super::rescan::ghost_sweep_block_reason;
use super::*;

#[tokio::test]
async fn oversized_vault_note_is_rejected_without_reading_the_whole_file() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("oversized.md");
    let file = std::fs::File::create(&path).unwrap();
    file.set_len(MAX_NOTE_BYTES + 1024).unwrap();
    assert_eq!(
        read_note(&path).await.unwrap_err().kind(),
        std::io::ErrorKind::InvalidData
    );
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
