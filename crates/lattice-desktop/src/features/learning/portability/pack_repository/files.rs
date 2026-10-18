//! Where a pack may be read from or written to. The managed folder and the
//! user's chosen file or folder all go through here: names come from course
//! titles, never overwrite another pack unless the OS save dialog asked, and
//! nothing lands inside the app's own data directory.
use super::{invalid, io_error};
use async_trait::async_trait;
use lattice_core::shared::error::Result;
use std::path::{Path, PathBuf};

pub(crate) const PACK_EXTENSION: &str = "lattice-learning";
const MAX_STEM_BYTES: usize = 80;
const MAX_PACK_BYTES: u64 = 256 * 1024 * 1024;

/// Asks the user where a pack goes. Destinations outside the app's folder
/// come only from here, never from a path the renderer sends, so an export
/// cannot become an arbitrary write.
#[async_trait]
pub trait DestinationPicker: Send + Sync {
    /// The file for one course, offered under `suggested_file_name`. `None`
    /// when the user dismissed the dialog.
    async fn pick_file(&self, suggested_file_name: &str) -> Result<Option<PathBuf>>;
    /// The folder for every course. `None` when the user dismissed it.
    async fn pick_folder(&self) -> Result<Option<PathBuf>>;
}

/// `<title>.lattice-learning`, safe as a file name on every desktop OS.
pub(crate) fn pack_file_name(title: &str) -> String {
    let cleaned: String = title
        .chars()
        .filter(|c| {
            !c.is_control() && !matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|')
        })
        .collect();
    // A leading dot would hide the file; only dots would name the folder or
    // its parent.
    let mut stem = cleaned.trim().trim_start_matches('.').trim_start();
    if stem.len() > MAX_STEM_BYTES {
        let mut end = MAX_STEM_BYTES;
        while !stem.is_char_boundary(end) {
            end -= 1;
        }
        stem = stem.get(..end).unwrap_or_default().trim_end();
    }
    let stem = if stem.is_empty() || is_windows_device_name(stem) {
        "course"
    } else {
        stem
    };
    format!("{stem}.{PACK_EXTENSION}")
}

/// Windows refuses `CON`, `nul.txt`, `Com1.notes` and the like as file
/// names: the part before the first dot names a device.
fn is_windows_device_name(stem: &str) -> bool {
    let device = stem
        .split('.')
        .next()
        .unwrap_or_default()
        .trim_end()
        .to_ascii_uppercase();
    match device.as_bytes() {
        b"CON" | b"PRN" | b"AUX" | b"NUL" => true,
        [b'C', b'O', b'M', digit] | [b'L', b'P', b'T', digit] => (b'1'..=b'9').contains(digit),
        _ => false,
    }
}

/// Write `bytes` beside `path` and rename over it, so a reader never sees a
/// half-written pack. A failed write removes its temporary file: exports go
/// to folders the user picked, such as a nearly full USB drive.
pub(super) async fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let tmp = path.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
    let file = tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&tmp)
        .await
        .map_err(io_error)?;
    let written = write_then_rename(file, &tmp, path, bytes).await;
    if written.is_err() {
        if let Err(error) = tokio::fs::remove_file(&tmp).await {
            tracing::warn!(%error, path = %tmp.display(), "could not remove a partial course pack");
        }
    }
    written
}

async fn write_then_rename(
    mut file: tokio::fs::File,
    tmp: &Path,
    path: &Path,
    bytes: &[u8],
) -> Result<()> {
    use tokio::io::AsyncWriteExt;
    file.write_all(bytes).await.map_err(io_error)?;
    file.sync_all().await.map_err(io_error)?;
    drop(file);
    tokio::fs::rename(tmp, path).await.map_err(io_error)
}

fn has_pack_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case(PACK_EXTENSION))
}

/// The first of `name`, `name (2)`, `name (3)`, ... that does not exist yet.
/// Anything in the way that is not a regular file stops the export instead.
pub(super) async fn next_free_path(dir: &Path, file_name: &str) -> Result<PathBuf> {
    let file = Path::new(file_name);
    let stem = file
        .file_stem()
        .and_then(|stem| stem.to_str())
        .filter(|stem| !stem.is_empty())
        .ok_or_else(|| invalid("Choose a simple pack filename without a path."))?;
    for attempt in 1..=10_000u32 {
        let name = if attempt == 1 {
            format!("{stem}.{PACK_EXTENSION}")
        } else {
            format!("{stem} ({attempt}).{PACK_EXTENSION}")
        };
        let path = dir.join(name);
        match tokio::fs::symlink_metadata(&path).await {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(path),
            Err(error) => return Err(io_error(error)),
            Ok(meta) if meta.file_type().is_symlink() || !meta.is_file() => {
                return Err(invalid(format!(
                    "{} is in the way and is not a regular pack file.",
                    path.display()
                )));
            }
            Ok(_) => {}
        }
    }
    Err(invalid(
        "Too many packs share this name in the destination folder.",
    ))
}

async fn canonical_app_data(app_data_dir: &Path) -> PathBuf {
    tokio::fs::canonicalize(app_data_dir)
        .await
        .unwrap_or_else(|_| app_data_dir.to_path_buf())
}

