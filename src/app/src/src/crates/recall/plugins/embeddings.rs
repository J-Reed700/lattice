//! Embeddings Plugin
//!
//! Tauri commands for embedding generation and model management.
//! Routes to interfaces/commands/embeddings.rs implementations.

use crate::{
    interfaces::commands::embeddings::{
        self, EmbeddingOperation, EmbeddingResponse, EmbeddingState,
    },
    interfaces::di::Container,
    shared::api_result::ApiError,
};
use tauri::{
    plugin::{Builder, TauriPlugin},
    Runtime, State,
};

#[tauri::command]
#[specta::specta]
pub async fn embedding_operation(
    container: State<'_, Container>,
    operation: EmbeddingOperation,
    state: State<'_, EmbeddingState>,
) -> Result<EmbeddingResponse, ApiError> {
    embeddings::embedding_operation(container, operation, state)
        .await
        .map_err(ApiError::from)
}

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

#[tauri::command]
#[specta::specta]
pub async fn initialize_models(container: State<'_, Container>) -> Result<String, ApiError> {
    let response = container
        .initialize_models_use_case()
        .execute()
        .await
        .map_err(ApiError::from)?;

    let model_name = response.model_name.unwrap_or_else(|| "unknown".to_string());
    let dimension = response.model_dimension.unwrap_or(0);
    let message = if response.was_cached {
        format!(
            "Embedding model ready (cached): {} (dim {})",
            model_name, dimension
        )
    } else {
        format!(
            "Embedding model initialized: {} (dim {})",
            model_name, dimension
        )
    };

    Ok(message)
}

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("embeddings")
        .invoke_handler(tauri::generate_handler![
            embedding_operation,
            generate_embedding,
            generate_embeddings_batch,
            get_embedding_model_info,
            initialize_models,
        ])
        .build()
}
