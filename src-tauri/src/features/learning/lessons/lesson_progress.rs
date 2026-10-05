//! Progress for the current durable lesson job, scoped to its future. Keeping
//! this context task-local lets nested review calls report without affecting
//! concurrent chat, outline generation, or other jobs.
use crate::features::learning::curriculum_repository::LearningCurriculumRepository;
use crate::shared::error::{AppError, Result};
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct State {
    message: String,
    completed: u32,
    model_running: bool,
    response_characters: usize,
    model_attempt: usize,
}

#[derive(Clone, Default)]
struct Progress(Arc<Mutex<State>>);

tokio::task_local! { static CURRENT: Progress; }

pub(in crate::features::learning) fn active() -> bool {
    CURRENT.try_with(|_| ()).is_ok()
}

pub(in crate::features::learning) fn stage(message: impl Into<String>) {
    let message = message.into();
    let _ = CURRENT.try_with(|progress| {
        let mut state = progress.0.lock().unwrap_or_else(|e| e.into_inner());
        tracing::info!(stage = %message, "Lesson preparation stage changed");
        state.message = message;
        state.model_running = false;
        state.response_characters = 0;
        state.model_attempt = 0;
    });
}

pub(in crate::features::learning) fn completed(count: u32) {
    let _ = CURRENT.try_with(|progress| {
        progress
            .0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .completed = count;
    });
}

pub(in crate::features::learning) fn model_started() {
    model_retry(1);
}

pub(in crate::features::learning) fn model_retry(attempt: usize) {
    let _ = CURRENT.try_with(|progress| {
        let mut state = progress.0.lock().unwrap_or_else(|e| e.into_inner());
        state.model_running = true;
        state.response_characters = 0;
        state.model_attempt = attempt;
    });
}

pub(in crate::features::learning) fn received(text: &str) {
    let _ = CURRENT.try_with(|progress| {
        progress
            .0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .response_characters += text.chars().count();
    });
}

impl Progress {
    fn snapshot(&self) -> (u32, String) {
        let state = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let detail = if !state.model_running {
            String::new()
        } else if state.response_characters == 0 {
            " · Waiting for the model’s response".into()
        } else {
            format!(
                " · {} response characters received",
                state.response_characters
            )
        };
        let attempt = if state.model_attempt > 1 {
            format!(" · Model request attempt {}", state.model_attempt)
        } else {
            String::new()
        };
        (
            state.completed,
            format!("{}{attempt}{detail}", state.message),
        )
    }
}

