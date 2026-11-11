//! Confining a caller-supplied path to a directory we control.
//!
//! Several Tauri commands accept a filesystem path from the webview. Rejecting
//! `..` components is not sufficient on its own, for three reasons that have
//! each produced a real bug in this codebase:
//!
//! 1. An **absolute** path contains no `..` at all, so a `..`-only filter lets
//!    `/Users/me/Library/LaunchAgents/x.plist` straight through.
//! 2. `Path::join` **discards the base** when the joined component is
//!    absolute, so building `root.join(user_input)` silently escapes the root.
//! 3. A **symlink** inside the root can point anywhere, so the check has to
//!    happen after resolution, not on the lexical path.
//!
//! `confine_to_root` handles all three. It resolves as much of the path as
//! exists (a destination file usually does not yet), then requires the result
//! to sit under the canonicalized root.

use std::path::{Path, PathBuf};

use crate::shared::error::AppError;
use crate::shared::path_safety::reject_parent_components;

/// Resolve `candidate` and require the result to live under `root`.
///
/// `root` must exist; it is canonicalized so that a symlinked or
/// non-normalized root still compares correctly. `candidate` need not exist —
/// its nearest existing ancestor is canonicalized and the remaining components
/// appended — which is the normal case for a download destination or an
/// export target.
///
/// # Errors
///
/// - `AppError::Security` if the resolved path escapes `root`
/// - `AppError::Security` if `candidate` has a parent component or a NUL byte
/// - `AppError::FileSystem` if `root` cannot be canonicalized
pub fn confine_to_root(root: &Path, candidate: &Path) -> Result<PathBuf, AppError> {
    reject_parent_components(candidate)
        .map_err(|reason| AppError::Security(format!("Path {reason}: {}", candidate.display())))?;

    let canonical_root = root.canonicalize().map_err(|e| {
        AppError::FileSystem(format!(
            "Cannot resolve allowed root {}: {}",
            root.display(),
            e
        ))
    })?;

    let resolved = resolve_against_existing_ancestor(candidate)?;

    if !resolved.starts_with(&canonical_root) {
        return Err(AppError::Security(format!(
            "Path escapes the allowed directory: {} is not inside {}",
            resolved.display(),
            canonical_root.display()
        )));
    }

    Ok(resolved)
}

/// Canonicalize the deepest existing ancestor of `path` and re-append the
/// components that don't exist yet.
///
/// Canonicalizing the whole path fails when the leaf is missing, and
/// canonicalizing nothing leaves symlinks unresolved — this does the useful
/// part of both.
fn resolve_against_existing_ancestor(path: &Path) -> Result<PathBuf, AppError> {
    if let Ok(canonical) = path.canonicalize() {
        return Ok(canonical);
    }

    let mut missing = Vec::new();
    let mut cursor = path;

    loop {
        match cursor.parent() {
            Some(parent) => {
                let name = cursor.file_name().ok_or_else(|| {
                    AppError::Security(format!("Path has no file name: {}", path.display()))
                })?;
                missing.push(name.to_os_string());

                if let Ok(canonical_parent) = parent.canonicalize() {
                    let mut resolved = canonical_parent;
                    for component in missing.iter().rev() {
                        resolved.push(component);
                    }
                    return Ok(resolved);
                }

                cursor = parent;
            }
            None => {
                return Err(AppError::Security(format!(
                    "Cannot resolve any part of path: {}",
                    path.display()
                )));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared::testing::links::{self, LinkKind};
    use tempfile::tempdir;

    #[test]
    fn allows_a_path_inside_the_root() {
        let root = tempdir().unwrap();
        let candidate = root.path().join("models").join("a.gguf");
        let confined = confine_to_root(root.path(), &candidate).expect("should be allowed");
        assert!(confined.starts_with(root.path().canonicalize().unwrap()));
    }

    #[test]
    fn allows_a_nonexistent_leaf() {
        // Download destinations don't exist yet; that must not be an error.
        let root = tempdir().unwrap();
        let candidate = root.path().join("deep").join("not").join("there.bin");
        assert!(confine_to_root(root.path(), &candidate).is_ok());
    }

    #[test]
    fn rejects_an_absolute_path_outside_the_root() {
        // The LaunchAgents case: no `..` anywhere, so a `..`-only filter
        // would have accepted this.
        let root = tempdir().unwrap();
        let outside = tempdir().unwrap();
        let candidate = outside.path().join("com.evil.plist");
        assert!(confine_to_root(root.path(), &candidate).is_err());
    }

    #[test]
    fn rejects_parent_traversal() {
        let root = tempdir().unwrap();
        let candidate = root.path().join("..").join("escaped.bin");
        assert!(confine_to_root(root.path(), &candidate).is_err());
    }

    #[test]
    fn rejects_a_nul_byte() {
        let root = tempdir().unwrap();
        let candidate = root.path().join("a\0b");
        assert!(confine_to_root(root.path(), &candidate).is_err());
    }

    #[test]
    fn rejects_a_backslash_separated_parent_on_every_platform() {
        let root = tempdir().unwrap();
        let candidate = root.path().join("a\\..\\..\\b");
        assert!(confine_to_root(root.path(), &candidate).is_err());
    }

    #[cfg(windows)]
    #[test]
    fn rejects_a_dots_only_segment_on_windows() {
        let root = tempdir().unwrap();
        let candidate = root.path().join("...").join("x");
        assert!(confine_to_root(root.path(), &candidate).is_err());
    }

    fn link_cannot_leave_the_root(kind: LinkKind) {
        let root = tempdir().unwrap();
        let outside = tempdir().unwrap();
        let link = root.path().join("escape");
        if !links::link_dir(kind, outside.path(), &link).unwrap() {
            return;
        }

        let candidate = link.join("payload.bin");
        assert!(
            confine_to_root(root.path(), &candidate).is_err(),
            "a {kind} to a directory must not be a way out of the root"
        );
    }

    #[test]
    fn rejects_a_symlink_pointing_outside_the_root() {
        link_cannot_leave_the_root(LinkKind::Symlink);
    }

    #[cfg(windows)]
    #[test]
    fn rejects_a_junction_pointing_outside_the_root() {
        link_cannot_leave_the_root(LinkKind::Junction);
    }
}
