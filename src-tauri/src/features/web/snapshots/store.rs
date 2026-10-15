//! The files: `~/.lattice/web-snapshots/<id>.html`, written through
//! `<id>.html.part`, fsynced and renamed, 0600 on unix (§7).

use std::io::Write;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use lattice_core::shared::error::{AppError, Result};

/// Suffix of a finished snapshot file.
pub const HTML_SUFFIX: &str = ".html";
/// Suffix of a file still being written.
pub const PART_SUFFIX: &str = ".html.part";

/// Snapshot ids are `Uuid::new_v4().simple()`: 32 lowercase hex characters.
pub fn is_valid_id(id: &str) -> bool {
    id.len() == 32 && id.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

pub fn new_id() -> String {
    uuid::Uuid::new_v4().simple().to_string()
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

/// What reading a stored file against its recorded hash found.
#[derive(Debug)]
pub enum Verified {
    Intact(Vec<u8>),
    Mismatch,
    Missing,
}

#[derive(Debug, Clone)]
pub struct SnapshotStore {
    root: PathBuf,
}

impl SnapshotStore {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    /// `~/.lattice/web-snapshots`, beside `web-archive`.
    pub fn default_root() -> Result<PathBuf> {
        lattice_core::shared::fs::roots::web_snapshots_root()
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The file for `id`; `None` for anything that is not a valid id, so no
    /// other path can ever be built from input.
    pub fn path_for(&self, id: &str) -> Option<PathBuf> {
        is_valid_id(id).then(|| self.root.join(format!("{id}{HTML_SUFFIX}")))
    }

    /// Writes `html` atomically and returns its SHA-256 and size.
    pub async fn write(&self, id: &str, html: Vec<u8>) -> Result<(String, u64)> {
        let path = self
            .path_for(id)
            .ok_or_else(|| AppError::InvalidInput(format!("invalid snapshot id {id}")))?;
        let root = self.root.clone();
        tokio::task::spawn_blocking(move || write_atomically(&root, &path, &html))
            .await
            .map_err(|e| AppError::Other(format!("snapshot write task failed: {e}")))?
    }

    /// Reads the file for `id` and checks it against `expected` (hex SHA-256).
    pub async fn read_verified(&self, id: &str, expected: &str) -> Verified {
        let Some(path) = self.path_for(id) else {
            return Verified::Missing;
        };
        let expected = expected.to_ascii_lowercase();
        tokio::task::spawn_blocking(move || match std::fs::read(&path) {
            Ok(bytes) if sha256_hex(&bytes) == expected => Verified::Intact(bytes),
            Ok(_) => Verified::Mismatch,
            Err(_) => Verified::Missing,
        })
        .await
        .unwrap_or(Verified::Missing)
    }

    /// Whether the file for `id` still hashes to `expected`.
    pub async fn verify(&self, id: &str, expected: &str) -> bool {
        matches!(self.read_verified(id, expected).await, Verified::Intact(_))
    }

    pub async fn remove(&self, id: &str) {
        if let Some(path) = self.path_for(id) {
            let _ = tokio::fs::remove_file(path).await;
        }
    }
}

fn write_atomically(root: &Path, path: &Path, html: &[u8]) -> Result<(String, u64)> {
    std::fs::create_dir_all(root)?;
    let part = path.with_extension("html.part");
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let result = (|| -> std::io::Result<()> {
        let mut file = options.open(&part)?;
        file.write_all(html)?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&part, path)?;
        // Make the rename durable too; best effort where directories cannot
        // be opened.
        if let Ok(dir) = std::fs::File::open(root) {
            let _ = dir.sync_all();
        }
        Ok(())
    })();
    if let Err(error) = result {
        let _ = std::fs::remove_file(&part);
        return Err(error.into());
    }
    Ok((sha256_hex(html), html.len() as u64))
}
