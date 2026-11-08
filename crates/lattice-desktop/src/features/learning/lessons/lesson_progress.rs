//! Progress for the current durable lesson job, scoped to its future. Keeping
//! this context task-local lets nested review calls report without affecting
//! concurrent chat, outline generation, or other jobs. The calls it counts are
//! those made inside [`checks`], the work of the call plan's readers and
//! rewrites; every other request shows the model working and counts nothing.
use super::call_plan::{CallPlan, Joining, Planned};
pub(in crate::features::learning) use crate::features::learning::curriculum::LearningGenerationPhase as Phase;
use crate::features::learning::curriculum::{
    LearningClaimTally, LearningGenerationActivity, LearningGenerationPhase,
    LearningGenerationStep, LearningStatementPlan,
};
use lattice_core::domain::learning::acceptance::AcceptancePolicy;
use lattice_core::domain::learning::claim_review::{ClaimId, Class};
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
    /// The claims the plan follows, in its order: those of the pass it was
    /// made for, then each it took in.
    planned: Vec<ClaimId>,
    /// The sorting calls made since the plan's last event: once it is made, a
    /// sort sorts only statements new to it, which it takes in.
    sorted: u32,
    /// Every ceiling the plan published, in order.
    #[cfg(test)]
    ceilings: Vec<u32>,
    #[cfg(test)]
    stages: Vec<String>,
}

#[derive(Clone, Default)]
struct Progress(Arc<Mutex<State>>);

tokio::task_local! {
    static CURRENT: Progress;
    /// Whether the work running is the plan's checks.
    static CHECKS: bool;
}

pub(in crate::features::learning) fn active() -> bool {
    CURRENT.try_with(|_| ()).is_ok()
}

/// Runs `future` as the plan's checks: the calls it makes are the calls the
/// panel counts (acceptance design §2.4).
pub(in crate::features::learning) async fn checks<F: std::future::Future>(future: F) -> F::Output {
    CHECKS.scope(true, future).await
}

/// Runs `future`, a stage inside a check step that checks none of the plan's
/// statements: its calls count nothing.
pub(in crate::features::learning) async fn outside_checks<F: std::future::Future>(
    future: F,
) -> F::Output {
    CHECKS.scope(false, future).await
}

fn in_checks() -> bool {
    CHECKS.try_with(|inside| *inside).unwrap_or(false)
}

/// The term of the call plan a counted call is drawn from.
#[derive(Clone, Copy, Debug)]
pub(in crate::features::learning) enum Term {
    /// The sort: among the calls made before the plan, or of a statement it
    /// takes in.
    Sort,
    /// A statement's reading: its share, or its statement's reserve when it is
    /// a successor.
    Reading,
    /// A rewrite or narrowing of statements' sentences, or a section asked for
    /// whole in its place: their shares.
    Rewrite,
    /// A section's flow repair: one of the flow repairs held.
    FlowRepair,
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
        #[cfg(test)]
        state.stages.push(message.clone());
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

pub(in crate::features::learning) fn model_call(term: Term) -> ModelCall {
    model_calls(1, term)
}

/// `count` calls drawn from `term` are about to be made: a knowledge check
/// asks its samples one after another. Inside [`checks`] they are counted;
/// elsewhere they are a [`request`].
pub(in crate::features::learning) fn model_calls(count: u32, term: Term) -> ModelCall {
    let call = request();
    if let (true, Some(progress)) = (in_checks(), &call.0) {
        let mut state = progress.0.lock().unwrap_or_else(|e| e.into_inner());
        let before = state.activity.model_calls;
        state.activity.model_calls = before.saturating_add(count);
        if matches!(term, Term::Sort) {
            state.sorted = state.sorted.saturating_add(count);
        }
        tracing::debug!(
            ?term,
            calls = count,
            made = state.activity.model_calls,
            "claim.plan: a call of the checks"
        );
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
    call
}

/// A model request is running: the panel shows the model working and the
/// calls made stay as they are. A call of the checks is counted where they
/// make it.
pub(in crate::features::learning) fn request() -> ModelCall {
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
            state.activity.verification_pass = 0;
            state.activity.statements = None;
            state.activity.model_calls = 0;
            state.activity.model_calls_ceiling = None;
            state.plan = None;
            state.planned.clear();
            state.sorted = 0;
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
/// ceiling (claim classes design §3.12). A later sort changes nothing here: a
/// statement new to the ledger is taken in when its pass is named
/// ([`statements_moved`]).
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
            publish_ceiling(&mut state, Change::default());
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
        publish_ceiling(&mut state, Change::default());
    });
}

