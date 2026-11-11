//! The changes between two drafts of a program, line by line.

use crate::practice::coding::observation::{changes_since, LineChange, LineChangeKind};
use crate::practice::kinds::program::parts::part_of;
use crate::practice::kinds::program::shape::files_of;
use crate::practice::session::kind::Diff;
use lattice_core::domain::learning::practice::work::{Brief, Change, ChangeKind, Piece};
use lattice_core::shared::error::Result;

#[cfg(test)]
mod tests;

pub struct ProgramDiff;

fn change_of(change: &LineChange) -> Change {
    Change {
        part: part_of(&change.path, change.line),
        kind: match change.kind {
            LineChangeKind::Added => ChangeKind::Added,
            LineChangeKind::Removed => ChangeKind::Removed,
        },
        text: change.text.clone(),
    }
}

impl Diff for ProgramDiff {
    fn changes(&self, _brief: &Brief, old: &Piece, new: &Piece) -> Result<Vec<Change>> {
        let changes = changes_since(&files_of(new)?, &files_of(old)?);
        Ok(changes.iter().map(change_of).collect())
    }
}
