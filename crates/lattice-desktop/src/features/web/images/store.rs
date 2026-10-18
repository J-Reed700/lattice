//! The files: `~/.lattice/chat-images/<id>.<ext>`, written through
//! `<id>.<ext>.part`, fsynced and renamed, 0600 on unix (§7.5). A copy of the
//! snapshot store with the extension carried, since a picture's format is
//! only known once its bytes are sniffed.
//!
//! Beside them, `prepared/<id>-<w>x<h>.<ext>`: a picture fitted to a model's
//! vision geometry and re-encoded (chat-vision §7). A cache: each file is
//! derived from `<id>.<ext>`, goes when that picture goes, and is rebuilt on
//! demand when missing.

use std::io::Write;
use std::path::{Path, PathBuf};

use super::is_allowed_extension;
use crate::features::web::snapshots::store::sha256_hex;
pub use crate::features::web::snapshots::store::{is_valid_id, new_id, Verified};
use lattice_core::shared::error::{AppError, Result};

/// Suffix of a file still being written, after its extension.
pub const PART_SUFFIX: &str = ".part";

/// The subdirectory of prepared pictures.
pub const PREPARED_DIR: &str = "prepared";

/// The extensions a prepared picture is written under.
const PREPARED_EXTENSIONS: &[&str] = &["jpg", "png"];

/// `(id, w, h, ext)` of a prepared file name the store writes, finished or
/// not.
pub fn recognised_prepared(name: &str) -> Option<(&str, u32, u32, &str)> {
    let name = name.strip_suffix(PART_SUFFIX).unwrap_or(name);
    let (stem, ext) = name.split_once('.')?;
    let (id, size) = stem.split_once('-')?;
    let (width, height) = size.split_once('x')?;
    let (width, height) = (width.parse().ok()?, height.parse().ok()?);
    (is_valid_id(id) && PREPARED_EXTENSIONS.contains(&ext)).then_some((id, width, height, ext))
}

#[derive(Debug, Clone)]
pub struct ImageStore {
    root: PathBuf,
}

