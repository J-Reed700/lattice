//! The check a drill's key passes before the drill is stored: its code and its
//! key agree, line for line.
//!
//! Design: docs/design/2026-11-04-code-review-drills.md §2, step 7.

use super::catalogue::pattern;
use super::drill::{Drill, DrillFile, Edit, KeyIssue, LineSpan, Origin};
use super::edits::{apply, replace, EditFault};

/// Why a key is refused. Issues are named by position in the key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyFault {
    /// The key holds no issue.
    EmptyKey,
    /// The clean files list a path twice.
    DuplicateCleanPath { path: String },
    /// The drill's files list a path twice.
    DuplicateDrillPath { path: String },
    /// The drill's files are not the clean files' paths in the same order.
    PathsDiffer,
    /// Two issues have one id.
    DuplicateIssueId { id: String },
    /// The issue's pattern is not in the catalogue.
    UnknownPattern { issue: usize },
    /// The issue's pattern does not apply to the drill's language.
    PatternNotForLanguage { issue: usize },
    /// The issue's fix does not apply alone to the drill's files. The fault's
    /// positions are those of a list of that one fix.
    FixDoesNotApply { issue: usize, fault: EditFault },
    /// The issue's span is not `expected`, the lines its fix's `before` text
    /// occupies in the drill.
    SpanMismatch { issue: usize, expected: LineSpan },
    /// The fixes of the planted issues do not apply together to the drill's
    /// files. The fault's positions are positions in the key.
    FixesDoNotApplyTogether { fault: EditFault },
    /// The fixes of the planted issues apply together, but the file at `path`
    /// does not come back as the clean file was.
    RestoreDiffers { path: String },
}

/// Whether a drill's key and its code agree with each other and with the
/// clean code the defects were planted into. A drill that fails is not stored.
///
/// The first fault is reported, in this order:
/// - an empty key;
/// - a path listed twice in `clean`, then in the drill's files, or the two
///   lists not having the same paths in the same order;
/// - two issues with one id;
/// - an issue whose pattern is not in the catalogue, or does not apply to the
///   drill's language;
/// - an issue whose fix does not apply alone to the drill's files;
/// - an issue whose span is not exactly the lines its fix's `before` occupies
///   in the drill, which is the span `apply` gives the edit that replaces
///   `before` with itself, so a span always derives from text;
/// - the fixes of the planted issues, replaced together, not giving back
///   `clean` exactly.
///
/// Each check runs over the whole key before the next begins. Issues found in
/// review get the checks on the issue alone and stay out of the restore, so
/// they may overlap a planted issue's span. The fixes are replaced without
/// `apply`'s rule that no two results share a line, because the lines they
/// land on are not key spans: a plant that split one clean line in two gives
/// two fixes that land on one.
///
/// A deletion must keep a neighbouring line in its `before` and its `after`: a
/// pure deletion's reverse has an empty `before`, so its fix does not apply
/// and the key is refused.
pub fn check_key(clean: &[DrillFile], drill: &Drill) -> Result<(), KeyFault> {
    if drill.key.is_empty() {
        return Err(KeyFault::EmptyKey);
    }
    check_paths(clean, &drill.files)?;
    check_ids(&drill.key)?;
    check_patterns(&drill.language, &drill.key)?;
    check_fixes(&drill.files, &drill.key)?;
    check_spans(&drill.files, &drill.key)?;
    check_restore(clean, drill)
}

fn check_paths(clean: &[DrillFile], files: &[DrillFile]) -> Result<(), KeyFault> {
    if let Some(path) = repeated_path(clean) {
        return Err(KeyFault::DuplicateCleanPath { path });
    }
    if let Some(path) = repeated_path(files) {
        return Err(KeyFault::DuplicateDrillPath { path });
    }
    let same = clean
        .iter()
        .map(|file| &file.path)
        .eq(files.iter().map(|file| &file.path));
    if same {
        Ok(())
    } else {
        Err(KeyFault::PathsDiffer)
    }
}

/// The first path that a later file has again.
fn repeated_path(files: &[DrillFile]) -> Option<String> {
    files.iter().enumerate().find_map(|(position, file)| {
        files
            .iter()
            .skip(position + 1)
            .any(|other| other.path == file.path)
            .then(|| file.path.clone())
    })
}

