//! Warming up the active chat and utility models.
use super::*;

/// Warm up the currently active chat model.
///
/// This eagerly loads the LLM into memory so first chat response is faster.
pub async fn warm_up_active_chat_model_impl(container: &Container) -> Result<(), AppError> {
    let repository = (*container.downloaded_model_repository()).clone();

    let use_case = GetActiveChatModelUseCase::new(repository.clone());
    let active_model = use_case.execute().await?.ok_or_else(|| {
        AppError::InvalidState("No active chat model is selected. Set one first.".to_string())
    })?;

    let start = Instant::now();
    if let Err(error) = container.get_or_load_llm().await {
        if let AppError::ModelLoadFailed(load_error) = &error {
            warn!(
                model_id = %active_model.model_id(),
                model_name = %active_model.model_name(),
                "Active chat model failed to load during warmup; clearing active selection"
            );

            let clear_use_case = ClearActiveChatModelUseCase::new(repository);
            if let Err(clear_error) = clear_use_case.execute().await {
                error!(
                    model_id = %active_model.model_id(),
                    model_name = %active_model.model_name(),
                    clear_error = %clear_error,
                    "Failed to clear invalid active chat model after warmup failure"
                );
                return Err(AppError::ModelLoadFailed(format!(
                    "Failed to warm up active chat model: {}. Failed to clear invalid active model: {}",
                    load_error, clear_error
                )));
            }

            container.invalidate_llm_cache();
            warn!(
                model_id = %active_model.model_id(),
                model_name = %active_model.model_name(),
                "Invalid active chat model cleared after warmup failure"
            );

            return Err(AppError::ModelLoadFailed(format!(
                "Failed to warm up active chat model: {}. The model was deactivated because it could not be loaded. \
                 Please download or select a compatible model in Settings → Model Catalog.",
                load_error
            )));
        }

        return Err(error);
    }

    let elapsed_ms = start.elapsed().as_millis() as u64;

    info!(
        model_id = %active_model.model_id(),
        model_name = %active_model.model_name(),
        elapsed_ms,
        "Active chat model warmed up"
    );

    Ok(())
}

/// Warm up the currently active utility model.
///
/// Utility models (HyDE expansion, intent routing, follow-up classifier)
/// pay the same cold-start cost as chat models on first use — a 9B GGUF
/// can take 60-120s to mmap + bind on CPU. Calling this from the UI
/// pays that cost up-front (with a visible "loading" state) so the
/// user's first chat turn doesn't carry it.
///
/// Returns `Ok(())` even when no utility model is configured — the
/// runtime falls back to the chat LLM in that case, so there's nothing
/// to warm up. Returns `Err` only when a model IS configured but fails
/// to load.
pub async fn warm_up_active_utility_model_impl(container: &Container) -> Result<(), AppError> {
    let start = Instant::now();
    match container.get_or_load_utility_llm().await {
        Ok(Some(_llm)) => {
            let elapsed_ms = start.elapsed().as_millis() as u64;
            info!(elapsed_ms, "Active utility model warmed up");
            Ok(())
        }
        Ok(None) => {
            // No utility model is configured. Not an error — the runtime
            // falls back to the chat LLM. Surface a friendly message in
            // logs but treat as success so the UI doesn't toast errors
            // for users who haven't picked a utility model.
            info!("No active utility model configured; warmup is a no-op");
            Ok(())
        }
        Err(AppError::ModelLoadFailed(load_error)) => Err(AppError::ModelLoadFailed(format!(
            "Failed to warm up active utility model: {}",
            load_error
        ))),
        Err(error) => Err(error),
    }
}
