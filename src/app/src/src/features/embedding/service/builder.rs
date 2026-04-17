use super::EmbeddingService;
use crate::shared::error::{AppError, Result};
use std::marker::PhantomData;
use std::path::PathBuf;

pub struct Uninitialized;
pub struct WithModel;
pub struct Ready;

pub struct EmbeddingServiceBuilder<State = Uninitialized> {
    model_path: Option<PathBuf>,
    _state: PhantomData<State>,
}

impl EmbeddingServiceBuilder<Uninitialized> {
    pub fn new() -> Self {
        Self {
            model_path: None,
            _state: PhantomData,
        }
    }

    pub fn model(self, path: PathBuf) -> EmbeddingServiceBuilder<Ready> {
        EmbeddingServiceBuilder {
            model_path: Some(path),
            _state: PhantomData,
        }
    }
}

impl Default for EmbeddingServiceBuilder<Uninitialized> {
    fn default() -> Self {
        Self::new()
    }
}

impl EmbeddingServiceBuilder<Ready> {
    pub async fn build(self) -> Result<EmbeddingService> {
        let model_path = self
            .model_path
            .ok_or_else(|| AppError::InvalidConfig("Model path not set in builder".to_string()))?;
        EmbeddingService::new(model_path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builder_state_transitions() {
        let builder = EmbeddingServiceBuilder::new();
        let builder_with_model = builder.model(PathBuf::from("test_path"));
    }
}
