//! Provider selection policy. Loading is injected so fallback behavior needs no I/O to test.
use std::future::Future;

use crate::application::contracts::settings::LLMProvider;
use crate::shared::error::{AppError, Result};

/// Explicit choices never fall back. Auto tries local, configured llama.cpp, then Ollama;
/// when all of them fail, a local model that failed to load is the error worth
/// reporting, not "nothing is installed".
pub async fn select_model<T, Local, LlamaCpp, Remote, LocalFuture, LlamaCppFuture, RemoteFuture>(
    provider: LLMProvider,
    local: Local,
    llama_cpp: LlamaCpp,
    ollama: Remote,
) -> Result<T>
where
    Local: FnOnce() -> LocalFuture,
    LlamaCpp: FnOnce() -> LlamaCppFuture,
    Remote: FnOnce() -> RemoteFuture,
    LocalFuture: Future<Output = Result<Option<T>>>,
    LlamaCppFuture: Future<Output = Result<Option<T>>>,
    RemoteFuture: Future<Output = Result<T>>,
{
    match provider {
        LLMProvider::Local => local().await?.ok_or_else(|| AppError::AiModelsNotInstalled(
            "No local model available. Download and activate a model in Settings → Model Catalog.".to_string(),
        )),
        LLMProvider::Ollama => ollama().await,
        LLMProvider::Llamacpp | LLMProvider::Openai | LLMProvider::Anthropic => Err(AppError::InvalidConfig("Explicit providers must be resolved by their native adapter".into())),
        LLMProvider::Auto => {
            let local_error = match local().await {
                Ok(Some(model)) => return Ok(model),
                Ok(None) => {
                    tracing::debug!("No active downloaded model configured, trying remote providers");
                    None
                }
                Err(error) => {
                    tracing::warn!("Failed to load active model: {}, trying remote providers", error);
                    Some(error)
                }
            };
            match llama_cpp().await {
                Ok(Some(model)) => return Ok(model),
                Ok(None) => tracing::debug!("No llama.cpp model configured, trying Ollama"),
                Err(error) => tracing::warn!("llama.cpp not available: {}, trying Ollama", error),
            }
            match ollama().await {
                Ok(model) => Ok(model),
                Err(error) => {
                    tracing::debug!("Ollama not available: {}", error);
                    Err(local_error.unwrap_or_else(|| AppError::AiModelsNotInstalled(
                        "No LLM available. Please either:\n\
                         1. Download and activate a model in Settings → Model Catalog, OR\n\
                         2. Configure a llama.cpp or Ollama server in Settings → Chat".to_string(),
                    )))
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::future::ready;

    async fn run(
        provider: LLMProvider,
        local_result: Result<Option<&'static str>>,
        remote_result: Result<&'static str>,
    ) -> (Result<&'static str>, Vec<&'static str>) {
        run_with_llama_cpp(provider, local_result, Ok(None), remote_result).await
    }

    async fn run_with_llama_cpp(
        provider: LLMProvider,
        local_result: Result<Option<&'static str>>,
        llama_cpp_result: Result<Option<&'static str>>,
        remote_result: Result<&'static str>,
    ) -> (Result<&'static str>, Vec<&'static str>) {
        let calls = RefCell::new(Vec::new());
        let result = select_model(
            provider,
            || {
                calls.borrow_mut().push("local");
                ready(local_result)
            },
            || {
                calls.borrow_mut().push("llamacpp");
                ready(llama_cpp_result)
            },
            || {
                calls.borrow_mut().push("ollama");
                ready(remote_result)
            },
        )
        .await;
        (result, calls.into_inner())
    }

    #[tokio::test]
    async fn explicit_local_never_calls_remote() {
        let (result, calls) = run(LLMProvider::Local, Ok(Some("local")), Ok("remote")).await;
        assert_eq!(result.unwrap(), "local");
        assert_eq!(calls, ["local"]);
        let (result, calls) = run(LLMProvider::Local, Ok(None), Ok("remote")).await;
        assert!(matches!(result, Err(AppError::AiModelsNotInstalled(_))));
        assert_eq!(calls, ["local"]);
    }

    #[tokio::test]
    async fn explicit_local_preserves_load_failure() {
        let (result, calls) = run(
            LLMProvider::Local,
            Err(AppError::ModelLoadFailed("broken".into())),
            Ok("remote"),
        )
        .await;
        assert!(matches!(result, Err(AppError::ModelLoadFailed(message)) if message == "broken"));
        assert_eq!(calls, ["local"]);
    }

    #[tokio::test]
    async fn explicit_ollama_never_calls_local() {
        let (result, calls) = run(LLMProvider::Ollama, Ok(Some("local")), Ok("remote")).await;
        assert_eq!(result.unwrap(), "remote");
        assert_eq!(calls, ["ollama"]);
        let (result, calls) = run(
            LLMProvider::Ollama,
            Ok(Some("local")),
            Err(AppError::ServiceNotAvailable("offline".into())),
        )
        .await;
        assert!(
            matches!(result, Err(AppError::ServiceNotAvailable(message)) if message == "offline")
        );
        assert_eq!(calls, ["ollama"]);
    }

    #[tokio::test]
    async fn auto_stops_after_local_success() {
        let (result, calls) = run(LLMProvider::Auto, Ok(Some("local")), Ok("remote")).await;
        assert_eq!(result.unwrap(), "local");
        assert_eq!(calls, ["local"]);
    }

    #[tokio::test]
    async fn auto_falls_back_on_absent_or_broken_local_model() {
        for local in [Ok(None), Err(AppError::ModelLoadFailed("broken".into()))] {
            let (result, calls) = run(LLMProvider::Auto, local, Ok("remote")).await;
            assert_eq!(result.unwrap(), "remote");
            assert_eq!(calls, ["local", "llamacpp", "ollama"]);
        }
    }

    #[tokio::test]
    async fn auto_reports_no_models_after_both_attempts_fail() {
        let (result, calls) = run(
            LLMProvider::Auto,
            Ok(None),
            Err(AppError::ServiceNotAvailable("offline".into())),
        )
        .await;
        assert!(matches!(result, Err(AppError::AiModelsNotInstalled(_))));
        assert_eq!(calls, ["local", "llamacpp", "ollama"]);
    }

    #[tokio::test]
    async fn auto_reports_the_local_failure_when_nothing_else_answers() {
        for local in [
            AppError::ModelLoadFailed("broken".into()),
            AppError::ServiceNotAvailable("bundled llama-server can't run".into()),
        ] {
            let expected = local.to_string();
            let (result, calls) = run(
                LLMProvider::Auto,
                Err(local),
                Err(AppError::ServiceNotAvailable("offline".into())),
            )
            .await;
            assert_eq!(result.unwrap_err().to_string(), expected);
            assert_eq!(calls, ["local", "llamacpp", "ollama"]);
        }
    }

    #[tokio::test]
    async fn auto_prefers_configured_llama_cpp_after_local_failure() {
        for local in [Ok(None), Err(AppError::ModelLoadFailed("broken".into()))] {
            let (result, calls) =
                run_with_llama_cpp(LLMProvider::Auto, local, Ok(Some("llamacpp")), Ok("ollama"))
                    .await;
            assert_eq!(result.unwrap(), "llamacpp");
            assert_eq!(calls, ["local", "llamacpp"]);
        }
    }

    #[tokio::test]
    async fn auto_falls_back_to_ollama_when_llama_cpp_is_unavailable() {
        let (result, calls) = run_with_llama_cpp(
            LLMProvider::Auto,
            Ok(None),
            Err(AppError::Network("offline".into())),
            Ok("ollama"),
        )
        .await;
        assert_eq!(result.unwrap(), "ollama");
        assert_eq!(calls, ["local", "llamacpp", "ollama"]);
    }
}
