use lattice_core::application::ports::{EmbeddingPort, LoadedEmbeddingModelPort};
use lattice_core::shared::error::AppError;
use lattice_core::shared::error::Result;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::State;

/// The loaded embedding model, as the embedding commands reach it.
pub struct EmbeddingState {
    loaded: Arc<dyn LoadedEmbeddingModelPort>,
}

impl EmbeddingState {
    pub fn new(loaded: Arc<dyn LoadedEmbeddingModelPort>) -> Self {
        Self { loaded }
    }

    fn model(&self) -> Result<Arc<dyn EmbeddingPort>> {
        self.loaded
            .current_model()
            .ok_or_else(|| AppError::ModelLoadFailed("No embedding model is loaded".into()))
    }
}

#[derive(Debug, Serialize, Deserialize, specta::Type)]
pub struct ModelInfo {
    /// The loaded model's vector-space identity; empty when none is loaded.
    pub model_name: String,
    pub dimensions: i32,
    pub is_loaded: bool,
}

/// Embeds one text as a document passage with the loaded model.
pub(crate) async fn generate_embedding_impl(
    text: String,
    state: &EmbeddingState,
) -> Result<Vec<f32>, AppError> {
    state.model()?.embed_single(&text).await
}

pub async fn generate_embedding(
    text: String,
    state: State<'_, EmbeddingState>,
) -> Result<Vec<f32>, AppError> {
    generate_embedding_impl(text, &state).await
}

/// Embeds several texts as document passages in one batch, in input order.
pub(crate) async fn generate_embeddings_batch_impl(
    texts: Vec<String>,
    state: &EmbeddingState,
) -> Result<Vec<Vec<f32>>, AppError> {
    state.model()?.embed_batch(&texts).await
}

pub async fn generate_embeddings_batch(
    texts: Vec<String>,
    state: State<'_, EmbeddingState>,
) -> Result<Vec<Vec<f32>>, AppError> {
    generate_embeddings_batch_impl(texts, &state).await
}

/// Reports the loaded embedding model: its vector-space identity, its
/// dimension, and whether one is loaded at all.
pub(crate) async fn get_embedding_model_info_impl(
    state: &EmbeddingState,
) -> Result<ModelInfo, AppError> {
    Ok(match state.loaded.current_model() {
        Some(model) => ModelInfo {
            model_name: model.model_identity(),
            dimensions: i32::try_from(model.dimension()).unwrap_or(i32::MAX),
            is_loaded: true,
        },
        None => ModelInfo {
            model_name: String::new(),
            dimensions: 0,
            is_loaded: false,
        },
    })
}

pub async fn get_embedding_model_info(
    state: State<'_, EmbeddingState>,
) -> Result<ModelInfo, AppError> {
    get_embedding_model_info_impl(&state).await
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Loaded(Option<Arc<dyn EmbeddingPort>>);

    impl LoadedEmbeddingModelPort for Loaded {
        fn current_model(&self) -> Option<Arc<dyn EmbeddingPort>> {
            self.0.clone()
        }
    }

    struct Fixed;

    #[async_trait::async_trait]
    impl EmbeddingPort for Fixed {
        async fn embed_single(&self, _text: &str) -> Result<Vec<f32>> {
            Ok(vec![0.5; 3])
        }
        async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
            Ok(texts.iter().map(|_| vec![0.5; 3]).collect())
        }
        fn model_identity(&self) -> String {
            "sha256:fixed".to_owned()
        }
        fn dimension(&self) -> usize {
            3
        }
        async fn is_ready(&self) -> Result<bool> {
            Ok(true)
        }
    }

    fn state(model: Option<Arc<dyn EmbeddingPort>>) -> EmbeddingState {
        EmbeddingState::new(Arc::new(Loaded(model)))
    }

    #[tokio::test]
    async fn every_command_reports_that_no_model_is_loaded() {
        let state = state(None);
        let single = generate_embedding_impl("Hello world".to_string(), &state).await;
        assert!(matches!(single, Err(AppError::ModelLoadFailed(_))));
        let batch =
            generate_embeddings_batch_impl(vec!["Hello".into(), "World".into()], &state).await;
        assert!(matches!(batch, Err(AppError::ModelLoadFailed(_))));
        let info = get_embedding_model_info_impl(&state).await.unwrap();
        assert!(!info.is_loaded);
        assert_eq!(info.dimensions, 0);
    }

    #[tokio::test]
    async fn the_commands_embed_with_the_loaded_model() {
        let state = state(Some(Arc::new(Fixed)));
        let single = generate_embedding_impl("Hello".to_string(), &state)
            .await
            .unwrap();
        assert_eq!(single.len(), 3);
        let batch = generate_embeddings_batch_impl(vec!["a".into(), "b".into()], &state)
            .await
            .unwrap();
        assert_eq!(batch.len(), 2);
        let info = get_embedding_model_info_impl(&state).await.unwrap();
        assert!(info.is_loaded);
        assert_eq!(info.model_name, "sha256:fixed");
        assert_eq!(info.dimensions, 3);
    }
}
