#![allow(clippy::indexing_slicing, clippy::unwrap_used, clippy::expect_used)]

use super::*;
use crate::lab_runtime::LearningLabFile;
use crate::practice::coding::proof::fakes::spec;
use crate::practice::kinds::program::shape::{brief_of, piece_of};

fn file(path: &str, content: &str) -> LearningLabFile {
    LearningLabFile {
        path: path.into(),
        content: content.into(),
    }
}

fn piece() -> Piece {
    piece_of(&[
        file("solution.py", "one\ntwo\nthree\n"),
        file("src/util.py", "only"),
        file("empty.py", ""),
    ])
    .unwrap()
}

fn brief() -> Brief {
    brief_of(&spec()).unwrap()
}

#[test]
fn a_part_round_trips_through_its_text() {
    let part = part_of("src/util.py", 7);
    assert_eq!(part.as_str(), "src/util.py:7");
    assert_eq!(line_of(&part).unwrap(), ("src/util.py".to_owned(), 7));
    let drive = part_of("C:/x.py", 2);
    assert_eq!(line_of(&drive).unwrap(), ("C:/x.py".to_owned(), 2));
}

#[test]
fn a_part_without_a_line_is_invalid() {
    for text in ["solution.py", "solution.py:", "solution.py:two"] {
        assert!(
            matches!(line_of(&PartRef::new(text)), Err(AppError::InvalidState(_))),
            "{text}"
        );
    }
}

#[test]
fn all_is_every_line_of_every_file_in_file_order() {
    let all = ProgramParts.all(&brief(), &piece()).unwrap();
    let text: Vec<&str> = all.iter().map(PartRef::as_str).collect();
    assert_eq!(
        text,
        [
            "solution.py:1",
            "solution.py:2",
            "solution.py:3",
            "src/util.py:1"
        ]
    );
}

#[test]
fn a_piece_that_is_not_files_has_no_parts() {
    let piece = Piece::new(serde_json::json!("text"));
    assert!(matches!(
        ProgramParts.all(&brief(), &piece),
        Err(AppError::InvalidState(_))
    ));
}

#[test]
fn a_part_is_held_when_the_file_has_that_line() {
    let held = |path: &str, line: u32| {
        ProgramParts
            .holds(&brief(), &piece(), &part_of(path, line))
            .unwrap()
    };
    assert!(held("solution.py", 1));
    assert!(held("solution.py", 3));
    assert!(!held("solution.py", 0));
    assert!(!held("solution.py", 4));
    assert!(!held("missing.py", 1));
    assert!(!held("empty.py", 1));
    assert!(held("src/util.py", 1));
}

#[test]
fn a_part_that_names_no_line_is_not_held() {
    let held = ProgramParts
        .holds(&brief(), &piece(), &PartRef::new("solution.py"))
        .unwrap();
    assert!(!held);
}

#[test]
fn a_part_is_named_by_its_file_and_line() {
    let name = ProgramParts
        .name(&brief(), &piece(), &part_of("solution.py", 2))
        .unwrap();
    assert_eq!(name, "solution.py line 2");
    assert!(matches!(
        ProgramParts.name(&brief(), &piece(), &PartRef::new("solution.py")),
        Err(AppError::InvalidState(_))
    ));
}
