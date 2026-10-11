//! Deep research turns run as jobs, one at a time per conversation, so the
//! longest work in the app survives a restart.
//!
//! The command that sends a research turn saves it as a job and waits for the
//! answer, which streams to its window as any turn's does. The turn saves
//! itself as it goes (see [`super::turn::Research`]): after a restart its job
//! is delivered again and carries on from its last finished round, streaming
//! to every window, and the answer lands in the conversation. The chat's stop
//! button cancels the job: a stopped turn ends rather than resuming.
use super::cancellation;
use super::ports::ChatRuntime;
use super::turn::{
    self, AssembledTurn, Research, ResearchJournal, ResearchRound, Saved, TurnRequest,
};
use super::{
    get_or_create_conversation_id, tool_loop, validate_and_guard_chat_request, ChatEventSink,
    ChatResponse,
};
use crate::shared::error::{AppError, Result};
use crate::shared::runtime::jobs::{
    JobContext, JobHandler, JobKindConfig, JobOutcome, JobStatus, NewJob, RecoveryPolicy,
};
use async_trait::async_trait;
use once_cell::sync::Lazy;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use tokio::sync::oneshot;
use tracing::{debug, warn};

/// The job kind of a deep research turn; its subject is the conversation.
pub const DEEP_RESEARCH: &str = "chat.deep_research";
const ASSEMBLED: &str = "assembled";
const ROUND: &str = "round";

/// One research turn per conversation at a time; a turn stopped by a restart
/// resumes from its last round.
pub fn job_config() -> JobKindConfig {
    JobKindConfig::new(RecoveryPolicy::Requeue).exclusive_per_subject()
}

/// The command waiting on a research turn: where the turn streams, and where
/// its answer goes.
struct Waiter {
    emit: ChatEventSink,
    reply: oneshot::Sender<Result<ChatResponse>>,
}

/// Commands waiting on their research turns, by request id.
static WAITING: Lazy<Mutex<HashMap<String, Waiter>>> = Lazy::new(Mutex::default);

fn waiting() -> MutexGuard<'static, HashMap<String, Waiter>> {
    WAITING.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Forgets a waiter when its command ends, however it ends.
struct Waiting(String);

impl Drop for Waiting {
    fn drop(&mut self) {
        waiting().remove(&self.0);
    }
}

fn operation_id(request_id: &str) -> String {
    format!("{DEEP_RESEARCH}:{request_id}")
}

fn decode<T: serde::de::DeserializeOwned>(value: serde_json::Value) -> Result<T> {
    serde_json::from_value(value).map_err(|error| AppError::Serialization(error.to_string()))
}

fn encode<T: serde::Serialize>(value: &T) -> Result<serde_json::Value> {
    serde_json::to_value(value).map_err(|error| AppError::Serialization(error.to_string()))
}

/// Saves a research turn as a job and waits for its answer.
pub(super) async fn run(
    container: &dyn ChatRuntime,
    mut request: TurnRequest,
    emit: ChatEventSink,
) -> Result<ChatResponse> {
    let request_id = request
        .request_id
        .clone()
        .filter(|id| !id.trim().is_empty())
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    request.request_id = Some(request_id.clone());
    // The job's subject is its conversation, so a new one is opened first.
    let conversation_id = match request.conversation_id.clone() {
        Some(id) if !id.trim().is_empty() => id,
        _ => {
            let message = validate_and_guard_chat_request(container, &request.message).await?;
            let llm = container.get_or_load_llm().await?;
            get_or_create_conversation_id(container, None, &message, &llm).await?
        }
    };
    request.conversation_id = Some(conversation_id);

    let (reply, answer) = oneshot::channel();
    waiting().insert(request_id.clone(), Waiter { emit, reply });
    let _waiting = Waiting(request_id);
    let job = container.jobs().submit(&research_job(&request)?).await?;
    if job.status.is_finished() {
        return Err(AppError::InvalidState(
            "This research request has already run.".into(),
        ));
    }
    // A job stopped before it ran drops its waiter without an answer.
    answer
        .await
        .unwrap_or_else(|_| Err(cancellation::cancelled_error()))
}

/// The job a research turn runs as. The request names its conversation and
/// request id by now.
fn research_job(request: &TurnRequest) -> Result<NewJob> {
    let requested = encode(request)?;
    Ok(NewJob {
        kind: DEEP_RESEARCH.into(),
        subject_id: request.conversation_id.clone(),
        operation_id: operation_id(request.request_id.as_deref().unwrap_or_default()),
        payload_hash: format!("{:x}", Sha256::digest(requested.to_string().as_bytes())),
        requested,
        progress_total: rounds_total(),
        message: "Waiting to research".into(),
    })
}

/// The rounds a research turn may spend: its progress counts them.
fn rounds_total() -> u32 {
    u32::try_from(tool_loop::max_tool_rounds(true, false)).unwrap_or(u32::MAX)
}