pub(in crate::features::learning) async fn run<T>(
    repo: &LearningCurriculumRepository,
    job_id: &str,
    future: impl std::future::Future<Output = Result<T>>,
) -> Result<T> {
    let progress = Progress::default();
    CURRENT
        .scope(progress.clone(), async {
            stage("Checking saved references");
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(2));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            let persist = async {
                let mut previous = None;
                loop {
                    interval.tick().await;
                    let snapshot = progress.snapshot();
                    if previous.as_ref() != Some(&snapshot) {
                        if !repo.advance_job(job_id, snapshot.0, &snapshot.1).await? {
                            return Err(AppError::InvalidState(
                                "Lesson preparation is no longer running.".into(),
                            ));
                        }
                        previous = Some(snapshot);
                    }
                }
            };
            // Keep polling preparation while progress waits for the database.
            // Preparation may hold the only connection across an await; waiting
            // inside a select branch would otherwise deadlock both operations.
            tokio::select! {
                result = future => result,
                result = persist => result,
            }
        })
        .await
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::panic)]
    use super::*;

    #[tokio::test]
    async fn retry_discards_partial_response_progress_and_keeps_the_stage() {
        let progress = Progress::default();
        CURRENT.scope(progress.clone(), async {
            stage("Extracting factual claims");
            model_started();
            received("partial response that cannot be parsed");
            model_retry(2);
            assert_eq!(progress.snapshot().1,"Extracting factual claims · Model request attempt 2 · Waiting for the model’s response");
            received("{}");
            assert_eq!(progress.snapshot().1,"Extracting factual claims · Model request attempt 2 · 2 response characters received");
        }).await;
    }

    #[tokio::test(start_paused = true)]
    async fn lesson_model_can_run_past_old_deadlines_and_reports_stream_activity() -> Result<()> {
        use crate::application::ports::{
            llm_port::{CompletionRequest, CompletionResponse},
            LLMPort,
        };
        struct SlowModel;
        #[async_trait::async_trait]
        impl LLMPort for SlowModel {
            fn supports_typed_completions(&self) -> bool {
                true
            }
            async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
                assert!(request.no_time_limit);
                assert!(request.wall_clock_budget().is_none());
                tokio::time::sleep(std::time::Duration::from_secs(1800)).await;
                Ok(CompletionResponse { text: "supported\nReason: The source explicitly states this.\nSource quote: Rust is a language.".into(), finish_reason: "stop".into(), ..Default::default() })
            }
            async fn complete_with_progress(
                &self,
                request: &CompletionRequest,
                on_text: &(dyn Fn(String) -> Result<()> + Send + Sync),
            ) -> Result<CompletionResponse> {
                let response = self.complete(request).await?;
                on_text(response.text.clone())?;
                Ok(response)
            }
            async fn generate(
                &self,
                _: &str,
                _: &[String],
                _: Option<Vec<String>>,
            ) -> Result<String> {
                unreachable!()
            }
            async fn generate_streaming(
                &self,
                _: &str,
                _: &[String],
                _: Option<Vec<String>>,
            ) -> Result<Box<dyn futures::Stream<Item = Result<String>> + Send + Unpin + '_>>
            {
                unreachable!()
            }
            fn model_name(&self) -> &str {
                "slow-lesson-fixture"
            }
            fn max_context_tokens(&self) -> usize {
                128_000
            }
            fn count_tokens(&self, text: &str) -> usize {
                text.len() / 4
            }
            async fn is_ready(&self) -> Result<bool> {
                Ok(true)
            }
        }
        let progress = Progress::default();
        CURRENT.scope(progress.clone(), async {
            stage("Writing lesson");
            crate::features::learning::generation::complete_json(&SlowModel, "Teach", "Lesson".into(), serde_json::json!({"type":"object"}), 4000).await?;
            assert!(progress.snapshot().1.contains("response characters received"));
            use crate::application::services::claim_verification::{CheckPolicy, ClaimChecker, ClaimEvidence, ClaimJudgment, ClaimVerdict};
            let checker = ClaimChecker::new(&SlowModel, crate::application::ports::llm_port::SamplingOverride::deterministic(), 512, CheckPolicy::Strict);
            let judgment = checker.check_without_deadline("Rust is a language.", &ClaimEvidence { text: "Rust is a language.".into(), quote: None }).await;
            assert!(matches!(judgment, ClaimJudgment::Judged(outcome) if outcome.verdict == ClaimVerdict::Supported));
            Ok(())
        }).await
    }

    #[tokio::test]
    async fn progress_is_scoped_and_new_calls_reset_response_counts() {
        assert!(!active());
        let progress = Progress::default();
        CURRENT
            .scope(progress.clone(), async {
                stage("Writing lesson");
                model_started();
                received("Hello 🦀");
                assert_eq!(
                    progress.snapshot(),
                    (0, "Writing lesson · 7 response characters received".into())
                );
                assert!(!tokio::spawn(async { active() }).await.unwrap());
                stage("Checking answer keys");
                model_started();
                assert_eq!(
                    progress.snapshot().1,
                    "Checking answer keys · Waiting for the model’s response"
                );
                completed(1);
                stage("Saving verified lesson");
                assert_eq!(progress.snapshot(), (1, "Saving verified lesson".into()));
            })
            .await;
        assert!(!active());
    }
}
