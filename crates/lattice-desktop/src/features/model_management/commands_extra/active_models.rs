//! Setting, clearing and reading the active chat, utility and embedding models.
use super::*;

/// Set the active chat model
///
/// Sets which downloaded model should be used for chat feature.
/// Only one model can be active at a time (enforced by database trigger).
/// Called by gateway - async dispatch.
pub async fn set_active_chat_model_impl(
    container: &Container,
    model_id: &str,
) -> Result<(), AppError> {
    let repository = container.downloaded_models();
    let active_models = container.active_models();
    let validated_model_id = container
        .security_context()
        .input_validator()
        .validate_model_name(model_id)?;

    let use_case = SetActiveChatModelUseCase::new(repository, active_models);

    match use_case.execute(&validated_model_id).await {
        Ok(()) => {
            // Nothing is unloaded: the next request for the default resolves
            // to the new row, and a model no longer named is evicted only when
            // its memory is wanted.
            info!(model_id = %validated_model_id, "Active chat model set");
            Ok(())
        }
        Err(e) => {
            error!(error = %e, model_id = %validated_model_id, "Failed to set active chat model");
            Err(e)
        }
    }
}

/// Clear the active chat model (deactivate)
///
/// Deactivates the currently active chat model.
/// Called by gateway - async dispatch.
pub async fn clear_active_chat_model_impl(container: &Container) -> Result<(), AppError> {
    let active_models = container.active_models();

    let use_case = ClearActiveChatModelUseCase::new(active_models);

    match use_case.execute().await {
        Ok(()) => {
            info!("Active chat model cleared");
            Ok(())
        }
        Err(e) => {
            error!(error = %e, "Failed to clear active chat model");
            Err(e)
        }
    }
}

/// Clear the active embedding model (deactivate)
///
/// Deactivates the currently active embedding model.
/// Called by gateway - async dispatch.
pub async fn clear_active_embedding_model_impl(container: &Container) -> Result<(), AppError> {
    let active_models = container.active_models();

    let use_case = ClearActiveEmbeddingModelUseCase::new(active_models);

    match use_case.execute().await {
        Ok(()) => {
            info!("Active embedding model cleared");
            Ok(())
        }
        Err(e) => {
            error!(error = %e, "Failed to clear active embedding model");
            Err(e)
        }
    }
}

/// Get the currently active chat model
///
/// Returns the model currently set as active for chat, or None if no model is active.
/// Called by gateway - async dispatch.
pub async fn get_active_chat_model_impl(container: &Container) -> Result<String, AppError> {
    let active_models = container.active_models();

    let use_case = GetActiveChatModelUseCase::new(active_models);

    match use_case.execute().await {
        Ok(maybe_model) => {
            let facts = ModelFacts::read(container).await;
            let response: Option<DownloadedModelResponse> =
                maybe_model.map(|m| DownloadedModelResponse::from_model(m, &facts));
            serde_json::to_string(&response).map_err(|e| {
                AppError::Serialization(format!("Failed to serialize chat model: {}", e))
            })
        }
        Err(e) => {
            error!(error = %e, "Failed to get active chat model");
            Err(e)
        }
    }
}

/// Get the currently active embedding model
///
/// Returns the model currently set as active for embeddings, or None if no model is active.
/// Called by gateway - async dispatch.
pub async fn get_active_embedding_model_impl(container: &Container) -> Result<String, AppError> {
    let active_models = container.active_models();

    let use_case = GetActiveEmbeddingModelUseCase::new(active_models);

    match use_case.execute().await {
        Ok(maybe_model) => {
            let facts = ModelFacts::read(container).await;
            let response: Option<DownloadedModelResponse> =
                maybe_model.map(|m| DownloadedModelResponse::from_model(m, &facts));
            serde_json::to_string(&response).map_err(|e| {
                AppError::Serialization(format!("Failed to serialize embedding model: {}", e))
            })
        }
        Err(e) => {
            error!(error = %e, "Failed to get active embedding model");
            Err(e)
        }
    }
}

