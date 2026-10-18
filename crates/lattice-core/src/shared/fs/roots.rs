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

/// Frozen copies of cited web pages: `~/.lattice/web-snapshots/<id>.html`.
pub fn web_snapshots_root() -> Result<PathBuf> {
    Ok(lattice_home()?.join("web-snapshots"))
}

/// The markdown vault: the configured path, or `<home>/Lattice` when the
/// setting is empty.
pub fn resolve_vault_root(configured: &str) -> Option<PathBuf> {
    let trimmed = configured.trim();
    if !trimmed.is_empty() {
        return Some(PathBuf::from(trimmed));
    }
    dirs::home_dir().map(|h| h.join("Lattice"))
}

/// Pictures assistant answers show inline: `~/.lattice/chat-images/<id>.<ext>`.
pub fn chat_images_root() -> Result<PathBuf> {
    Ok(lattice_home()?.join("chat-images"))
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

    #[test]
    fn resolve_vault_root_uses_configured_when_present() {
        let root = resolve_vault_root("/explicit/vault").unwrap();
        assert_eq!(root, PathBuf::from("/explicit/vault"));
    }

    #[test]
    fn resolve_vault_root_falls_back_to_default_on_empty_string() {
        let root = resolve_vault_root("");
        if let Some(p) = root {
            assert!(p.ends_with("Lattice"));
        }
    }

    #[test]
    fn resolve_vault_root_treats_whitespace_as_empty() {
        let root = resolve_vault_root("   ");
        if let Some(p) = root {
            assert!(p.ends_with("Lattice"));
        }
    }
}
