//! The folder boundary every Explorer read goes through.
//!
//! A scope is a canonical directory. Paths from the frontend and from the
//! model are relative to it; each is joined under the root and canonicalised
//! again, and anything that lands outside (a `..`, an absolute path, a symlink
//! pointing out) is refused before a byte is read.

use crate::shared::{AppError, Result};
use std::path::{Component, Path, PathBuf};

#[derive(Debug, Clone)]
pub struct Scope {
    root: PathBuf,
}

/// A path inside a scope, in the three forms callers need.
#[derive(Debug, Clone)]
pub struct ScopedPath {
    /// Relative, `/`-separated, no leading `./`; `""` is the root.
    pub relative: String,
    /// The root joined with `relative`, before symlinks are followed. This is
    /// the path a walker sees, so ignore rules are matched against it.
    pub lexical: PathBuf,
    /// Where it really is; always inside the root.
    pub canonical: PathBuf,
}

impl Scope {
    /// Opens a scope at an existing directory, given as an absolute path.
    pub fn open(root: &str) -> Result<Self> {
        let trimmed = root.trim();
        let raw = Path::new(trimmed);
        if trimmed.is_empty() || !raw.is_absolute() {
            return Err(AppError::InvalidInput(
                "Choose a folder by its full path.".to_string(),
            ));
        }
        let canonical = std::fs::canonicalize(raw)
            .map_err(|_| AppError::NotFound(format!("Folder not found: {trimmed}")))?;
        if !canonical.is_dir() {
            return Err(AppError::InvalidInput(format!(
                "Not a folder: {}",
                canonical.display()
            )));
        }
        Ok(Self { root: canonical })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn root_string(&self) -> String {
        self.root.to_string_lossy().into_owned()
    }

    /// The folder's own name; the whole path for a filesystem root.
    pub fn name(&self) -> String {
        self.root
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.root_string())
    }

    /// Resolves a relative path to an existing file or directory in the scope.
    pub fn resolve(&self, relative: &str) -> Result<ScopedPath> {
        let mut parts: Vec<String> = Vec::new();
        for component in Path::new(relative.trim()).components() {
            match component {
                Component::Normal(part) => parts.push(part.to_string_lossy().into_owned()),
                Component::CurDir => {}
                Component::ParentDir => {
                    return Err(AppError::PermissionDenied(format!(
                        "`..` is not allowed in an Explorer path: {relative}"
                    )))
                }
                Component::RootDir | Component::Prefix(_) => {
                    return Err(AppError::PermissionDenied(format!(
                        "Explorer paths are relative to the folder, not absolute: {relative}"
                    )))
                }
            }
        }
        let relative = parts.join("/");
        let lexical = parts
            .iter()
            .fold(self.root.clone(), |path, part| path.join(part));
        let canonical = std::fs::canonicalize(&lexical)
            .map_err(|_| AppError::NotFound(format!("No such file or folder: {relative}")))?;
        if !canonical.starts_with(&self.root) {
            return Err(AppError::PermissionDenied(format!(
                "{relative} points outside the folder"
            )));
        }
        Ok(ScopedPath {
            relative,
            lexical,
            canonical,
        })
    }

    /// The scope-relative form of a path a walker produced under the root.
    pub fn relative_of(&self, path: &Path) -> Option<String> {
        let rest = path.strip_prefix(&self.root).ok()?;
        Some(
            rest.components()
                .map(|component| component.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("/"),
        )
    }
}