async fn require_outside_app_data(
    app_data_dir: &Path,
    chosen: &Path,
    canonical: &Path,
) -> Result<()> {
    let app_data = canonical_app_data(app_data_dir).await;
    if chosen.starts_with(app_data_dir) || canonical.starts_with(&app_data) {
        return Err(invalid(
            "Choose a location outside Lattice's own data folder, such as Documents or a USB drive.",
        ));
    }
    Ok(())
}

/// The file picked in the native save dialog. The dialog already confirmed
/// replacing an existing file, so an existing pack is replaced; anything else
/// already at that path, or a pack the dialog never named because the
/// extension was added here, is refused rather than overwritten.
pub(super) async fn chosen_file(app_data_dir: &Path, chosen: &Path) -> Result<PathBuf> {
    let name = chosen
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.trim().is_empty())
        .ok_or_else(|| invalid("Choose a file name for the course pack."))?;
    let appended = !has_pack_extension(chosen);
    if appended && tokio::fs::symlink_metadata(chosen).await.is_ok() {
        return Err(invalid(format!(
            "{name} is not a .{PACK_EXTENSION} file; Lattice will not replace it."
        )));
    }
    let name = if appended {
        format!("{name}.{PACK_EXTENSION}")
    } else {
        name.to_owned()
    };
    let parent = chosen
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .ok_or_else(|| invalid("Choose a folder for the course pack."))?;
    let folder = tokio::fs::canonicalize(parent)
        .await
        .map_err(|_| invalid("The chosen folder does not exist."))?;
    if !tokio::fs::metadata(&folder)
        .await
        .map_err(io_error)?
        .is_dir()
    {
        return Err(invalid("The chosen folder is not a folder."));
    }
    require_outside_app_data(app_data_dir, parent, &folder).await?;
    let path = folder.join(&name);
    if let Ok(meta) = tokio::fs::symlink_metadata(&path).await {
        // The dialog asked about the name without the extension, never
        // about this file.
        if appended {
            return Err(invalid(format!(
                "{name} already exists in that folder. Choose another name, or pick that file to replace it."
            )));
        }
        if meta.file_type().is_symlink() || !meta.is_file() {
            return Err(invalid(
                "The chosen pack path is a link or a folder; choose a regular file.",
            ));
        }
    }
    Ok(path)
}

/// The folder picked for an export of every course.
pub(super) async fn chosen_folder(app_data_dir: &Path, chosen: &Path) -> Result<PathBuf> {
    let folder = tokio::fs::canonicalize(chosen)
        .await
        .map_err(|_| invalid("The chosen folder does not exist."))?;
    if !tokio::fs::metadata(&folder)
        .await
        .map_err(io_error)?
        .is_dir()
    {
        return Err(invalid("The chosen location is not a folder."));
    }
    require_outside_app_data(app_data_dir, chosen, &folder).await?;
    Ok(folder)
}

/// A fresh `<title>.lattice-learning` in the app's own pack folder,
/// suffixing ` (2)`, ` (3)`, ... on a name collision.
pub(super) async fn managed_path(managed: &Path, title: &str) -> Result<PathBuf> {
    tokio::fs::create_dir_all(managed).await.map_err(io_error)?;
    let root = tokio::fs::canonicalize(managed).await.map_err(io_error)?;
    let path = next_free_path(&root, &pack_file_name(title)).await?;
    if path.parent() != Some(root.as_path()) {
        return Err(invalid("Pack path escaped its managed directory."));
    }
    Ok(path)
}

/// A pack to import: a regular file, never a link, within the size cap.
pub(super) async fn read_untrusted(path: &Path) -> Result<Vec<u8>> {
    let metadata = tokio::fs::symlink_metadata(path).await.map_err(io_error)?;
    if metadata.file_type().is_symlink()
        || !metadata.is_file()
        || metadata.len() == 0
        || metadata.len() > MAX_PACK_BYTES
    {
        return Err(invalid(
            "Import must be a regular learning-pack file no larger than 256 MiB.",
        ));
    }
    tokio::fs::read(path).await.map_err(io_error)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_failed_write_leaves_no_temporary_file_behind() -> anyhow::Result<()> {
        let elsewhere = tempfile::tempdir()?;
        // The temporary file is written beside the target, then renamed over
        // it; renaming a file over a folder fails after the write succeeded.
        let target = elsewhere.path().join("course.lattice-learning");
        tokio::fs::create_dir(&target).await?;
        tokio::fs::write(target.join("keep.txt"), b"keep me").await?;
        atomic_write(&target, b"pack")
            .await
            .expect_err("a pack cannot replace a folder");
        let mut names = std::fs::read_dir(elsewhere.path())?
            .map(|entry| entry.map(|entry| entry.file_name()))
            .collect::<std::io::Result<Vec<_>>>()?;
        names.sort();
        assert_eq!(names, ["course.lattice-learning"]);
        assert_eq!(tokio::fs::read(target.join("keep.txt")).await?, b"keep me");
        Ok(())
    }
}
