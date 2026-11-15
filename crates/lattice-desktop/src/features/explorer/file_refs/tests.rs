use super::*;
use lattice_core::application::services::model_contract::reply::{split, Markers, Piece, Place};
use lattice_core::application::services::model_contract::shown::Scope as Shown;

fn folder() -> (tempfile::TempDir, Scope) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("kiln")).unwrap();
    std::fs::write(
        dir.path().join("kiln/tides.txt"),
        "high at six\nlow at noon\nhigh at six again\nlow at midnight\n",
    )
    .unwrap();
    let scope = Scope::open(dir.path().to_str().unwrap()).unwrap();
    (dir, scope)
}

fn markers() -> Markers {
    Markers::new().of_lines::<FolderFile>(Place::Inline)
}

fn shown() -> Shown {
    let mut scope = Shown::for_question();
    scope
        .request()
        .show(&[FolderFile("kiln/tides.txt".to_string())])
        .unwrap();
    scope
}

fn references(reply: &str) -> Vec<Reference> {
    split(&shown(), markers(), reply)
        .into_iter()
        .filter_map(|piece| match piece {
            Piece::Reference(reference) => Some(reference),
            Piece::Text(_) => None,
        })
        .collect()
}

#[test]
fn a_marker_reads_the_lines_from_the_folder() {
    let (_dir, scope) = folder();
    let found = references("The tide turns at [f1:2-3].");
    let lines = lines_of(&scope, &found[0]).unwrap().unwrap();
    assert_eq!(lines.path, "kiln/tides.txt");
    assert_eq!(lines.text, "low at noon\nhigh at six again");
}

#[test]
fn a_single_line_marker_reads_one_line() {
    let (_dir, scope) = folder();
    let found = references("See [f1:4].");
    let lines = lines_of(&scope, &found[0]).unwrap().unwrap();
    assert_eq!(lines.text, "low at midnight");
}

#[test]
fn a_marker_without_lines_has_nothing_to_read() {
    let (_dir, scope) = folder();
    let found = references("In [f1].");
    assert_eq!(lines_of(&scope, &found[0]).unwrap(), None);
}

#[test]
fn lines_past_the_end_are_refused() {
    let (_dir, scope) = folder();
    let found = references("See [f1:3-9].");
    assert!(lines_of(&scope, &found[0]).is_err());
}

#[test]
fn a_path_written_in_code_is_no_marker() {
    assert!(references("It is in `kiln/tides.txt:2-3`.").is_empty());
}

#[test]
fn a_file_that_was_not_shown_does_not_resolve() {
    assert!(references("See [f2:1-2].").is_empty());
}

fn names() -> FileNames {
    FileNames::new(Arc::new(TurnNames::unstored()))
}

#[test]
fn a_file_has_one_name_and_names_with_lines_are_written_as_a_reply_writes_them() {
    let names = names();
    assert_eq!(names.name("src/a.rs").unwrap(), "[f1]");
    assert_eq!(names.name("src/b.rs").unwrap(), "[f2]");
    assert_eq!(names.name("src/a.rs").unwrap(), "[f1]");
    assert_eq!(names.name_lines("src/b.rs", 10, 24).unwrap(), "[f2:10-24]");
    assert_eq!(names.name_lines("src/b.rs", 12, 12).unwrap(), "[f2:12]");
}

#[test]
fn held_lists_the_named_files_newest_first() {
    let names = names();
    names.name("src/a.rs").unwrap();
    names.name("src/b.rs").unwrap();
    assert_eq!(
        names.held(),
        vec![
            ("[f2]".to_string(), "src/b.rs".to_string()),
            ("[f1]".to_string(), "src/a.rs".to_string())
        ]
    );
}

#[test]
fn an_old_line_reference_in_code_becomes_the_name_with_lines() {
    let names = names();
    let old = "See `src/a.rs:3-5` and `src/a.rs:7`, also `kiln/b.txt:2–4`.";
    assert_eq!(
        rewrite_old_references(old, &names),
        "See [f1:3-5] and [f1:7], also [f2:2-4]."
    );
}

#[test]
fn other_text_is_left_byte_for_byte() {
    let names = names();
    let text = "Run `cargo build` first.
```
`src/a.rs:3`
src/a.rs:3
```
Plain src/a.rs:3 and `Option:2` and `src/a.rs:5-`.
";
    assert_eq!(rewrite_old_references(text, &names), text);
    assert!(names.held().is_empty());
}
