//! Progress for the current durable lesson job, scoped to its future. Keeping
//! this context task-local lets nested review calls report without affecting
//! concurrent chat, outline generation, or other jobs.
pub(in crate::features::learning) use crate::features::learning::curriculum::LearningGenerationPhase as Phase;
use crate::features::learning::curriculum::{
    LearningGenerationActivity, LearningGenerationPhase, LearningGenerationStep,
};
use crate::features::learning::curriculum_repository::LearningCurriculumRepository;
use crate::shared::error::{AppError, Result};
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct State {
    message: String,
    completed: u32,
    activity: LearningGenerationActivity,
    active_model_calls: usize,
    /// The course this job prepares; its model calls share the course's
    /// sources as a prompt prefix.
    program_id: Option<String>,
}

#[derive(Clone, Default)]
struct Progress(Arc<Mutex<State>>);

tokio::task_local! { static CURRENT: Progress; }

pub(in crate::features::learning) fn active() -> bool {
    CURRENT.try_with(|_| ()).is_ok()
}

/// The slot-affinity key for this job's model calls: its course, so calls
/// over the same sources return to the llama-server slot that holds them.
pub(in crate::features::learning) fn cache_key() -> Option<String> {
    CURRENT
        .try_with(|progress| {
            progress
                .0
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .program_id
                .clone()
        })
        .ok()
        .flatten()
}

