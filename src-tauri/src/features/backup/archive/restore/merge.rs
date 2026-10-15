//! Placing the extracted payload onto this machine without overwriting what
//! is already here: library blobs, saved web articles, and the vault folder.

use std::path::{Path, PathBuf};

use tracing::{debug, warn};

use crate::features::backup::archive::format::ArchiveError;

/// `<extracted>/<sha>/<name>` -> `<root>/<sha>/<name>`, skipping existing.
pub(super) fn copy_new_files(files_dir: &Path, root: &Path) -> u64 {
    let mut restored = 0u64;
    // repository-barrier-allow: walking the extracted payload on disk.
    let digests = match std::fs::read_dir(files_dir) {
        Ok(entries) => entries,
        Err(e) => {
            warn!(error = %e, "could not read the extracted files directory");
            return 0;
        }
    };

    for digest_entry in digests.filter_map(|e| e.ok()) {
        let digest_dir = digest_entry.path();
        // repository-barrier-allow: walking the extracted payload on disk.
        let Ok(files) = std::fs::read_dir(&digest_dir) else {
            continue;
        };
        let Some(digest) = digest_entry.file_name().to_str().map(str::to_string) else {
            continue;
        };
        let target_dir = root.join(&digest);

        for file_entry in files.filter_map(|e| e.ok()) {
            let source = file_entry.path();
            // repository-barrier-allow: classifying an extracted payload entry.
            if !source.is_file() {
                continue;
            }
            let target = target_dir.join(file_entry.file_name());
            // repository-barrier-allow: content-addressed skip — identical
            // digest means identical bytes.
            if target.exists() {
                continue;
            }
            if let Err(e) = std::fs::create_dir_all(&target_dir) {
                warn!(error = %e, path = %target_dir.display(), "could not create library folder");
                continue;
            }
            match std::fs::copy(&source, &target) {
                Ok(_) => restored += 1,
                Err(e) => warn!(error = %e, path = %target.display(), "could not restore file"),
            }
        }
    }
    restored
}

/// `<extracted>/<rel>` -> `<root>/<rel>` for every regular file, skipping
/// any that already exist.
pub(super) fn copy_missing_tree(source: &Path, root: &Path) -> u64 {
    let mut restored = 0u64;
    for entry in walkdir::WalkDir::new(source)
        .follow_links(false)
        .min_depth(1)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if !entry.file_type().is_file() {
            continue;
        }
        let Ok(rel) = entry.path().strip_prefix(source) else {
            continue;
        };
        let target = root.join(rel);
        // repository-barrier-allow: never overwrite an article already on disk.
        if target.exists() {
            continue;
        }
        if let Some(parent) = target.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                warn!(error = %e, path = %parent.display(), "could not create web archive folder");
                continue;
            }
        }
        match std::fs::copy(entry.path(), &target) {
            Ok(_) => restored += 1,
            Err(e) => {
                warn!(error = %e, path = %target.display(), "could not restore web article file")
            }
        }
    }
    restored
}

/// Move the extracted vault to `target`, or beside it when `target` already
/// holds something. Returns where it actually landed.
pub(super) fn place_vault(source: &Path, target: &Path) -> Result<PathBuf, ArchiveError> {
    if is_free_for_vault(target) {
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        // An empty directory would make `rename` fail on some platforms.
        let _ = std::fs::remove_dir(target);
        move_dir(source, target)?;
        return Ok(target.to_path_buf());
    }

    let stamp = chrono::Utc::now().format("%Y%m%dT%H%M%SZ");
    let mut name = target.as_os_str().to_os_string();
    name.push(format!("-restored-{stamp}"));
    let beside = PathBuf::from(name);
    move_dir(source, &beside)?;
    Ok(beside)
}

/// True when writing the vault here loses nothing: no such path, or an
/// empty directory.
fn is_free_for_vault(target: &Path) -> bool {
    // repository-barrier-allow: the vault folder is the resource being placed.
    match std::fs::read_dir(target) {
        Ok(mut entries) => entries.next().is_none(),
        // Not a directory, or missing. Missing is free; a *file* there is not.
        // repository-barrier-allow: the vault folder is the resource being placed.
        Err(_) => !target.exists(),
    }
}

/// `rename` where possible, recursive copy across filesystems (the scratch
/// directory and the user's home are often different volumes).
fn move_dir(source: &Path, target: &Path) -> Result<(), ArchiveError> {
    if std::fs::rename(source, target).is_ok() {
        return Ok(());
    }
    copy_dir_recursive(source, target)?;
    if let Err(e) = std::fs::remove_dir_all(source) {
        debug!(error = %e, "could not clean up the extracted vault copy");
    }
    Ok(())
}

pub(super) fn copy_dir_recursive(source: &Path, target: &Path) -> Result<(), ArchiveError> {
    std::fs::create_dir_all(target)?;
    // repository-barrier-allow: walking the extracted payload on disk.
    for entry in std::fs::read_dir(source)? {
        let entry = entry?;
        let from = entry.path();
        let to = target.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir_recursive(&from, &to)?;
        } else {
            std::fs::copy(&from, &to)?;
        }
    }
    Ok(())
}
