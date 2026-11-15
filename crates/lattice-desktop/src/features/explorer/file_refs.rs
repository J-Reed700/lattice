//! Lines of a folder's file, pointed at from an Explorer reply as `[f2:10-20]`.
//!
//! A file the loop read is shown under a handle; the reply points at lines of
//! it with the handle, and the app reads the words from the folder itself.

use std::sync::Arc;

use lattice_core::application::services::model_contract::kept::TurnNames;
use lattice_core::application::services::model_contract::reply::{self, Lines, Piece, Reference};
use lattice_core::application::services::model_contract::shown::Kind;
use lattice_core::application::services::tool_handles::FolderFile;
use lattice_core::shared::{AppError, Result};
use once_cell::sync::Lazy;
use regex::{Captures, Regex};

use super::fs;
use super::scope::Scope;

#[cfg(test)]
mod tests;

/// The names a conversation gives the folder's files.
#[derive(Clone)]
pub struct FileNames(Arc<TurnNames>);

impl FileNames {
    pub fn new(names: Arc<TurnNames>) -> Self {
        Self(names)
    }

    /// The bracketed name `path` is shown under, given the first time it is shown: `[f2]`.
    pub fn name(&self, path: &str) -> Result<String> {
        Ok(reply::written(&[Piece::Reference(
            self.reference(path, None)?,
        )]))
    }

    /// The name of `path` with lines, as a reply writes it: `[f2:10-24]`, `[f2:12]` for one line.
    pub fn name_lines(&self, path: &str, first: u32, last: u32) -> Result<String> {
        Ok(reply::written(&[Piece::Reference(
            self.reference(path, Some(Lines { first, last }))?,
        )]))
    }

    /// Every file the conversation has named as (name, path), newest first.
    pub fn held(&self) -> Vec<(String, String)> {
        let mut held: Vec<(String, String)> = self.0.with(|scope| {
            scope
                .held::<FolderFile>()
                .into_iter()
                .map(|(handle, path)| (handle.bracketed(), path))
                .collect()
        });
        held.reverse();
        held
    }

    /// Keeps the names given since they were last kept.
    pub async fn keep(&self) {
        if let Err(error) = self.0.save().await {
            tracing::warn!(%error, "The folder chat's names could not be kept");
        }
    }

    fn reference(&self, path: &str, lines: Option<Lines>) -> Result<Reference> {
        let things = [FolderFile(path.to_owned())];
        self.0.with(|scope| {
            let shown = scope.request().show(&things)?;
            let handle = shown
                .handles()
                .first()
                .ok_or_else(|| AppError::InternalError("A file was shown under no name.".into()))?;
            Ok(Reference {
                prefix: FolderFile::PREFIX,
                number: handle.number(),
                reference: serde_json::Value::String(path.to_owned()),
                drawn: FolderFile::DRAWN,
                lines,
            })
        })
    }
}

static INLINE_CODE: Lazy<Option<Regex>> = Lazy::new(|| Regex::new(r"`([^`\n]+)`").ok());

static OLD_REFERENCE: Lazy<Option<Regex>> =
    Lazy::new(|| Regex::new(r"^([A-Za-z0-9_@.+\-/]+):(\d+)(?:[-–](\d+))?$").ok());

/// The path and lines of a span of inline code that is wholly the old grammar
/// `path:line` or `path:first-last`; the path has a `/` or a `.` in it.
fn read_old_reference(code: &str) -> Option<(&str, u32, u32)> {
    let captures = OLD_REFERENCE.as_ref()?.captures(code)?;
    let path = captures.get(1)?.as_str();
    if !path.contains(['/', '.']) {
        return None;
    }
    let first: u32 = captures.get(2)?.as_str().parse().ok()?;
    let last: u32 = match captures.get(3) {
        Some(last) => last.as_str().parse().ok()?,
        None => first,
    };
    (1..=last).contains(&first).then_some((path, first, last))
}

/// `reply` with each inline code span in the old grammar written as the name with
/// lines of its path; everything else, fenced blocks included, byte for byte.
pub fn rewrite_old_references(reply: &str, names: &FileNames) -> String {
    let Some(pattern) = INLINE_CODE.as_ref() else {
        return reply.to_owned();
    };
    let mut out = String::with_capacity(reply.len());
    let mut fence: Option<char> = None;
    for line in reply.split_inclusive('\n') {
        match (fence, fence_character(line)) {
            (Some(open), Some(found)) if open == found => {
                fence = None;
                out.push_str(line);
            }
            (Some(_), _) => out.push_str(line),
            (None, Some(found)) => {
                fence = Some(found);
                out.push_str(line);
            }
            (None, None) => out.push_str(&pattern.replace_all(line, |found: &Captures<'_>| {
                let whole = found.get(0).map_or("", |all| all.as_str());
                found
                    .get(1)
                    .and_then(|code| read_old_reference(code.as_str()))
                    .and_then(|(path, first, last)| names.name_lines(path, first, last).ok())
                    .unwrap_or_else(|| whole.to_owned())
            })),
        }
    }
    out
}

/// The character a line opens or closes a fenced block with, when it does.
fn fence_character(line: &str) -> Option<char> {
    let line = line.trim_start();
    ['`', '~']
        .into_iter()
        .find(|fence| line.starts_with(&fence.to_string().repeat(3)))
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