/// Cancels the research job of `request_id` in a conversation, or every live
/// one when no request is named. True when one was live.
pub(super) async fn stop(
    container: &dyn ChatRuntime,
    conversation_id: &str,
    request_id: Option<&str>,
) -> Result<bool> {
    let jobs = container.jobs();
    let wanted = request_id.map(operation_id);
    let mut stopped = false;
    for job in jobs
        .store()
        .list_for_subject(DEEP_RESEARCH, conversation_id)
        .await?
    {
        if job.status.is_finished()
            || wanted
                .as_deref()
                .is_some_and(|wanted| job.operation_id != wanted)
        {
            continue;
        }
        if let Err(error) = jobs.cancel(&job.id).await {
            // It finished between the read and the cancel.
            debug!(job_id = job.id.as_str(), %error, "Research job ended before its stop");
            continue;
        }
        stopped = true;
        // A job that never started has no turn to answer its command.
        if job.status == JobStatus::Pending {
            if let Some(request_id) = job.operation_id.strip_prefix(&operation_id("")) {
                waiting().remove(request_id);
            }
        }
    }
    Ok(stopped)
}

/// Runs saved research turns.
pub struct DeepResearchJob {
    runtime: Arc<dyn ChatRuntime>,
    /// Where a turn streams when no command is waiting on it: after a restart.
    emit: ChatEventSink,
}

impl DeepResearchJob {
    pub fn new(runtime: Arc<dyn ChatRuntime>, emit: ChatEventSink) -> Self {
        Self { runtime, emit }
    }
}

#[async_trait]
impl JobHandler for DeepResearchJob {
    async fn run(&self, context: &JobContext) -> Result<JobOutcome> {
        let request: TurnRequest = decode(context.job().requested.clone())?;
        let request_id = request.request_id.clone().unwrap_or_default();
        let emit = waiting()
            .get(&request_id)
            .map_or_else(|| Arc::clone(&self.emit), |waiter| Arc::clone(&waiter.emit));
        let journal = JobJournal { context };
        let saved = journal.saved().await?;
        let answered = turn::run_stages(
            self.runtime.as_ref(),
            request,
            &emit,
            Some(Research {
                journal: &journal,
                cancel: context.cancellation().clone(),
                saved,
            }),
        )
        .await;
        let outcome = match &answered {
            Ok(response) => JobOutcome::Completed {
                result_ref: answer_ref(response),
                message: "Answered".into(),
            },
            Err(_) if journal.resumes_later().await => JobOutcome::Stopped,
            Err(error) if cancellation::is_cancellation(error) || context.is_cancelled() => {
                // A stop that reached the turn before its job ends the job too.
                if let Err(error) = context.store().cancel(context.id()).await {
                    debug!(job_id = context.id(), %error, "Research job was already cancelled");
                }
                JobOutcome::Stopped
            }
            Err(error) => JobOutcome::Failed {
                code: "failed".into(),
                message: error.to_string(),
            },
        };
        if let Some(waiter) = waiting().remove(&request_id) {
            // The command may have gone; the answer is saved either way.
            let _ = waiter.reply.send(answered);
        }
        Ok(outcome)
    }
}

/// The answer a turn saved, else its conversation.
fn answer_ref(response: &ChatResponse) -> String {
    response
        .messages
        .iter()
        .rev()
        .find(|message| message.role == "assistant")
        .map_or_else(
            || response.conversation_id.clone(),
            |message| message.id.clone(),
        )
}

/// Saves a research turn on its job.
struct JobJournal<'a> {
    context: &'a JobContext,
}

impl JobJournal<'_> {
    async fn saved(&self) -> Result<Option<Saved>> {
        let Some(assembled) = self.context.checkpoint(ASSEMBLED).await? else {
            return Ok(None);
        };
        Ok(Some(Saved {
            assembled: decode(assembled)?,
            round: self
                .context
                .checkpoint(ROUND)
                .await?
                .map(decode)
                .transpose()?,
        }))
    }

    /// A job that is no longer running takes no more saves, and its turn
    /// stops.
    async fn save(&self, key: &str, value: &serde_json::Value) -> Result<()> {
        if self.context.put_checkpoint(key, value).await? {
            Ok(())
        } else {
            Err(cancellation::cancelled_error())
        }
    }

    async fn report(&self, round: usize, message: &str, activity: serde_json::Value) {
        let current = u32::try_from(round).unwrap_or(u32::MAX).min(rounds_total());
        if let Err(error) = self
            .context
            .progress(current, message, Some(&activity))
            .await
        {
            warn!(job_id = self.context.id(), %error, "Could not save research progress");
        }
    }
}

#[async_trait]
impl ResearchJournal for JobJournal<'_> {
    async fn assembled(&self, turn: &AssembledTurn) -> Result<()> {
        self.save(ASSEMBLED, &encode(turn)?).await?;
        self.report(
            0,
            "Researching",
            serde_json::json!({ "round": 0, "queries": 0, "pages": 0 }),
        )
        .await;
        Ok(())
    }

    async fn round(&self, round: &ResearchRound) -> Result<()> {
        self.save(ROUND, &encode(round)?).await?;
        self.report(
            round.completed,
            &format!("Finished research round {}", round.completed),
            serde_json::json!({
                "round": round.completed,
                "queries": round.queries.len(),
                "pages": round.fetched.len(),
            }),
        )
        .await;
        Ok(())
    }

    /// The app is closing: the job's stop fired while it still runs. A user's
    /// cancellation is committed before it fires.
    async fn resumes_later(&self) -> bool {
        if !self.context.is_cancelled() {
            return false;
        }
        match self.context.store().get(self.context.id()).await {
            Ok(job) => job.status == JobStatus::Running,
            Err(error) => {
                warn!(job_id = self.context.id(), %error, "Could not read a stopped research job");
                false
            }
        }
    }
}

#[cfg(test)]
mod tests;
