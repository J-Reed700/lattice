#![allow(clippy::indexing_slicing, clippy::unwrap_used, clippy::expect_used)]

use super::*;
use crate::lab_runtime::LearningLabFile;
use crate::practice::coding::proof::fakes::spec;
use crate::practice::kinds::program::shape::{brief_of, piece_of};
use lattice_core::shared::error::AppError;

fn file(path: &str, content: &str) -> LearningLabFile {
    LearningLabFile {
        path: path.into(),
        content: content.into(),
    }
}

#[test]
fn the_changes_are_the_line_changes_one_to_one() {
    let old = vec![file("solution.py", "a\nb\nc\n"), file("gone.py", "x\n")];
    let new = vec![file("solution.py", "a\nB\nc\nd\n"), file("new.py", "y\n")];
    let expected = changes_since(&new, &old);
    assert!(expected.len() >= 5);
    let changes = ProgramDiff
        .changes(
            &brief_of(&spec()).unwrap(),
            &piece_of(&old).unwrap(),
            &piece_of(&new).unwrap(),
        )
        .unwrap();
    assert_eq!(changes.len(), expected.len());
    for (change, line) in changes.iter().zip(&expected) {
        assert_eq!(change.part.as_str(), format!("{}:{}", line.path, line.line));
        assert_eq!(change.text, line.text);
        let kind = match line.kind {
            LineChangeKind::Added => ChangeKind::Added,
            LineChangeKind::Removed => ChangeKind::Removed,
        };
        assert_eq!(change.kind, kind);
    }
}

#[test]
fn identical_drafts_have_no_changes() {
    let piece = piece_of(&[file("solution.py", "a\n")]).unwrap();
    let changes = ProgramDiff
        .changes(&brief_of(&spec()).unwrap(), &piece, &piece)
        .unwrap();
    assert!(changes.is_empty());
}

#[test]
fn a_draft_that_is_not_files_is_invalid() {
    let bad = Piece::new(serde_json::json!(3));
    let result = ProgramDiff.changes(&brief_of(&spec()).unwrap(), &bad, &bad);
    assert!(matches!(result, Err(AppError::InvalidState(_))));
}
