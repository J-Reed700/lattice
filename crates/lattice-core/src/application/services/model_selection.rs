//! The failures of an Auto chain of models, so every candidate that fails
//! gives way to the next and the last error is the one worth reporting.

use crate::domain::models::model_ref::ModelRef;
use crate::shared::error::AppError;

/// The failures of an Auto chain: each candidate that fails gives way to the
/// next, and when all fail a local model's failure is the error worth
/// reporting, not "nothing is installed".
#[derive(Debug, Default)]
pub struct AutoFailures {
    local: Option<AppError>,
}

impl AutoFailures {
    /// Logs that `model` failed and keeps the first failure of a local model.
    pub fn record(&mut self, model: &ModelRef, error: AppError) {
        tracing::warn!(%model, %error, "model not available, trying the next");
        if matches!(model, ModelRef::Local { .. }) && self.local.is_none() {
            self.local = Some(error);
        }
    }

    /// The kept local failure, else "No LLM available".
    pub fn into_error(self) -> AppError {
        self.local.unwrap_or_else(|| {
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
}
