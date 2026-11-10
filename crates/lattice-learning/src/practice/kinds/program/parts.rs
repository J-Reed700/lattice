//! The parts of a program: the lines of its files.

use crate::practice::kinds::program::shape::files_of;
use crate::practice::session::kind::Parts;
use lattice_core::domain::learning::practice::work::{Brief, PartRef, Piece};
use lattice_core::shared::error::{AppError, Result};

#[cfg(test)]
mod tests;

pub struct ProgramParts;

/// The part that is line `line` (1-based) of the file at `path`.
pub fn part_of(path: &str, line: u32) -> PartRef {
    PartRef::new(&format!("{path}:{line}"))
}

/// The path and the line (1-based) a part names.
pub fn line_of(part: &PartRef) -> Result<(String, u32)> {
    part.as_str()
        .rsplit_once(':')
        .and_then(|(path, line)| Some((path.to_owned(), line.parse().ok()?)))
        .ok_or_else(|| {
            AppError::InvalidState(format!(
                "{} does not name a line of a file as path:line.",
                part.as_str()
            ))
        })
}

impl Parts for ProgramParts {
    fn all(&self, _brief: &Brief, piece: &Piece) -> Result<Vec<PartRef>> {
        Ok(files_of(piece)?
            .iter()
            .flat_map(|file| {
                (1..=file.content.lines().count())
                    .filter_map(|line| u32::try_from(line).ok())
                    .map(|line| part_of(&file.path, line))
            })
            .collect())
    }

    fn holds(&self, _brief: &Brief, piece: &Piece, part: &PartRef) -> Result<bool> {
        let files = files_of(piece)?;
        let Ok((path, line)) = line_of(part) else {
            return Ok(false);
        };
        Ok(files.iter().any(|file| {
            file.path == path
                && line >= 1
                && usize::try_from(line).is_ok_and(|line| line <= file.content.lines().count())
        }))
    }

    fn name(&self, _brief: &Brief, _piece: &Piece, part: &PartRef) -> Result<String> {
        let (path, line) = line_of(part)?;
        Ok(format!("{path} line {line}"))
    }
}
