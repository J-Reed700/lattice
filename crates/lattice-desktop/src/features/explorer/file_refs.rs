//! Lines of a folder's file, pointed at from an Explorer reply as `[f2:10-20]`.
//!
//! A file the loop read is shown under a handle; the reply points at lines of
//! it with the handle, and the app reads the words from the folder itself.

use lattice_core::application::services::model_contract::reply::{
    Lines, Markers, Place, Reference,
};
use lattice_core::application::services::model_contract::shown::{Drawn, Kind};
use lattice_core::shared::{AppError, Result};

use super::fs;
use super::scope::Scope;

#[cfg(test)]
mod tests;

/// A file of the folder that a tool result showed, by its path from the folder root.
pub struct FolderFile(pub String);

impl Kind for FolderFile {
    const PREFIX: &'static str = "f";
    const DRAWN: Drawn = Drawn::Name;
    type Ref = String;

    fn reference(&self) -> String {
        self.0.clone()
    }
}

/// What an Explorer reply may point at.
pub fn markers() -> Markers {
    Markers::new().of_lines::<FolderFile>(Place::Inline)
}

/// The lines a reference names, read from the folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderLines {
    pub path: String,
    pub lines: Lines,
    pub text: String,
}

/// The lines a resolved reference names, as the folder holds them now. `None` for
/// a reference to a file without lines; an error when the file or those lines are not there.
pub fn lines_of(scope: &Scope, reference: &Reference) -> Result<Option<FolderLines>> {
    let Some(lines) = reference.lines else {
        return Ok(None);
    };
    let path: String = serde_json::from_value(reference.reference.clone()).map_err(|_| {
        AppError::InvalidInput("A file reference does not name a path.".to_string())
    })?;
    let mut text = String::new();
    let mut last_seen = 0u32;
    let total = fs::scan_lines(scope, &path, &mut |number, piece| {
        if number < lines.first {
            return true;
        }
        if number > lines.last {
            return false;
        }
        if number != last_seen && last_seen != 0 {
            text.push('\n');
        }
        last_seen = number;
        text.push_str(piece);
        true
    })?;
    if lines.last > total {
        return Err(AppError::InvalidInput(format!(
            "{path} has {total} lines; lines {}-{} are past the end.",
            lines.first, lines.last
        )));
    }
    Ok(Some(FolderLines { path, lines, text }))
}
