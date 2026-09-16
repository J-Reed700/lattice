//! Transcription Plugin — on-device speech-to-text.

use tauri::{
    plugin::{Builder, TauriPlugin},
    Runtime, State,
};

use crate::features::transcription::commands as transcription_commands;
use crate::features::transcription::dto::{TranscriptDto, TranscriptionStatusDto};
use crate::interfaces::di::Container;
use crate::shared::api_result::ApiError;

#[tauri::command]
#[specta::specta]
pub async fn transcribe_file(
    path: String,
    container: State<'_, Container>,
) -> Result<TranscriptDto, ApiError> {
    transcription_commands::transcribe_file_impl(container.inner(), &path)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn get_transcription_status(
    container: State<'_, Container>,
) -> Result<TranscriptionStatusDto, ApiError> {
    transcription_commands::get_transcription_status_impl(container.inner())
        .await
        .map_err(ApiError::from)
}

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("transcription")
        .invoke_handler(tauri::generate_handler![
            transcribe_file,
            get_transcription_status
        ])
        .build()
}
