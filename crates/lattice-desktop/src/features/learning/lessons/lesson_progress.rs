//! Progress for the current durable lesson job, scoped to its future. Keeping
//! this context task-local lets nested review calls report without affecting
//! concurrent chat, outline generation, or other jobs.
use super::call_plan::{CallPlan, Planned};
pub(in crate::features::learning) use crate::features::learning::curriculum::LearningGenerationPhase as Phase;
use crate::features::learning::curriculum::{
    LearningClaimTally, LearningGenerationActivity, LearningGenerationPhase,
    LearningGenerationStep, LearningStatementPlan,
};
use lattice_core::domain::learning::acceptance::AcceptancePolicy;
use lattice_core::domain::learning::claim_review::Class;
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::runtime::jobs::JobContext;
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
    /// The most calls the lesson's checks can make, once its statements are
    /// sorted.
    plan: Option<CallPlan>,
    /// Every ceiling the plan published, in order.
    #[cfg(test)]
    ceilings: Vec<u32>,
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
    model_calls(1)
}

/// `count` calls are about to be made: a knowledge check asks its samples one
/// after another.
pub(in crate::features::learning) fn model_calls(count: u32) -> ModelCall {
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
        let before = state.activity.model_calls;
        state.activity.model_calls = before.saturating_add(count);
        if let Some(ceiling) = state.activity.model_calls_ceiling {
            if before <= ceiling && state.activity.model_calls > ceiling {
                tracing::error!(
                    pass = state.activity.verification_pass,
                    calls = state.activity.model_calls,
                    ceiling,
                    "claim.plan: the checks made more calls than the plan allowed"
                );
            }
        }
    }
    ModelCall(progress)
}

