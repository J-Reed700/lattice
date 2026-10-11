//! Embeddings Plugin
//!
//! Tauri commands for embedding generation and model management.
//! Routes to interfaces/commands/domains/embeddings.rs implementations.

use crate::{
    features::embedding::commands::{self as embeddings, EmbeddingState},
    shared::ipc::ApiError,
};
use tauri::{
    plugin::{Builder, TauriPlugin},
    Runtime, State,
};

#[tauri::command]
#[specta::specta]
pub async fn generate_embedding(
    text: String,
    state: State<'_, EmbeddingState>,
) -> Result<Vec<f32>, ApiError> {
    embeddings::generate_embedding(text, state)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn generate_embeddings_batch(
    texts: Vec<String>,
    state: State<'_, EmbeddingState>,
) -> Result<Vec<Vec<f32>>, ApiError> {
    embeddings::generate_embeddings_batch(texts, state)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn get_embedding_model_info(
    state: State<'_, EmbeddingState>,
) -> Result<embeddings::ModelInfo, ApiError> {
    embeddings::get_embedding_model_info(state)
        .await
        .map_err(ApiError::from)
}

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("embeddings")
        .invoke_handler(tauri::generate_handler![
            generate_embedding,
            generate_embeddings_batch,
            get_embedding_model_info,
        ])
        .build()
}
