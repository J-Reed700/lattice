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