impl ImageStore {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    /// `~/.lattice/chat-images`, beside `web-snapshots`.
    pub fn default_root() -> Result<PathBuf> {
        lattice_core::shared::fs::roots::chat_images_root()
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The file for `id` and `ext`; `None` unless `id` is a valid id and
    /// `ext` an allowed extension, so no other path can ever be built from
    /// input.
    pub fn path_for(&self, id: &str, ext: &str) -> Option<PathBuf> {
        (is_valid_id(id) && is_allowed_extension(ext))
            .then(|| self.root.join(format!("{id}.{ext}")))
    }

    pub fn prepared_root(&self) -> PathBuf {
        self.root.join(PREPARED_DIR)
    }

    /// The prepared file for `id` at `width`×`height`; `None` unless `id` is
    /// a valid id and `ext` a prepared extension.
    pub fn prepared_path(&self, id: &str, width: u32, height: u32, ext: &str) -> Option<PathBuf> {
        (is_valid_id(id) && PREPARED_EXTENSIONS.contains(&ext)).then(|| {
            self.prepared_root()
                .join(format!("{id}-{width}x{height}.{ext}"))
        })
    }

    /// The prepared file for `id` at `width`×`height` in whichever format
    /// it was written, with its extension, if one exists.
    pub async fn read_prepared(
        &self,
        id: &str,
        width: u32,
        height: u32,
    ) -> Option<(&'static str, Vec<u8>)> {
        for ext in PREPARED_EXTENSIONS {
            let path = self.prepared_path(id, width, height, ext)?;
            if let Ok(bytes) = tokio::fs::read(&path).await {
                return Some((ext, bytes));
            }
        }
        None
    }

    /// Writes a prepared picture atomically, as the originals are written.
    pub async fn write_prepared(
        &self,
        id: &str,
        width: u32,
        height: u32,
        ext: &str,
        bytes: Vec<u8>,
    ) -> Result<()> {
        let path = self
            .prepared_path(id, width, height, ext)
            .ok_or_else(|| AppError::InvalidInput(format!("invalid prepared image {id}.{ext}")))?;
        let root = self.prepared_root();
        tokio::task::spawn_blocking(move || write_atomically(&root, &path, &bytes))
            .await
            .map_err(|e| AppError::Other(format!("prepared image write task failed: {e}")))?
            .map(|_| ())
    }

    /// Removes every prepared file of `id`; none existing is not an error.
    pub async fn remove_prepared(&self, id: &str) {
        let Ok(mut entries) = tokio::fs::read_dir(self.prepared_root()).await else {
            return;
        };
        while let Ok(Some(entry)) = entries.next_entry().await {
            let name = entry.file_name();
            let Some((owner, ..)) = name.to_str().and_then(recognised_prepared) else {
                continue;
            };
            if owner == id {
                if let Err(error) = tokio::fs::remove_file(entry.path()).await {
                    if error.kind() != std::io::ErrorKind::NotFound {
                        tracing::warn!(%error, image_id = id, "Could not remove a prepared chat image");
                    }
                }
            }
        }
    }

    /// Writes `bytes` atomically and returns their SHA-256 and size.
    pub async fn write(&self, id: &str, ext: &str, bytes: Vec<u8>) -> Result<(String, u64)> {
        let path = self
            .path_for(id, ext)
            .ok_or_else(|| AppError::InvalidInput(format!("invalid chat image {id}.{ext}")))?;
        let root = self.root.clone();
        tokio::task::spawn_blocking(move || write_atomically(&root, &path, &bytes))
            .await
            .map_err(|e| AppError::Other(format!("chat image write task failed: {e}")))?
    }

    /// Reads the file for `id`/`ext` and checks it against `expected` (hex
    /// SHA-256).
    pub async fn read_verified(&self, id: &str, ext: &str, expected: &str) -> Verified {
        let Some(path) = self.path_for(id, ext) else {
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

    /// Copies the file of `from` to `to` (same extension) through a `.part`
    /// file and a rename, so no reader sees half a copy. On APFS the copy is
    /// a clone.
    pub async fn copy(&self, from: &str, to: &str, ext: &str) -> Result<()> {
        let (Some(source), Some(target)) = (self.path_for(from, ext), self.path_for(to, ext))
        else {
            return Err(AppError::InvalidInput(format!(
                "invalid chat image copy {from}.{ext} -> {to}.{ext}"
            )));
        };
        tokio::task::spawn_blocking(move || {
            let mut part = target.as_os_str().to_owned();
            part.push(PART_SUFFIX);
            let part = PathBuf::from(part);
            let copied =
                std::fs::copy(&source, &part).and_then(|_| std::fs::rename(&part, &target));
            if copied.is_err() {
                let _ = std::fs::remove_file(&part);
            }
            copied
        })
        .await
        .map_err(|e| AppError::Other(format!("chat image copy task failed: {e}")))?
        .map_err(AppError::from)
    }

    /// Removes the file for `id`/`ext` and its prepared copies; a file
    /// already gone is not an error.
    pub async fn remove(&self, id: &str, ext: &str) {
        if let Some(path) = self.path_for(id, ext) {
            if let Err(error) = tokio::fs::remove_file(&path).await {
                if error.kind() != std::io::ErrorKind::NotFound {
                    tracing::warn!(%error, image_id = id, "Could not remove a chat image file");
                }
            }
        }
        self.remove_prepared(id).await;
    }
}

fn write_atomically(root: &Path, path: &Path, bytes: &[u8]) -> Result<(String, u64)> {
    std::fs::create_dir_all(root)?;
    let mut part = path.as_os_str().to_owned();
    part.push(PART_SUFFIX);
    let part = PathBuf::from(part);
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let result = (|| -> std::io::Result<()> {
        let mut file = options.open(&part)?;
        file.write_all(bytes)?;
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
    Ok((sha256_hex(bytes), bytes.len() as u64))
}
