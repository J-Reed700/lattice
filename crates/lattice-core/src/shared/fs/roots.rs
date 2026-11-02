//! Storage roots Lattice keeps outside the app data directory.
//!
//! Everything else lives under the platform app data directory that Tauri
//! resolves (`lattice.db`, search indexes, `folder-index/`, `learning-packs/`).
//! The two roots here hold user content that a backup must carry, so every
//! owner and the backup writer resolve them from this one place.

use std::path::PathBuf;

use crate::shared::error::{AppError, Result};

/// When set and not empty, replaces `~/.lattice` as the parent of every root
/// below. A second copy of the app on one machine sets it to a folder of its
/// own, so neither collects the other's files.
pub const LATTICE_HOME_ENV: &str = "LATTICE_HOME";

/// `$LATTICE_HOME`, else `~/.lattice`: the parent of every root below.
fn lattice_home() -> Result<PathBuf> {
    if let Some(home) = std::env::var_os(LATTICE_HOME_ENV).filter(|value| !value.is_empty()) {
        return Ok(PathBuf::from(home));
    }
    let home = dirs::home_dir()
        .ok_or_else(|| AppError::InvalidState("Cannot determine home directory".to_string()))?;
    Ok(home.join(".lattice"))
}

/// Holds the id of the database that owns the files in the roots below:
/// `~/.lattice/owner`.
pub fn library_owner_file() -> Result<PathBuf> {
    Ok(lattice_home()?.join("owner"))
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
    use std::path::Path;

    /// Set on a child copy of this test binary so that [`print_roots`] runs.
    const PROBE: &str = "LATTICE_ROOTS_PROBE";

    /// What a child process resolved: the name of each root and where it is.
    /// The variable is set only on the child, never on the process running the
    /// tests, because another test reading a root would see it.
    fn roots_with(configured: Option<&str>) -> Vec<(String, PathBuf)> {
        let mut child = std::process::Command::new(std::env::current_exe().unwrap());
        child
            .args([
                "--exact",
                "shared::fs::roots::tests::print_roots",
                "--nocapture",
            ])
            .env(PROBE, "1");
        match configured {
            Some(value) => child.env(LATTICE_HOME_ENV, value),
            None => child.env_remove(LATTICE_HOME_ENV),
        };
        let output = child.output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter_map(|line| {
                let (_, fields) = line.split_once("ROOT\t")?;
                let (name, path) = fields.split_once('\t')?;
                Some((name.to_string(), PathBuf::from(path)))
            })
            .collect()
    }

    /// Prints every root for [`roots_with`]; does nothing in a normal run.
    #[test]
    fn print_roots() {
        if std::env::var_os(PROBE).is_none() {
            return;
        }
        for (name, root) in [
            ("home", lattice_home()),
            ("files", library_root()),
            ("web-archive", web_archive_root()),
            ("web-snapshots", web_snapshots_root()),
            ("chat-images", chat_images_root()),
            ("owner", library_owner_file()),
        ] {
            println!("ROOT\t{name}\t{}", root.unwrap().display());
        }
    }

    fn all_under(home: &Path) -> Vec<(String, PathBuf)> {
        let mut roots = vec![("home".to_string(), home.to_path_buf())];
        roots.extend(
            [
                "files",
                "web-archive",
                "web-snapshots",
                "chat-images",
                "owner",
            ]
            .map(|name| (name.to_string(), home.join(name))),
        );
        roots
    }

    #[test]
    fn lattice_home_moves_every_root_and_an_empty_value_is_ignored() {
        let dir = tempfile::tempdir().unwrap();
        let second = dir.path().join("second");
        assert_eq!(
            roots_with(Some(second.to_str().unwrap())),
            all_under(&second)
        );

        let default = dirs::home_dir().unwrap().join(".lattice");
        assert_eq!(roots_with(Some("")), all_under(&default));
        assert_eq!(roots_with(None), all_under(&default));
    }

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
