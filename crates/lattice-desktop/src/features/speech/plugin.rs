//! Speech Plugin — read-aloud on this computer.

use tauri::{
    plugin::{Builder, TauriPlugin},
    Runtime, State,
};

use crate::features::speech::commands as speech_commands;
use crate::features::speech::dto::{SpeechClipDto, SpeechStatusDto};
use crate::interfaces::di::Container;
use lattice_core::shared::ipc::ApiError;

#[tauri::command]
#[specta::specta]
pub async fn get_speech_status(
    container: State<'_, Container>,
) -> Result<SpeechStatusDto, ApiError> {
    speech_commands::get_speech_status_impl(container.inner())
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn synthesize_speech(
    text: String,
    voice: String,
    speed: f32,
    container: State<'_, Container>,
) -> Result<SpeechClipDto, ApiError> {
    speech_commands::synthesize_speech_impl(container.inner(), text, voice, speed)
        .await
        .map_err(ApiError::from)
}

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("speech")
        .invoke_handler(tauri::generate_handler![
            get_speech_status,
            synthesize_speech
        ])
        .build()
}