/// A section's flow repair is done: it made the call held for it, or none.
pub(in crate::features::learning) fn flow_repaired() {
    let _ = CURRENT.try_with(|progress| {
        let mut state = progress.0.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(plan) = state.plan.as_mut() {
            plan.flow_repaired();
        }
        publish_ceiling(&mut state, Change::default());
    });
}

/// The claims the plan follows, in its order, once it is made: none until a
/// pass names the claims it was made for.
pub(in crate::features::learning) fn plan_claims() -> Option<Vec<ClaimId>> {
    CURRENT
        .try_with(|progress| {
            let state = progress.0.lock().unwrap_or_else(|e| e.into_inner());
            state.plan.as_ref().map(|_| state.planned.clone())
        })
        .ok()
        .flatten()
}

/// The plan's statements have moved through their graphs: `moved` are where
/// the ledger now has the plan's claims, in its order, and `joining` the
/// statements of `claims` beyond them, which it was not made for and takes in
/// where they stand. The plan follows `claims` from now on.
pub(in crate::features::learning) fn statements_moved(
    policy: &dyn AcceptancePolicy,
    claims: &[ClaimId],
    moved: &[Planned],
    joining: &[Joining],
) {
    let _ = CURRENT.try_with(|progress| {
        let mut state = progress.0.lock().unwrap_or_else(|e| e.into_inner());
        let Some(plan) = state.plan.as_mut() else {
            return;
        };
        let taken_in = plan.take_in(policy, joining);
        let beyond = plan.moved(policy, moved);
        state.planned = claims.to_vec();
        publish_ceiling(&mut state, Change { taken_in, beyond });
    });
}

/// What an event of the plan changed in it beside the calls made.
#[derive(Clone, Copy, Default)]
struct Change {
    /// The calls of the statements it took in, which it was not made for.
    taken_in: u32,
    /// The calls statements can cost beyond their share and their reserve.
    beyond: u32,
}

fn publish_ceiling(state: &mut State, change: Change) {
    let taken_in = change
        .taken_in
        .saturating_add(std::mem::take(&mut state.sorted));
    let made = state.activity.model_calls;
    let ceiling = state.plan.as_ref().map(|plan| plan.ceiling(made));
    if let (Some(before), Some(now), Some(plan)) = (
        state.activity.model_calls_ceiling,
        ceiling,
        state.plan.as_ref(),
    ) {
        if now > before {
            state.activity.model_calls_ceiling_rises += 1;
            let unheld = (now - before).saturating_sub(taken_in.saturating_add(change.beyond));
            let cause = if unheld > 0 {
                "the checks made calls no slot held"
            } else if taken_in > 0 {
                "it took in statements it was not made for"
            } else {
                "a statement can cost more than its share and its reserve"
            };
            tracing::error!(
                made,
                most = plan.shares(),
                reserve = plan.reserves(),
                flow_repairs = plan.flow_repairs(),
                previous = before,
                ceiling = now,
                taken_in,
                beyond = change.beyond,
                unheld,
                "claim.plan: the ceiling rose; {cause}"
            );
        }
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

/// Runs `future` with fresh progress and returns every stage it showed.
#[cfg(test)]
pub(in crate::features::learning) async fn staged<T>(
    future: impl std::future::Future<Output = T>,
) -> (T, Vec<String>) {
    let progress = Progress::default();
    let value = CURRENT.scope(progress.clone(), future).await;
    let stages = progress
        .0
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .stages
        .clone();
    (value, stages)
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
mod tests;
