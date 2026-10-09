//! Storage roots Lattice keeps outside the app data directory.
//!
//! Everything else lives under the platform app data directory that Tauri
//! resolves (`lattice.db`, search indexes, `folder-index/`, `learning-packs/`).
//! The two roots here hold user content that a backup must carry, so every
//! owner and the backup writer resolve them from this one place.

use std::path::PathBuf;

use crate::shared::error::{AppError, Result};

/// `~/.lattice`, the parent of every root below.
fn lattice_home() -> Result<PathBuf> {
    let home = dirs::home_dir()
        .ok_or_else(|| AppError::InvalidState("Cannot determine home directory".to_string()))?;
    Ok(home.join(".lattice"))
}

/// Content-addressed copies of imported files: `~/.lattice/files/{sha256}/{name}`.
pub fn library_root() -> Result<PathBuf> {
    Ok(lattice_home()?.join("files"))
}

/// Archived web pages that web documents and citations open: `~/.lattice/web-archive`.
pub fn web_archive_root() -> Result<PathBuf> {
    Ok(lattice_home()?.join("web-archive"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_roots_share_the_lattice_home() {
        let library = library_root().expect("home directory");
        let archive = web_archive_root().expect("home directory");
        assert_eq!(library.parent(), archive.parent());
        assert!(library.ends_with(".lattice/files"));
        assert!(archive.ends_with(".lattice/web-archive"));
    }
}
