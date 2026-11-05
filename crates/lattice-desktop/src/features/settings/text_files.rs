//! Text files the user names or picks in the platform's file dialog. The
//! backend shows the dialog and reads or writes the file, so the webview never
//! names a path.

use async_trait::async_trait;
use lattice_core::shared::error::{AppError, Result};
use serde::{Deserialize, Serialize};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Runtime};
use tauri_plugin_dialog::{DialogExt, FileDialogBuilder};

/// One entry of a file dialog's type list: a name and the extensions it shows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct FileFilterDto {
    pub name: String,
    pub extensions: Vec<String>,
}

/// Where the user chooses to write a file, or to read one.
#[async_trait]
pub trait TextFileDialogs: Send + Sync {
    /// The file to write, offered under `suggested_name`. `None` when the user
    /// cancels.
    async fn save_path(
        &self,
        suggested_name: &str,
        filters: &[FileFilterDto],
    ) -> Result<Option<PathBuf>>;

    /// The file to read. `None` when the user cancels.
    async fn open_path(&self, filters: &[FileFilterDto]) -> Result<Option<PathBuf>>;
}

/// Writes `contents` to the file the user picks. `false` when the user cancels.
pub async fn save(
    dialogs: &dyn TextFileDialogs,
    suggested_name: &str,
    filters: &[FileFilterDto],
    contents: &str,
) -> Result<bool> {
    let Some(path) = dialogs.save_path(suggested_name, filters).await? else {
        return Ok(false);
    };
    tokio::fs::write(&path, contents)
        .await
        .map_err(|error| AppError::Io {
            message: format!("Couldn't write {}: {error}", path.display()),
            kind: error.kind().to_string(),
        })?;
    Ok(true)
}

/// The text of the file the user picks, less a leading byte order mark. `None`
/// when the user cancels.
pub async fn open(
    dialogs: &dyn TextFileDialogs,
    filters: &[FileFilterDto],
) -> Result<Option<String>> {
    let Some(path) = dialogs.open_path(filters).await? else {
        return Ok(None);
    };
    read_text(&path).await.map(Some)
}

async fn read_text(path: &Path) -> Result<String> {
    let bytes = tokio::fs::read(path)
        .await
        .map_err(|error| match error.kind() {
            ErrorKind::NotFound => AppError::FileNotFound {
                path: path.display().to_string(),
            },
            _ => AppError::FileRead {
                path: path.display().to_string(),
                reason: error.to_string(),
            },
        })?;
    let mut text = String::from_utf8(bytes).map_err(|_| AppError::FileRead {
        path: path.display().to_string(),
        reason: "The file is not valid UTF-8 text.".to_string(),
    })?;
    if text.starts_with('\u{feff}') {
        text.remove(0);
    }
    Ok(text)
}

/// The platform's own dialogs, shown off the async runtime.
pub struct NativeDialogs<R: Runtime> {
    app: AppHandle<R>,
}

impl<R: Runtime> NativeDialogs<R> {
    pub fn new(app: AppHandle<R>) -> Self {
        Self { app }
    }
}

fn dialog_failed(error: tokio::task::JoinError) -> AppError {
    AppError::InternalError(format!("The file dialog stopped unexpectedly: {error}"))
}

fn with_filters<R: Runtime>(
    builder: FileDialogBuilder<R>,
    filters: &[FileFilterDto],
) -> FileDialogBuilder<R> {
    filters.iter().fold(builder, |builder, filter| {
        let extensions: Vec<&str> = filter.extensions.iter().map(String::as_str).collect();
        builder.add_filter(&filter.name, &extensions)
    })
}

#[async_trait]
impl<R: Runtime> TextFileDialogs for NativeDialogs<R> {
    async fn save_path(
        &self,
        suggested_name: &str,
        filters: &[FileFilterDto],
    ) -> Result<Option<PathBuf>> {
        let (app, suggested_name, filters) = (
            self.app.clone(),
            suggested_name.to_owned(),
            filters.to_vec(),
        );
        tokio::task::spawn_blocking(move || {
            with_filters(app.dialog().file().set_file_name(suggested_name), &filters)
                .blocking_save_file()
                .and_then(|picked| picked.into_path().ok())
        })
        .await
        .map_err(dialog_failed)
    }

    async fn open_path(&self, filters: &[FileFilterDto]) -> Result<Option<PathBuf>> {
        let (app, filters) = (self.app.clone(), filters.to_vec());
        tokio::task::spawn_blocking(move || {
            with_filters(app.dialog().file(), &filters)
                .blocking_pick_file()
                .and_then(|picked| picked.into_path().ok())
        })
        .await
        .map_err(dialog_failed)
    }
}

#[cfg(test)]
#[path = "text_files_tests.rs"]
mod tests;
