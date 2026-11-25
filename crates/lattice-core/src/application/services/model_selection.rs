//! How a list of candidate models is tried: the first that loads is used, an
//! explicit choice never falls back, and when every Auto candidate fails the
//! error worth reporting is kept.

use std::future::Future;

use crate::application::services::chat_model_resolution::Candidates;
use crate::domain::models::model_ref::ModelRef;
use crate::shared::error::{AppError, Result};

/// The first of `candidates` that loads. `load` gets each model and whether
/// its server must list it ([`Candidates::must_list`]). An explicit choice
/// never falls back: its own failure is the error. Under Auto each failure
/// gives way to the next model, and when all fail, a local model's failure is
/// the error, else "No LLM available".
pub async fn first_that_loads<'a, T, F, Fut>(candidates: &'a Candidates, mut load: F) -> Result<T>
where
    F: FnMut(&'a ModelRef, bool) -> Fut,
    Fut: Future<Output = Result<T>>,
{
    let mut failures = AutoFailures::default();
    for model in &candidates.models {
        match load(model, candidates.must_list(model)).await {
            Ok(loaded) => return Ok(loaded),
            Err(error) if !candidates.auto => return Err(error),
            Err(error) => failures.record(model, error),
        }
    }
    Err(failures.into_error())
}

/// The failures of an Auto chain: each candidate that fails gives way to the
/// next, and when all fail a local model's failure (or a configuration error)
/// is the error worth reporting, not "nothing is installed".
#[derive(Debug, Default)]
struct AutoFailures {
    local: Option<AppError>,
    invalid_config: Option<AppError>,
}

impl AutoFailures {
    /// Logs that `model` failed and keeps the first failure of a local model
    /// or the first invalid-configuration error.
    fn record(&mut self, model: &ModelRef, error: AppError) {
        tracing::warn!(%model, %error, "model not available, trying the next");
        if matches!(model, ModelRef::Local { .. }) && self.local.is_none() {
            self.local = Some(error);
        } else if matches!(error, AppError::InvalidConfig(_)) && self.invalid_config.is_none() {
            self.invalid_config = Some(error);
        }
    }

    /// The kept local failure or configuration error, else "No LLM available".
    fn into_error(self) -> AppError {
        self.local.or(self.invalid_config).unwrap_or_else(|| {
            AppError::AiModelsNotInstalled(
                "No LLM available. Please either:\n\
                 1. Download and activate a model in Settings → Model Catalog, OR\n\
                 2. Configure a llama.cpp or Ollama server in Settings → Chat"
                    .to_string(),
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::future::ready;

    fn local_model() -> ModelRef {
        ModelRef::Local {
            model_id: "ornith-1.5-9b".into(),
        }
    }

    fn llama_cpp_model() -> ModelRef {
        ModelRef::LlamaCpp {
            model: "qwen3.5-27b".into(),
        }
    }

    fn ollama_model() -> ModelRef {
        ModelRef::Ollama {
            model: "qwen3:8b".into(),
        }
    }

    #[test]
    fn auto_failures_report_the_local_failure_even_when_later_candidates_fail() {
        let mut failures = AutoFailures::default();
        failures.record(&local_model(), AppError::ModelLoadFailed("broken".into()));
        failures.record(&llama_cpp_model(), AppError::Network("offline".into()));
        failures.record(
            &ollama_model(),
            AppError::ServiceNotAvailable("offline".into()),
        );
        assert!(
            matches!(failures.into_error(), AppError::ModelLoadFailed(message) if message == "broken")
        );
    }

    #[test]
    fn auto_failures_keep_the_first_local_failure() {
        let mut failures = AutoFailures::default();
        failures.record(&local_model(), AppError::ModelLoadFailed("first".into()));
        failures.record(&local_model(), AppError::ModelLoadFailed("second".into()));
        assert!(
            matches!(failures.into_error(), AppError::ModelLoadFailed(message) if message == "first")
        );
    }

    #[test]
    fn auto_failures_without_a_local_failure_say_no_llm_is_available() {
        let mut failures = AutoFailures::default();
        failures.record(&llama_cpp_model(), AppError::Network("offline".into()));
        failures.record(
            &ollama_model(),
            AppError::ServiceNotAvailable("offline".into()),
        );
        assert!(matches!(
            failures.into_error(),
            AppError::AiModelsNotInstalled(message) if message.starts_with("No LLM available. Please either:")
        ));
        assert!(matches!(
            AutoFailures::default().into_error(),
            AppError::AiModelsNotInstalled(_)
        ));
    }

    /// Walks `models` giving each model tried the next of `results`, and
    /// returns the outcome with each model tried and whether it had to be
    /// listed.
    async fn walk(
        models: Vec<ModelRef>,
        auto: bool,
        results: Vec<Result<&'static str>>,
    ) -> (Result<&'static str>, Vec<(ModelRef, bool)>) {
        let candidates = Candidates { models, auto };
        let results = RefCell::new(results.into_iter());
        let tried = RefCell::new(Vec::new());
        let outcome = first_that_loads(&candidates, |model, must_list| {
            tried.borrow_mut().push((model.clone(), must_list));
            ready(
                results
                    .borrow_mut()
                    .next()
                    .expect("a result for every model tried"),
            )
        })
        .await;
        (outcome, tried.into_inner())
    }

    fn models(tried: &[(ModelRef, bool)]) -> Vec<ModelRef> {
        tried.iter().map(|(model, _)| model.clone()).collect()
    }

    fn all_three() -> Vec<ModelRef> {
        vec![local_model(), llama_cpp_model(), ollama_model()]
    }

    #[tokio::test]
    async fn an_explicit_choice_never_tries_another_model() {
        let (outcome, tried) = walk(
            vec![local_model(), ollama_model()],
            false,
            vec![
                Err(AppError::ModelLoadFailed("broken".into())),
                Ok("ollama"),
            ],
        )
        .await;
        assert!(matches!(outcome, Err(AppError::ModelLoadFailed(message)) if message == "broken"));
        assert_eq!(models(&tried), [local_model()]);

        let (outcome, tried) = walk(
            vec![local_model(), ollama_model()],
            false,
            vec![Ok("local")],
        )
        .await;
        assert_eq!(outcome.unwrap(), "local");
        assert_eq!(models(&tried), [local_model()]);
    }

    #[tokio::test]
    async fn an_explicit_choice_reports_its_own_failure() {
        let (outcome, _) = walk(
            vec![ollama_model()],
            false,
            vec![Err(AppError::ServiceNotAvailable("offline".into()))],
        )
        .await;
        assert!(
            matches!(outcome, Err(AppError::ServiceNotAvailable(message)) if message == "offline")
        );
    }

    #[tokio::test]
    async fn auto_stops_at_the_first_model_that_loads() {
        let (outcome, tried) = walk(all_three(), true, vec![Ok("local")]).await;
        assert_eq!(outcome.unwrap(), "local");
        assert_eq!(models(&tried), [local_model()]);
    }

    #[tokio::test]
    async fn auto_moves_on_to_the_next_model_after_a_failure() {
        let (outcome, tried) = walk(
            all_three(),
            true,
            vec![
                Err(AppError::ModelLoadFailed("broken".into())),
                Ok("llamacpp"),
            ],
        )
        .await;
        assert_eq!(outcome.unwrap(), "llamacpp");
        assert_eq!(models(&tried), [local_model(), llama_cpp_model()]);

        let (outcome, tried) = walk(
            all_three(),
            true,
            vec![
                Err(AppError::ModelLoadFailed("broken".into())),
                Err(AppError::Network("offline".into())),
                Ok("ollama"),
            ],
        )
        .await;
        assert_eq!(outcome.unwrap(), "ollama");
        assert_eq!(models(&tried), all_three());
    }

    #[tokio::test]
    async fn auto_reports_the_local_failure_when_every_model_fails() {
        for local in [
            AppError::ModelLoadFailed("broken".into()),
            AppError::ServiceNotAvailable("bundled llama-server can't run".into()),
        ] {
            let expected = local.to_string();
            let (outcome, tried) = walk(
                all_three(),
                true,
                vec![
                    Err(local),
                    Err(AppError::Network("offline".into())),
                    Err(AppError::ServiceNotAvailable("offline".into())),
                ],
            )
            .await;
            assert_eq!(outcome.unwrap_err().to_string(), expected);
            assert_eq!(models(&tried), all_three());
        }
    }

    #[tokio::test]
    async fn auto_without_a_local_failure_says_no_llm_is_available() {
        let (outcome, tried) = walk(
            vec![llama_cpp_model(), ollama_model()],
            true,
            vec![
                Err(AppError::Network("offline".into())),
                Err(AppError::ServiceNotAvailable("offline".into())),
            ],
        )
        .await;
        assert!(matches!(outcome, Err(AppError::AiModelsNotInstalled(_))));
        assert_eq!(models(&tried), [llama_cpp_model(), ollama_model()]);

        let (outcome, tried) = walk(Vec::new(), true, Vec::new()).await;
        assert!(matches!(outcome, Err(AppError::AiModelsNotInstalled(_))));
        assert!(tried.is_empty());
    }

    #[tokio::test]
    async fn auto_without_a_local_failure_preserves_invalid_config_errors() {
        let (outcome, tried) = walk(
            vec![llama_cpp_model(), ollama_model()],
            true,
            vec![
                Err(AppError::InvalidConfig(
                    "Set both authentication header fields, or clear both.".into(),
                )),
                Err(AppError::ServiceNotAvailable("offline".into())),
            ],
        )
        .await;
        assert!(matches!(
            outcome,
            Err(AppError::InvalidConfig(message))
                if message == "Set both authentication header fields, or clear both."
        ));
        assert_eq!(models(&tried), [llama_cpp_model(), ollama_model()]);
    }

    #[tokio::test]
    async fn under_auto_only_a_llama_cpp_server_must_list_its_model() {
        let (_, tried) = walk(
            all_three(),
            true,
            vec![
                Err(AppError::ModelLoadFailed("broken".into())),
                Err(AppError::Network("offline".into())),
                Ok("ollama"),
            ],
        )
        .await;
        let must_list: Vec<bool> = tried.iter().map(|(_, must_list)| *must_list).collect();
        assert_eq!(must_list, [false, true, false]);

        let (_, tried) = walk(vec![llama_cpp_model()], false, vec![Ok("llamacpp")]).await;
        assert_eq!(tried, [(llama_cpp_model(), false)]);
    }
}
