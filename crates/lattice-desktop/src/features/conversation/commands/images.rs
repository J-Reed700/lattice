//! Pictures the reader attaches in the composer, before the message is sent.
//!
//! A drop or the file dialog hands over paths; each file is read here,
//! validated by the picture store and staged as a draft row leased to one
//! `draft:<id>` per call. The message that is sent claims its drafts; the
//! composer releases the ones it discards, and the next start sweeps any
//! left behind. "Show to the model again" stages a copy of a picture the
//! conversation already holds. Design: docs/design/2026-10-17-chat-vision.md
//! §10.1–10.2, §13.

use std::path::Path;

use lattice_core::application::contracts::images::{ChatImageDto, MAX_IMAGE_BYTES};
use lattice_core::application::ports::chat_images::{ImageTurnScope, DRAFT_REQUEST_PREFIX};
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::ipc::ApiError;
use tauri::State;

use crate::features::conversation::chat::ports::ChatRuntime;
use crate::interfaces::di::Container;

/// Stage the pictures at `paths` for the conversation's next message, in the
/// order given. All or nothing: a file that cannot be staged releases the
/// ones staged before it and names itself in the error.
#[tauri::command]
#[specta::specta]
pub async fn attach_message_images(
    container: State<'_, Container>,
    conversation_id: String,
    paths: Vec<String>,
) -> std::result::Result<Vec<ChatImageDto>, ApiError> {
    attach_message_images_impl(container.inner(), conversation_id, paths)
        .await
        .map_err(ApiError::from)
}

/// Release staged pictures the composer discarded. Pictures a sent message
/// already holds are left alone.
#[tauri::command]
#[specta::specta]
pub async fn release_message_images(
    container: State<'_, Container>,
    conversation_id: String,
    image_ids: Vec<String>,
) -> std::result::Result<(), ApiError> {
    release_message_images_impl(container.inner(), conversation_id, image_ids)
        .await
        .map_err(ApiError::from)
}

/// Stage a copy of a picture the conversation holds, so the next message
/// shows it to the model again.
#[tauri::command]
#[specta::specta]
pub async fn restage_message_image(
    container: State<'_, Container>,
    conversation_id: String,
    image_id: String,
) -> std::result::Result<ChatImageDto, ApiError> {
    restage_message_image_impl(container.inner(), conversation_id, image_id)
        .await
        .map_err(ApiError::from)
}

fn require_conversation(conversation_id: &str) -> Result<()> {
    if conversation_id.trim().is_empty() {
        return Err(AppError::InvalidInput(
            "A conversation is required to attach pictures".to_string(),
        ));
    }
    Ok(())
}

pub async fn attach_message_images_impl(
    runtime: &dyn ChatRuntime,
    conversation_id: String,
    paths: Vec<String>,
) -> Result<Vec<ChatImageDto>> {
    require_conversation(&conversation_id)?;
    let scope = ImageTurnScope {
        conversation_id,
        request_id: format!("{DRAFT_REQUEST_PREFIX}{}", uuid::Uuid::new_v4().simple()),
    };
    let images = runtime.chat_images();
    let mut staged: Vec<ChatImageDto> = Vec::with_capacity(paths.len());
    for path in &paths {
        let stored = match read_picture(Path::new(path)).await {
            Ok((name, bytes)) => images.store_attachment(&scope, &name, bytes).await,
            Err(error) => Err(error),
        };
        match stored {
            Ok(picture) => staged.push(picture),
            Err(error) => {
                if !staged.is_empty() {
                    let ids: Vec<String> = staged.into_iter().map(|picture| picture.id).collect();
                    if let Err(release) = images.release_images(&scope.conversation_id, &ids).await
                    {
                        tracing::warn!(error = %release, "Could not release partly staged pictures");
                    }
                }
                return Err(error);
            }
        }
    }
    Ok(staged)
}

pub async fn release_message_images_impl(
    runtime: &dyn ChatRuntime,
    conversation_id: String,
    image_ids: Vec<String>,
) -> Result<()> {
    require_conversation(&conversation_id)?;
    if image_ids.is_empty() {
        return Ok(());
    }
    runtime
        .chat_images()
        .release_images(&conversation_id, &image_ids)
        .await
}

pub async fn restage_message_image_impl(
    runtime: &dyn ChatRuntime,
    conversation_id: String,
    image_id: String,
) -> Result<ChatImageDto> {
    require_conversation(&conversation_id)?;
    runtime
        .chat_images()
        .restage(&conversation_id, &image_id)
        .await
}

/// The file's name and bytes. The path is the reader's own choice, from a
/// drop or the file dialog; it is still read only when it is a regular file
/// within [`MAX_IMAGE_BYTES`] — a security bound checked before reading, so
/// a device file or a huge file is never pulled into memory. The picture
/// store checks what the bytes are.
async fn read_picture(path: &Path) -> Result<(String, Vec<u8>)> {
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .filter(|name| !name.trim().is_empty())
        .ok_or_else(|| AppError::InvalidInput(format!("{} is not a file", path.display())))?;
    let meta = tokio::fs::metadata(path)
        .await
        .map_err(|error| AppError::InvalidInput(format!("{name}: {error}")))?;
    if !meta.is_file() {
        return Err(AppError::InvalidInput(format!("{name} is not a file")));
    }
    if meta.len() > MAX_IMAGE_BYTES as u64 {
        return Err(AppError::InvalidInput(format!(
            "{name}: this picture is too large to attach"
        )));
    }
    let bytes = tokio::fs::read(path)
        .await
        .map_err(|error| AppError::InvalidInput(format!("{name}: {error}")))?;
    // The file may have grown between the size check and the read.
    if bytes.len() > MAX_IMAGE_BYTES {
        return Err(AppError::InvalidInput(format!(
            "{name}: this picture is too large to attach"
        )));
    }
    Ok((name, bytes))
}

#[cfg(test)]
mod tests;
