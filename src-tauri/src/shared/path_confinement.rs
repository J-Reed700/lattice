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

use std::path::{Component, Path, PathBuf};

use crate::shared::error::AppError;

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
/// - `AppError::Security` if `candidate` contains a `..` component
/// - `AppError::FileSystem` if `root` cannot be canonicalized
pub fn confine_to_root(root: &Path, candidate: &Path) -> Result<PathBuf, AppError> {
    // Reject `..` outright rather than trying to resolve it. A parent
    // reference that stays inside the root is legitimate but pointless here,
    // and allowing it means reasoning about symlink-vs-lexical resolution
    // order for no benefit.
    if candidate
        .components()
        .any(|c| matches!(c, Component::ParentDir))
    {
        return Err(AppError::Security(format!(
            "Path may not contain '..' components: {}",
            candidate.display()
        )));
    }

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

/// Reject a filename that is anything other than a single, plain name.
///
/// Used where a caller supplies a *file name* rather than a path — an
/// absolute string or one containing a separator would escape the directory
/// it is about to be joined onto.
pub fn validate_bare_filename(name: &str) -> Result<(), AppError> {
    if name.is_empty() {
        return Err(AppError::Security("Filename cannot be empty".to_string()));
    }

    if name.contains('/') || name.contains('\\') {
        return Err(AppError::Security(format!(
            "Filename may not contain path separators: {}",
            name
        )));
    }

    if Path::new(name).is_absolute() {
        return Err(AppError::Security(format!(
            "Filename may not be an absolute path: {}",
            name
        )));
    }

    if name == "." || name == ".." {
        return Err(AppError::Security(format!(
            "Filename may not be a directory reference: {}",
            name
        )));
    }

    if name.contains('\0') {
        return Err(AppError::Security(
            "Filename may not contain null bytes".to_string(),
        ));
    }

    Ok(())
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]
mod tests {
    use super::*;
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
    fn rejects_a_symlink_pointing_outside_the_root() {
        let root = tempdir().unwrap();
        let outside = tempdir().unwrap();
        let link = root.path().join("escape");

        #[cfg(unix)]
        std::os::unix::fs::symlink(outside.path(), &link).unwrap();
        #[cfg(windows)]
        std::os::windows::fs::symlink_dir(outside.path(), &link).unwrap();

        let candidate = link.join("payload.bin");
        assert!(
            confine_to_root(root.path(), &candidate).is_err(),
            "a symlinked directory must not be a way out of the root"
        );
    }

    #[test]
    fn bare_filename_accepts_plain_names() {
        assert!(validate_bare_filename("model.safetensors").is_ok());
        assert!(validate_bare_filename("model-00001-of-00002.bin").is_ok());
    }

    #[test]
    fn bare_filename_rejects_escapes() {
        // The SEC-2 case: an absolute filename makes `join` drop the base.
        assert!(validate_bare_filename("/Users/example/.zshenv").is_err());
        assert!(validate_bare_filename("../../.zshenv").is_err());
        assert!(validate_bare_filename("sub/dir.bin").is_err());
        assert!(validate_bare_filename("..").is_err());
        assert!(validate_bare_filename("").is_err());
    }
}
