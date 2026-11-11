//! The one readiness gate every path that embeds applies first.

use lattice_core::application::ports::EmbeddingPort;
use lattice_core::shared::error::{AppError, Result};
use std::sync::Arc;

/// Refuses a port that cannot embed: with the error its model failed to load
/// with when it has one, otherwise as not ready yet.
pub(crate) async fn require_ready(port: &dyn EmbeddingPort) -> Result<()> {
    if let Some(error) = port.load_failure() {
        return Err(error);
    }
    let ready = port.is_ready().await.map_err(|e| {
        AppError::ServiceNotAvailable(format!("Failed to verify embedding model readiness: {}", e))
    })?;
    if !ready {
        return Err(AppError::ServiceNotAvailable(
            "AI embedding model is not ready yet. Activate an embedding model in Settings → Downloaded and try again."
                .to_string(),
        ));
    }
    Ok(())
}

/// [`require_ready`] over the result of loading the active model; a failed
/// load call is `ServiceNotAvailable` for every caller.
pub(crate) async fn require_loaded_ready(loaded: Result<Arc<dyn EmbeddingPort>>) -> Result<()> {
    let port = loaded.map_err(|e| {
        AppError::ServiceNotAvailable(format!("Could not load the active embedding model: {}", e))
    })?;
    require_ready(port.as_ref()).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use lattice_core::application::ports::UnavailableEmbedding;

    fn failed() -> UnavailableEmbedding {
        UnavailableEmbedding::failed_to_load(AppError::ModelLoadFailed(
            "weights are corrupt".into(),
        ))
    }

    #[tokio::test]
    async fn a_port_that_failed_to_load_is_refused_with_its_error() {
        let error = require_ready(&failed()).await.unwrap_err();

        assert_eq!(
            error.to_string(),
            AppError::ModelLoadFailed("weights are corrupt".into()).to_string()
        );
    }

    #[tokio::test]
    async fn a_port_with_no_model_is_refused_as_not_ready_yet() {
        let error = require_ready(&UnavailableEmbedding::not_installed())
            .await
            .unwrap_err();

        assert!(matches!(error, AppError::ServiceNotAvailable(_)));
        assert!(error.to_string().contains("not ready yet"));
    }

    #[tokio::test]
    async fn a_failed_load_call_is_service_not_available() {
        let error = require_loaded_ready(Err(AppError::Other("boom".into())))
            .await
            .unwrap_err();

        assert_eq!(
            error.to_string(),
            "Service not available: Could not load the active embedding model: boom"
        );
    }

    #[tokio::test]
    async fn the_gate_gives_a_failed_port_the_same_error_through_the_loaded_path() {
        let port: Arc<dyn EmbeddingPort> = Arc::new(failed());

        let direct = require_ready(port.as_ref()).await.unwrap_err();
        let loaded = require_loaded_ready(Ok(port)).await.unwrap_err();

        assert_eq!(direct.to_string(), loaded.to_string());
    }
}