/// A request the plan does not count is running (a lesson's draft, the
/// extraction of its claims, the coverage audit, the teaching review): the
/// panel shows the model working and the calls made stay as they are.
pub(in crate::features::learning) fn request() -> ModelCall {
    model_calls(0)
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
            state.activity.verification_pass = 0;
            state.activity.statements = None;
            state.activity.model_calls = 0;
            state.activity.model_calls_ceiling = None;
            state.plan = None;
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

/// A new candidate text is about to be verified: the first draft, a repair or
/// the user's edits (§6.3). Research rechecks are steps inside a pass, not
/// passes.
pub(in crate::features::learning) fn new_pass() {
    let _ = CURRENT.try_with(|progress| {
        progress
            .0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .activity
            .verification_pass += 1;
    });
}

/// Where the lesson's claims stand after a pass observed them (§6.3).
pub(in crate::features::learning) fn claims(tally: LearningClaimTally) {
    let _ = CURRENT.try_with(|progress| {
        progress
            .0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .activity
            .claims = Some(tally);
    });
}

pub(in crate::features::learning) fn begin_checks(total: usize) {
    let _ = CURRENT.try_with(|progress| {
        let mut state = progress.0.lock().unwrap_or_else(|e| e.into_inner());
        let activity = &mut state.activity;
        activity.checks_total = total as u32;
        activity.checks_completed = 0;
        activity.checks_reused = 0;
    });
}

/// A claim's first check in this pass finished.
pub(in crate::features::learning) fn checked(reused: bool) {
    let _ = CURRENT.try_with(|progress| {
        let mut state = progress.0.lock().unwrap_or_else(|e| e.into_inner());
        state.activity.checks_completed += 1;
        state.activity.checks_reused += u32::from(reused);
    });
}

/// The checks that follow compare claims with `count` references research
/// has just saved (a step inside the pass, §6.3); `None` once they are done.
pub(in crate::features::learning) fn new_references(count: Option<usize>) {
    let _ = CURRENT.try_with(|progress| {
        progress
            .0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .activity
            .new_references = count.map(|count| u32::try_from(count).unwrap_or(u32::MAX));
    });
}

/// The lesson's statements are sorted: from here its checks' calls have a
/// ceiling (claim classes design §3.12). Later passes sort again and change
/// nothing.
pub(in crate::features::learning) fn calls_planned(
    policy: &dyn AcceptancePolicy,
    statements: &[Planned],
    sections: u32,
) {
    let _ = CURRENT.try_with(|progress| {
        let mut state = progress.0.lock().unwrap_or_else(|e| e.into_inner());
        if state.plan.is_none() {
            let count = |class: Class| {
                let of_class = statements
                    .iter()
                    .filter(|statement| statement.class == class);
                u32::try_from(of_class.count()).unwrap_or(u32::MAX)
            };
            state.activity.statements = Some(LearningStatementPlan {
                sourced: count(Class::Sourced),
                reasoning: count(Class::Reasoning),
                common: count(Class::Common),
            });
            state.plan = Some(CallPlan::new(policy, statements, sections));
            publish_ceiling(&mut state);
        }
    });
}

/// The cuts are known: `sections` took one and are left, and only they get a
/// flow repair (claim classes design §3.10).
pub(in crate::features::learning) fn cuts_known(sections: usize) {
    let _ = CURRENT.try_with(|progress| {
        let mut state = progress.0.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(plan) = state.plan.as_mut() {
            plan.cuts_known(u32::try_from(sections).unwrap_or(u32::MAX));
        }
        publish_ceiling(&mut state);
    });
}

/// A section's flow repair is done: it made the call held for it, or none.
pub(in crate::features::learning) fn flow_repaired() {
    let _ = CURRENT.try_with(|progress| {
        let mut state = progress.0.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(plan) = state.plan.as_mut() {
            plan.flow_repaired();
        }
        publish_ceiling(&mut state);
    });
}

/// The plan's statements have moved through their graphs: `statements` are
/// where the ledger now has them, in the plan's order.
pub(in crate::features::learning) fn statements_moved(
    policy: &dyn AcceptancePolicy,
    statements: &[Planned],
) {
    let _ = CURRENT.try_with(|progress| {
        let mut state = progress.0.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(plan) = state.plan.as_mut() {
            plan.moved(policy, statements);
        }
        publish_ceiling(&mut state);
    });
}

fn publish_ceiling(state: &mut State) {
    let made = state.activity.model_calls;
    let ceiling = state.plan.as_ref().map(|plan| plan.ceiling(made));
    if let (Some(before), Some(now)) = (state.activity.model_calls_ceiling, ceiling) {
        state.activity.model_calls_ceiling_rises += u32::from(now > before);
    }
    state.activity.model_calls_ceiling = ceiling;
    #[cfg(test)]
    {
        if let Some(ceiling) = state.activity.model_calls_ceiling {
            state.ceilings.push(ceiling);
        }
    }
}

/// What a traced run recorded.
#[cfg(test)]
pub(in crate::features::learning) struct Trace {
    /// The calls made.
    pub calls: u32,
    /// The ceiling after each event of the plan.
    pub ceilings: Vec<u32>,
    /// The checks finished, and how many of them reused saved checks.
    pub checked: u32,
    pub reused: u32,
    /// The latest stage message.
    pub message: String,
}

#[cfg(test)]
impl Trace {
    /// Whether the ceiling only came down across the plan's events.
    pub(in crate::features::learning) fn only_came_down(&self) -> bool {
        self.ceilings
            .windows(2)
            .all(|pair| matches!(pair, [before, after] if after <= before))
    }

    /// Whether the calls made stayed within the ceiling the run ended on.
    pub(in crate::features::learning) fn within_the_ceiling(&self) -> bool {
        self.ceilings
            .last()
            .is_some_and(|ceiling| self.calls <= *ceiling)
    }
}

/// `future` run with its progress recorded.
#[cfg(test)]
pub(in crate::features::learning) async fn traced<T>(
    future: impl std::future::Future<Output = T>,
) -> (T, Trace) {
    let progress = Progress::default();
    let value = CURRENT.scope(progress.clone(), future).await;
    let state = progress.0.lock().unwrap_or_else(|e| e.into_inner());
    let trace = Trace {
        calls: state.activity.model_calls,
        ceilings: state.ceilings.clone(),
        checked: state.activity.checks_completed,
        reused: state.activity.checks_reused,
        message: state.message.clone(),
    };
    (value, trace)
}

/// The activity a job restarts with: where it left off, its time and its
/// calls counted anew.
fn resumed(saved: Option<serde_json::Value>) -> LearningGenerationActivity {
    let mut activity: LearningGenerationActivity = saved
        .and_then(|activity| serde_json::from_value(activity).ok())
        .unwrap_or_default();
    // A new run starts by reopening references, even when the prior run
    // stopped in that same phase. Do not count time with the app closed.
    activity.phase_started_at = 0;
    activity.model_calls = 0;
    activity.model_calls_ceiling = None;
    activity.model_calls_ceiling_rises = 0;
    activity.statements = None;
    activity
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

fn activity_value(activity: &LearningGenerationActivity) -> Result<serde_json::Value> {
    serde_json::to_value(activity).map_err(|e| AppError::Serialization(e.to_string()))
}

pub(in crate::features::learning) async fn run<T>(
    context: &JobContext,
    future: impl std::future::Future<Output = Result<T>>,
) -> Result<T> {
    let progress = Progress::default();
    // Restarted jobs retain their completed lesson count. Heartbeats must not
    // attempt to reset it to zero while references/models are being loaded.
    let job = context.job();
    {
        let mut state = progress.0.lock().unwrap_or_else(|e| e.into_inner());
        state.completed = job.progress_current;
        state.activity = resumed(job.activity.clone());
        state.program_id = job.subject_id.clone();
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
                        if !context
                            .progress(snapshot.0, &snapshot.1, Some(&activity_value(&snapshot.2)?))
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
                if !context
                    .progress(snapshot.0, &snapshot.1, Some(&activity_value(&snapshot.2)?))
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
    use crate::features::learning::lessons::call_plan::PlannedAt;
    use lattice_core::domain::learning::acceptance::StandardPolicy;

    #[tokio::test]
    async fn structured_progress_tracks_repeated_phases_and_concurrent_model_requests() {
        let progress = Progress::default();
        CURRENT
            .scope(progress.clone(), async {
                lesson("A lesson");
                new_pass();
                phase(Phase::Evidence, "Checking claims");
                begin_checks(3);
                let first = model_call();
                let second = model_call();
                received("Answer");
                drop(first);
                checked(false);
                stage("Checked 1 of 3 claims");
                assert!(
                    progress.snapshot().2.model_running,
                    "One request is still active"
                );
                assert_eq!(progress.snapshot().2.checks_completed, 1);
                drop(second);
                assert!(!progress.snapshot().2.model_running);
                checkpoint_saved();
                phase(Phase::Research, "Finding missing evidence");
                phase(Phase::Evidence, "Rechecking evidence");
                begin_checks(3);
                checked(true);
                let activity = progress.snapshot().2;
                // A recheck after research is a step of the same pass (§6.3).
                assert_eq!(activity.verification_pass, 1);
                assert_eq!(activity.checks_reused, 1);
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
    async fn a_recheck_against_new_references_is_named() {
        let progress = Progress::default();
        CURRENT
            .scope(progress.clone(), async {
                new_references(Some(5));
                begin_checks(3);
                let activity = progress.snapshot().2;
                assert_eq!(activity.new_references, Some(5));
                new_references(None);
                assert_eq!(progress.snapshot().2.new_references, None);
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
        use lattice_core::application::ports::{
            llm_port::{CompletionRequest, CompletionResponse},
            LLMPort,
        };
        struct SlowModel;
        #[async_trait::async_trait]
        impl LLMPort for SlowModel {
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
            use lattice_core::application::services::claim_verification::{CheckPolicy, ClaimChecker, ClaimEvidence, ClaimJudgment, ClaimVerdict};
            let checker = ClaimChecker::new(&SlowModel, lattice_core::application::ports::llm_port::SamplingOverride::deterministic(), 512, CheckPolicy::Strict);
            let judgment = checker.check("Rust is a language.", &ClaimEvidence { text: "Rust is a language.".into(), quote: None }).await;
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

    #[tokio::test]
    async fn calls_are_counted_and_the_ceiling_appears_once_the_statements_are_sorted() {
        let progress = Progress::default();
        CURRENT
            .scope(progress.clone(), async {
                lesson("A lesson");
                drop(model_call());
                let activity = progress.snapshot().2;
                assert_eq!(
                    (activity.model_calls, activity.model_calls_ceiling),
                    (1, None)
                );

                let policy = StandardPolicy::new();
                let alone = |class, at| Planned {
                    class,
                    at,
                    sentence: None,
                };
                let statements = [
                    alone(Class::Sourced, PlannedAt::Entry),
                    alone(Class::Common, PlannedAt::Entry),
                    alone(Class::Common, PlannedAt::Entry),
                ];
                assert_eq!(progress.snapshot().2.statements, None);
                calls_planned(&policy, &statements, 1);
                // The plan is what the statements were sorted into.
                assert_eq!(
                    progress.snapshot().2.statements,
                    Some(LearningStatementPlan {
                        sourced: 1,
                        reasoning: 0,
                        common: 2
                    })
                );
                // The sorting call made (1), the longest path of each
                // statement from its entry (16 for the Sourced one, 16 for
                // each Common one) and the flow repair of the section (1).
                assert_eq!(progress.snapshot().2.model_calls_ceiling, Some(50));
                calls_planned(&policy, &[], 0);
                assert_eq!(progress.snapshot().2.model_calls_ceiling, Some(50));

                // The Sourced statement is resolved: its path is over.
                statements_moved(&policy, &[alone(Class::Sourced, PlannedAt::Ended)]);
                assert_eq!(progress.snapshot().2.model_calls_ceiling, Some(34));
                cuts_known(0);
                assert_eq!(progress.snapshot().2.model_calls_ceiling, Some(33));
                drop(model_calls(3));
                assert_eq!(progress.snapshot().2.model_calls, 4);

                lesson("Another lesson");
                let activity = progress.snapshot().2;
                assert_eq!(
                    (activity.model_calls, activity.model_calls_ceiling),
                    (0, None)
                );
            })
            .await;
    }

    #[test]
    fn a_ceiling_that_goes_up_is_counted_and_one_that_comes_down_is_not() {
        let policy = StandardPolicy::new();
        let sourced = |at| Planned {
            class: Class::Sourced,
            at,
            sentence: None,
        };
        // The sort made one call.
        let mut state = State {
            plan: Some(CallPlan::new(&policy, &[sourced(PlannedAt::Entry)], 1)),
            activity: LearningGenerationActivity {
                model_calls: 1,
                ..Default::default()
            },
            ..Default::default()
        };
        publish_ceiling(&mut state);
        let first = state.activity.model_calls_ceiling.unwrap();
        assert_eq!(first, 1 + 16 + 1);
        assert_eq!(state.activity.model_calls_ceiling_rises, 0);

        // The plan's own events only lower it.
        if let Some(plan) = state.plan.as_mut() {
            plan.moved(&policy, &[sourced(PlannedAt::Ended)]);
        }
        publish_ceiling(&mut state);
        assert_eq!(state.activity.model_calls_ceiling, Some(2));
        assert!(state.activity.model_calls_ceiling.unwrap() <= first);
        assert_eq!(state.activity.model_calls_ceiling_rises, 0);

        // A ceiling shown lower and then published higher is a rise.
        state.activity.model_calls_ceiling = Some(1);
        publish_ceiling(&mut state);
        assert_eq!(state.activity.model_calls_ceiling_rises, 1);
    }

    #[tokio::test]
    async fn a_flow_repair_with_its_call_leaves_the_ceiling_where_it_was_and_one_without_brings_it_down(
    ) {
        let progress = Progress::default();
        CURRENT
            .scope(progress.clone(), async {
                lesson("A lesson");
                drop(model_call());
                let policy = StandardPolicy::new();
                let statement = |at| Planned {
                    class: Class::Sourced,
                    at,
                    sentence: None,
                };
                let ceiling = || progress.snapshot().2.model_calls_ceiling;
                calls_planned(&policy, &[statement(PlannedAt::Entry)], 2);
                statements_moved(&policy, &[statement(PlannedAt::Ended)]);
                cuts_known(2);
                // The sorting call made (1) and the flow repair of each of the
                // two sections that took a cut.
                assert_eq!(ceiling(), Some(3));

                // The first repair makes its call and marks its section mended.
                drop(model_call());
                flow_repaired();
                assert_eq!(ceiling(), Some(3));
                // The pass after the repairs does not raise it.
                statements_moved(&policy, &[statement(PlannedAt::Ended)]);
                assert_eq!(ceiling(), Some(3));

                // The second finds no gap to mend: it makes no call, and the
                // call held for it is not needed.
                flow_repaired();
                assert_eq!(ceiling(), Some(2));
                assert_eq!(progress.snapshot().2.model_calls, 2);
                assert_eq!(progress.snapshot().2.model_calls_ceiling_rises, 0);
            })
            .await;
    }

    #[test]
    fn a_restarted_job_counts_its_calls_again_and_keeps_the_rest() {
        let saved = LearningGenerationActivity {
            model_calls: 40,
            model_calls_ceiling: Some(300),
            model_calls_ceiling_rises: 2,
            checks_total: 10,
            phase_started_at: 5,
            ..Default::default()
        };

        let activity = resumed(Some(serde_json::to_value(&saved).unwrap()));

        assert_eq!(
            (
                activity.model_calls,
                activity.model_calls_ceiling,
                activity.model_calls_ceiling_rises,
                activity.phase_started_at
            ),
            (0, None, 0, 0)
        );
        assert_eq!(activity.checks_total, 10);
    }
}
