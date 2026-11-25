//! A folder the app keeps for itself, and whether a path is inside it.

use std::path::{Path, PathBuf};

use crate::shared::fs::confinement::resolve_against_existing_ancestor;
use crate::shared::path_safety::reject_parent_components;

/// A folder the app keeps for itself. It answers whether a path is inside
/// it however either is spelled: as configured, through a link or a short
/// name, or with a Windows verbatim prefix. It answers before the folder exists.
#[derive(Debug, Clone)]
pub struct OwnedFolder {
    spellings: Vec<PathBuf>,
    resolved: PathBuf,
}

impl OwnedFolder {
    pub fn new(folder: &Path) -> Self {
        let resolved =
            resolve_against_existing_ancestor(folder).unwrap_or_else(|_| folder.to_path_buf());
        let mut spellings = vec![folder.to_path_buf()];
        for spelling in [without_verbatim_prefix(&resolved), resolved.clone()] {
            if !spellings.contains(&spelling) {
                spellings.push(spelling);
            }
        }
        Self {
            spellings,
            resolved,
        }
    }

    /// True when `path` has no parent component and is inside the folder.
    pub fn holds(&self, path: &Path) -> bool {
        if reject_parent_components(path).is_err() {
            return false;
        }
        self.spellings.iter().any(|folder| path.starts_with(folder))
            || resolve_against_existing_ancestor(path)
                .is_ok_and(|resolved| resolved.starts_with(&self.resolved))
    }
}

fn without_verbatim_prefix(path: &Path) -> PathBuf {
    use std::path::{Component, Prefix};
    let mut components = path.components();
    match components.next() {
        Some(Component::Prefix(prefix)) => match prefix.kind() {
            Prefix::VerbatimDisk(letter) => {
                let mut plain = PathBuf::from(format!("{}:", char::from(letter)));
                plain.extend(components);
                plain
            }
            _ => path.to_path_buf(),
        },
        _ => path.to_path_buf(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn holds_its_paths_however_they_are_spelled() {
        let temp = tempfile::tempdir().expect("temp dir");
        let folder = temp.path().join("library");
        let path = folder.join("blob").join("paper.pdf");
        assert!(
            OwnedFolder::new(&folder).holds(&path),
            "before the folder exists"
        );

        std::fs::create_dir_all(path.parent().expect("parent")).expect("folders");
        std::fs::write(&path, "body").expect("file");
        let canonical_folder = std::fs::canonicalize(&folder).expect("canonical folder");
        let canonical_path = std::fs::canonicalize(&path).expect("canonical path");
        assert!(OwnedFolder::new(&folder).holds(&canonical_path));
        assert!(OwnedFolder::new(&canonical_folder).holds(&path));
        assert!(!OwnedFolder::new(&folder).holds(&folder.join("..").join("escape.txt")));
        assert!(!OwnedFolder::new(&folder).holds(&temp.path().join("elsewhere").join("paper.pdf")));
    }
}