/// Set the active utility model.
///
/// Utility = HyDE expansion / router / intent classification — a chat-style
/// generation role. Only one model can be active at a time. The use-case
/// gates the local-only download check by backend, so Ollama-backed models
/// (which have no on-disk files) can be activated without false-positive
/// "not downloaded" errors. Called by gateway - async dispatch.
pub async fn set_active_utility_model_impl(
    container: &Container,
    model_id: &str,
) -> Result<(), AppError> {
    let repository = container.downloaded_models();
    let active_models = container.active_models();
    let validated_model_id = container
        .security_context()
        .input_validator()
        .validate_model_name(model_id)?;

    let use_case = SetActiveUtilityModelUseCase::new(repository, active_models);

    match use_case.execute(&validated_model_id).await {
        Ok(()) => {
            // The next utility call resolves to the new row; nothing is
            // unloaded.
            info!(model_id = %validated_model_id, "Active utility model set");
            Ok(())
        }
        Err(e) => {
            error!(error = %e, model_id = %validated_model_id, "Failed to set active utility model");
            Err(e)
        }
    }
}

/// Clear the active utility model — reverts HyDE/router back to the chat model.
pub async fn clear_active_utility_model_impl(container: &Container) -> Result<(), AppError> {
    let repository = container.downloaded_models();
    let active_models = container.active_models();

    let use_case = SetActiveUtilityModelUseCase::new(repository, active_models);

    match use_case.clear().await {
        Ok(()) => {
            info!("Active utility model cleared");
            Ok(())
        }
        Err(e) => {
            error!(error = %e, "Failed to clear active utility model");
            Err(e)
        }
    }
}

/// Set the active embedding model
///
/// Sets which downloaded model should be used for embeddings.
/// Only one model can be active at a time (enforced by database trigger).
/// The new model's vector generation is prepared before the switch, so a
/// failed prepare leaves the previous model active. Its artifact identity is
/// established once, up front, and used both to prepare and to activate.
/// Called by gateway - async dispatch.
pub async fn set_active_embedding_model_impl(
    container: &Container,
    model_id: &str,
) -> Result<(), AppError> {
    let repository = container.downloaded_models();
    let active_models = container.active_models();
    let validated_model_id = container
        .security_context()
        .input_validator()
        .validate_model_name(model_id)?;

    let use_case = SetActiveEmbeddingModelUseCase::new(repository, active_models);

    let candidate = container
        .downloaded_models()
        .find_by_model_id(&validated_model_id)
        .await?
        .ok_or_else(|| AppError::NotFound("Embedding model not found".to_owned()))?;
    candidate
        .location()
        .enclosing_dir()
        .ok_or_else(|| AppError::InvalidInput("Select a downloaded embedding model".to_owned()))?;
    let identity = match use_case.establish(&validated_model_id).await {
        Ok(Some(identity)) => identity,
        Ok(None) => {
            return Err(AppError::InvalidInput(
                "Select a downloaded embedding model".to_owned(),
            ))
        }
        Err(e) => return Err(embedding_activation_failed(&validated_model_id, e)),
    };
    let load_identity = identity.clone();
    let model = container
        .open_embedding_for_activation(&candidate, load_identity)
        .await?;
    crate::infrastructure::persistence::repositories::embedding::generation::prepare(
        container.db_pool(),
        model.as_ref(),
    )
    .await?;

    match use_case
        .activate(&validated_model_id, Some(&identity))
        .await
    {
        Ok(()) => {
            info!(model_id = %validated_model_id, "Active embedding model set");

            container.invalidate_embedding_cache();
            info!("Embedding cache invalidated - new model will be loaded on next access");
            Ok(())
        }
        Err(e) => Err(embedding_activation_failed(&validated_model_id, e)),
    }
}

/// Log a rejected embedding activation; returns the command error.
fn embedding_activation_failed(model_id: &str, e: AppError) -> AppError {
    error!(error = %e, model_id = %model_id, "Failed to set active embedding model");
    e
}
