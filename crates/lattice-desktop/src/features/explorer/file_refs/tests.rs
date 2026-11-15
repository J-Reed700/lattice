use super::*;
use lattice_core::application::services::model_contract::reply::{split, Piece};
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