fn check_ids(key: &[KeyIssue]) -> Result<(), KeyFault> {
    for (position, issue) in key.iter().enumerate() {
        if key
            .iter()
            .skip(position + 1)
            .any(|other| other.id == issue.id)
        {
            return Err(KeyFault::DuplicateIssueId {
                id: issue.id.clone(),
            });
        }
    }
    Ok(())
}

fn check_patterns(language: &str, key: &[KeyIssue]) -> Result<(), KeyFault> {
    for (position, issue) in key.iter().enumerate() {
        match pattern(&issue.pattern) {
            None => return Err(KeyFault::UnknownPattern { issue: position }),
            Some(known) if !known.applies_to(language) => {
                return Err(KeyFault::PatternNotForLanguage { issue: position });
            }
            Some(_) => {}
        }
    }
    Ok(())
}

fn check_fixes(files: &[DrillFile], key: &[KeyIssue]) -> Result<(), KeyFault> {
    for (position, issue) in key.iter().enumerate() {
        apply(files, std::slice::from_ref(&issue.fix)).map_err(|fault| {
            KeyFault::FixDoesNotApply {
                issue: position,
                fault,
            }
        })?;
    }
    Ok(())
}

fn check_spans(files: &[DrillFile], key: &[KeyIssue]) -> Result<(), KeyFault> {
    for (position, issue) in key.iter().enumerate() {
        let expected =
            lines_of_before(files, &issue.fix).map_err(|fault| KeyFault::FixDoesNotApply {
                issue: position,
                fault,
            })?;
        if issue.span != expected {
            return Err(KeyFault::SpanMismatch {
                issue: position,
                expected,
            });
        }
    }
    Ok(())
}

/// The lines `fix.before` occupies in `files`: the span `apply` gives the edit
/// that replaces it with itself.
fn lines_of_before(files: &[DrillFile], fix: &Edit) -> Result<LineSpan, EditFault> {
    let unchanged = Edit {
        path: fix.path.clone(),
        before: fix.before.clone(),
        after: fix.before.clone(),
    };
    let (_, spans) = apply(files, &[unchanged])?;
    spans
        .into_iter()
        .next()
        .ok_or(EditFault::NotFound { edit: 0 })
}

fn check_restore(clean: &[DrillFile], drill: &Drill) -> Result<(), KeyFault> {
    let planted: Vec<(usize, &KeyIssue)> = drill
        .key
        .iter()
        .enumerate()
        .filter(|(_, issue)| issue.origin == Origin::Planted)
        .collect();
    let fixes: Vec<Edit> = planted.iter().map(|(_, issue)| issue.fix.clone()).collect();
    let (restored, _) = replace(&drill.files, &fixes).map_err(|fault| {
        let positions: Vec<usize> = planted.iter().map(|&(position, _)| position).collect();
        KeyFault::FixesDoNotApplyTogether {
            fault: in_key_positions(fault, &positions),
        }
    })?;
    let differing = restored
        .iter()
        .zip(clean)
        .find(|(got, wanted)| got.text != wanted.text);
    match differing {
        Some((file, _)) => Err(KeyFault::RestoreDiffers {
            path: file.path.clone(),
        }),
        None => Ok(()),
    }
}

/// `fault` with its positions in the list of the planted issues' fixes
/// replaced by the issues' positions in the key.
fn in_key_positions(fault: EditFault, positions: &[usize]) -> EditFault {
    let in_key = |edit: usize| positions.get(edit).copied().unwrap_or(edit);
    match fault {
        EditFault::UnknownFile { edit } => EditFault::UnknownFile { edit: in_key(edit) },
        EditFault::Empty { edit } => EditFault::Empty { edit: in_key(edit) },
        EditFault::NotFound { edit } => EditFault::NotFound { edit: in_key(edit) },
        EditFault::Ambiguous { edit, count } => EditFault::Ambiguous {
            edit: in_key(edit),
            count,
        },
        EditFault::Overlaps { first, second } => EditFault::Overlaps {
            first: in_key(first),
            second: in_key(second),
        },
    }
}