pub(in crate::features::learning) fn stage(message: impl Into<String>) {
    let message = message.into();
    let _ = CURRENT.try_with(|progress| {
        let mut state = progress.0.lock().unwrap_or_else(|e| e.into_inner());
        tracing::info!(stage = %message, "Lesson preparation stage changed");
        state.message = message;
        state.activity.last_activity_at = chrono::Utc::now().timestamp_millis();
        if state.active_model_calls == 0 {
            state.activity.model_running = false;
            state.activity.response_characters = 0;
            state.activity.model_attempt = 0;
        }
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

#[cfg(test)]
pub(in crate::features::learning) fn model_started() {
    model_retry(1);
}

pub(in crate::features::learning) struct ModelCall(Option<Progress>);

impl Drop for ModelCall {
    fn drop(&mut self) {
        if let Some(progress) = &self.0 {
            let mut state = progress.0.lock().unwrap_or_else(|e| e.into_inner());
            state.active_model_calls = state.active_model_calls.saturating_sub(1);
            state.activity.model_running = state.active_model_calls > 0;
        }
    }
}

pub(in crate::features::learning) fn model_call() -> ModelCall {
    let progress = CURRENT.try_with(Clone::clone).ok();
    if let Some(progress) = &progress {
        let mut state = progress.0.lock().unwrap_or_else(|e| e.into_inner());
        if state.active_model_calls == 0 {
            state.activity.response_characters = 0;
            state.activity.model_attempt = 1;
        }
        state.active_model_calls += 1;
        state.activity.model_running = true;
        state.activity.last_activity_at = chrono::Utc::now().timestamp_millis();
    }
    ModelCall(progress)
}

pub(in crate::features::learning) fn model_retry(attempt: usize) {
    let _ = CURRENT.try_with(|progress| {
        let mut state = progress.0.lock().unwrap_or_else(|e| e.into_inner());
        state.activity.model_running = true;
        state.activity.response_characters = 0;
        state.activity.model_attempt = attempt as u32;
        state.activity.last_activity_at = chrono::Utc::now().timestamp_millis();
    });
}

pub(in crate::features::learning) fn received(text: &str) {
    let _ = CURRENT.try_with(|progress| {
        let mut state = progress.0.lock().unwrap_or_else(|e| e.into_inner());
        state.activity.response_characters = state
            .activity
            .response_characters
            .saturating_add(text.chars().count() as u32);
        state.activity.last_activity_at = chrono::Utc::now().timestamp_millis();
    });
}

pub(in crate::features::learning) fn phase(
    phase: LearningGenerationPhase,
    message: impl Into<String>,
) {
    let _ = CURRENT.try_with(|progress| {
        let mut state = progress.0.lock().unwrap_or_else(|e| e.into_inner());
        let activity = &mut state.activity;
        if activity.phase != phase || activity.phase_started_at == 0 {
            activity.phase = phase;
            activity.phase_started_at = chrono::Utc::now().timestamp_millis();
            activity.recent_steps.push(LearningGenerationStep {
                phase,
                started_at: activity.phase_started_at,
            });
            if activity.recent_steps.len() > 12 {
                activity.recent_steps.remove(0);
            }
        }
    });
    stage(message);
}

pub(in crate::features::learning) fn model(name: &str) {
    let _ = CURRENT.try_with(|progress| {
        progress
            .0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .activity
            .model_name = Some(name.into())
    });
}

pub(in crate::features::learning) fn lesson(title: &str) {
    let _ = CURRENT.try_with(|progress| {
        let mut state = progress.0.lock().unwrap_or_else(|e| e.into_inner());
        if state.activity.lesson_title.as_deref() != Some(title) {
            state.activity.checks_total = 0;
            state.activity.checks_completed = 0;
            state.activity.checks_reused = 0;
            state.activity.checks_unresolved = 0;
            state.activity.model_checks_total = None;
            state.activity.verification_pass = 0;
        }
        state.activity.lesson_title = Some(title.into());
    });
}

pub(in crate::features::learning) fn checkpoint_saved() {
    let _ = CURRENT.try_with(|progress| {
        progress
            .0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .activity
            .last_checkpoint_at = Some(chrono::Utc::now().timestamp_millis())
    });
}

pub(in crate::features::learning) fn begin_checks(total: usize) {
    let _ = CURRENT.try_with(|progress| {
        let mut state = progress.0.lock().unwrap_or_else(|e| e.into_inner());
        let activity = &mut state.activity;
        activity.verification_pass += 1;
        activity.checks_total = total as u32;
        activity.checks_completed = 0;
        activity.checks_reused = 0;
        activity.checks_unresolved = 0;
        activity.model_checks_total = None;
    });
}

pub(in crate::features::learning) fn plan_model_checks(total: usize) {
    let _ = CURRENT.try_with(|progress| {
        progress
            .0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .activity
            .model_checks_total = Some(total as u32);
    });
}

pub(in crate::features::learning) fn checked(reused: bool, unresolved: bool) {
    let _ = CURRENT.try_with(|progress| {
        let mut state = progress.0.lock().unwrap_or_else(|e| e.into_inner());
        state.activity.checks_completed += 1;
        state.activity.checks_reused += u32::from(reused);
        state.activity.checks_unresolved += u32::from(unresolved);
    });
}

impl Progress {
    fn snapshot(&self) -> (u32, String, LearningGenerationActivity) {
        let state = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let detail = if !state.activity.model_running {
            String::new()
        } else if state.activity.response_characters == 0 {
            " · Waiting for the model’s response".into()
        } else {
            format!(
                " · {} response characters received",
                state.activity.response_characters
            )
        };
        let attempt = if state.activity.model_attempt > 1 {
            format!(" · Model request attempt {}", state.activity.model_attempt)
        } else {
            String::new()
        };
        (
            state.completed,
            format!("{}{attempt}{detail}", state.message),
            state.activity.clone(),
        )
    }
}

pub(in crate::features::learning) async fn run<T>(
    repo: &LearningCurriculumRepository,
    job_id: &str,
    future: impl std::future::Future<Output = Result<T>>,
) -> Result<T> {
    let progress = Progress::default();
    // Restarted jobs retain their completed lesson count. Heartbeats must not
    // attempt to reset it to zero while references/models are being loaded.
    let job = repo.job(job_id).await?;
    let completed = job.progress_completed;
    {
        let mut state = progress.0.lock().unwrap_or_else(|e| e.into_inner());
        state.completed = completed;
        state.activity = job.activity.unwrap_or_default();
        state.program_id = Some(job.program_id.clone());
        // A new run starts by reopening references, even when the prior run
        // stopped in that same phase. Do not count time with the app closed.
        state.activity.phase_started_at = 0;
    }
    CURRENT
        .scope(progress.clone(), async {
            phase(Phase::References, "Checking saved references");
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(2));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            let persist = async {
                let mut previous = None;
                loop {
                    interval.tick().await;
                    let snapshot = progress.snapshot();
                    if previous.as_ref() != Some(&snapshot) {
                        if !repo
                            .advance_job_activity(job_id, snapshot.0, &snapshot.1, &snapshot.2)
                            .await?
                        {
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
            let result = tokio::select! {
                result = future => result,
                result = persist => result,
            };
            if result.is_ok() {
                let snapshot = progress.snapshot();
                if !repo
                    .advance_job_activity(job_id, snapshot.0, &snapshot.1, &snapshot.2)
                    .await?
                {
                    return Err(AppError::InvalidState(
                        "Lesson preparation is no longer running.".into(),
                    ));
                }
            }
            result
        })
        .await
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::panic)]
    use super::*;

    #[tokio::test]
    async fn structured_progress_tracks_repeated_phases_and_concurrent_model_requests() {
        let progress = Progress::default();
        CURRENT
            .scope(progress.clone(), async {
                lesson("A lesson");
                phase(Phase::Evidence, "Checking claims");
                begin_checks(3);
                let first = model_call();
                let second = model_call();
                received("Answer");
                drop(first);
                checked(false, true);
                stage("Checked 1 of 3 claims");
                assert!(
                    progress.snapshot().2.model_running,
                    "One request is still active"
                );
                assert_eq!(progress.snapshot().2.checks_unresolved, 1);
                drop(second);
                assert!(!progress.snapshot().2.model_running);
                checkpoint_saved();
                phase(Phase::Research, "Finding missing evidence");
                phase(Phase::Evidence, "Rechecking evidence");
                begin_checks(3);
                checked(true, false);
                let activity = progress.snapshot().2;
                assert_eq!(activity.verification_pass, 2);
                assert_eq!(activity.checks_reused, 1);
                assert_eq!(activity.checks_unresolved, 0);
                assert!(activity.last_checkpoint_at.is_some());
                assert_eq!(
                    activity
                        .recent_steps
                        .iter()
                        .map(|step| step.phase)
                        .collect::<Vec<_>>(),
                    [Phase::Evidence, Phase::Research, Phase::Evidence]
                );
            })
            .await;
    }

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
            assert!(progress.snapshot().2.response_characters > 0);
            assert!(!progress.snapshot().2.model_running);
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
                    (progress.snapshot().0, progress.snapshot().1),
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
                assert_eq!(
                    (progress.snapshot().0, progress.snapshot().1),
                    (1, "Saving verified lesson".into())
                );
            })
            .await;
        assert!(!active());
    }
}
