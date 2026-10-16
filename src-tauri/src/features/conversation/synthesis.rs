//! Journal synthesis: which entries it reads, and the job that writes it.
//!
//! A synthesis is a job (`journal.synthesis`) whose subject is its
//! destination. It gathers its entries once and saves the plan, writes each
//! part's notes in a scratch conversation and saves them as they come, then
//! merges them into its staged result. A synthesis stopped by a restart
//! resumes at the first part it had not saved; one that finished while the
//! app was closed waits, staged, until the renderer saves it.
use super::chat::ports::{ChatPolicy, ChatRuntime};
use super::chat::{run_unwatched_turn, ChatResponse, ToolPreferences};
use super::commands as conversation;
use super::dto::CreateConversationRequestDto;
use super::workspace_dto::*;
use crate::features::qa::dto::SourceDto;
use async_trait::async_trait;
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::runtime::jobs::{
    JobContext, JobDto, JobHandler, JobKindConfig, JobOutcome, JobRecord, JobRuntime, JobStatus,
    NewJob, RecoveryPolicy,
};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tracing::warn;

mod prompts;
mod week_scope;

use prompts::{
    assign_synthesis_source_ids, build_journal_merge_prompt, build_journal_reduce_prompt,
    build_synthesis_citations, build_synthesis_transcript, extract_latest_assistant_message,
    group_for_merge, normalize_synthesis_scope, plan_journal_map_prompts, source_keys_by_message,
};
use week_scope::select_week_entries;

type SnapshotKey = (String, Option<String>, String, Option<String>, bool);
type SourceKey = (String, String, String, Option<SnapshotKey>);

// The same URL and search excerpt can refer to different captured page versions.
// Preserve that provenance when combining citations from separate conversations.
fn source_key(source: &SourceDto) -> SourceKey {
    (
        source.document_id.clone(),
        source.chunk_id.clone(),
        source.content.clone(),
        source.web_snapshot.as_ref().map(|snapshot| {
            (
                snapshot.url.clone(),
                snapshot.title.clone(),
                snapshot.text.clone(),
                snapshot.fetched_at.clone(),
                snapshot.truncated,
            )
        }),
    )
}
#[derive(Debug, Clone)]
struct SynthesisMessage {
    role: String,
    content: String,
    citation_keys: HashMap<u32, SourceKey>,
}
const WEEK_SYNTHESIS_DAYS: i64 = 7;
/// Conversations listed per read while the week scope pages back through
/// them; it reads on until one is older than the week.
const WEEK_SYNTHESIS_CONVERSATION_PAGE: i64 = 200;
const WEEK_SYNTHESIS_PAGE_TITLE_PREFIX: &str = "Week of ";

/// Characters one synthesis prompt may hold on `llm`. Each part is a chat turn
/// in a scratch conversation, whose system prompt and tool schemas share the
/// turn's input budget with it, so a part takes half of that budget and
/// leaves the other half to them.
fn synthesis_part_chars(llm: &dyn lattice_core::application::ports::LLMPort) -> usize {
    use lattice_core::application::services::context_assembler::{
        BudgetAllocation, BudgetRequest, ModelCapacity,
    };
    let capacity = ModelCapacity::new(llm.model_name(), llm.max_context_tokens());
    let input = BudgetAllocation::plan(&capacity, &BudgetRequest::new(0, 0, 0))
        .map_or(0, |allocation| allocation.input_budget);
    lattice_core::application::ports::llm_port::chars_within_tokens(
        input / 2,
        llm.chars_per_token(),
    )
}

/// What a synthesis entry was built from. Conversations keep today's meaning;
/// references and notes only ever appear under the "week" scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SynthesisEntryKind {
    Conversation,
    Reference,
    Note,
}
impl SynthesisEntryKind {
    fn as_str(self) -> &'static str {
        match self {
            SynthesisEntryKind::Conversation => "conversation",
            SynthesisEntryKind::Reference => "reference",
            SynthesisEntryKind::Note => "note",
        }
    }
}
#[derive(Debug, Clone)]
struct JournalSynthesisEntry {
    /// Empty for reference and note entries.
    conversation_id: String,
    title: String,
    updated_at: String,
    message_count: usize,
    transcript: String,
    kind: SynthesisEntryKind,
    /// Conversation id, passage reference id, or workspace note id.
    source_id: String,
    /// Chat citations retained from this entry's assistant messages.
    sources: Vec<SourceDto>,
    messages: Vec<SynthesisMessage>,
}
/// The job kind of a journal synthesis; its subject is the destination.
pub const JOURNAL_SYNTHESIS: &str = "journal.synthesis";
const PLAN: &str = "plan";
const SCRATCH: &str = "scratch";
/// Progress is told in hundredths: the number of parts is known only once the
/// entries are gathered.
const PROGRESS_TOTAL: u32 = 100;
const GATHERED: u32 = 5;
const WRITING: u32 = 90;

/// One synthesis at a time, one per destination; one stopped by a restart
/// resumes from its last finished part.
pub fn synthesis_job_config() -> JobKindConfig {
    JobKindConfig::new(RecoveryPolicy::Requeue)
        .concurrency(1)
        .exclusive_per_subject()
}

fn step_key(index: usize) -> String {
    format!("map:{index}")
}

fn encode<T: serde::Serialize>(value: &T) -> Result<serde_json::Value> {
    serde_json::to_value(value).map_err(|error| AppError::Serialization(error.to_string()))
}

fn decode<T: serde::de::DeserializeOwned>(value: serde_json::Value) -> Result<T> {
    serde_json::from_value(value).map_err(|error| AppError::Serialization(error.to_string()))
}

impl SynthesisDestinationDto {
    /// The job's subject: two syntheses into one page never interleave.
    fn subject(&self) -> String {
        match self {
            Self::Capture => "capture".to_string(),
            Self::Note { note_id } => format!("note:{note_id}"),
            Self::Week { title } => format!("week:{title}"),
        }
    }
}

/// What the gathered entries came to: one prompt per part, and what the
/// finished synthesis cites. Saved once, so a resumed synthesis reads the same
/// entries it started with.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub(crate) struct SynthesisPlan {
    scope: String,
    entry_count: usize,
    prompts: Vec<String>,
    conversation_ids: Vec<String>,
    citations: Vec<SynthesisCitationDto>,
    sources: Vec<SourceDto>,
    /// What one prompt may hold, so the merge of the parts' notes is split
    /// the same way the entries were.
    part_chars: usize,
}

/// One part's notes, saved as soon as the model writes them.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct StepOutput {
    output: String,
}

/// The conversation a part is being written in, so a restart can remove it.
#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct Scratch {
    conversation_id: Option<String>,
}

/// Gathers the entries a synthesis reads and plans its parts.
async fn plan_synthesis(
    runtime: &dyn ChatRuntime,
    request: &SynthesizeJournalEntriesRequestDto,
) -> Result<SynthesisPlan> {
    let normalized_scope = normalize_synthesis_scope(request.scope.as_deref());

    let mut seen = HashSet::new();
    let mut conversation_ids = Vec::new();
    for id in &request.conversation_ids {
        let normalized = id.trim();
        if normalized.is_empty() {
            continue;
        }
        if seen.insert(normalized.to_string()) {
            conversation_ids.push(normalized.to_string());
        }
    }

    // The "week" scope selects server-side, so it carries no conversation ids.
    // Every other scope still requires an explicit selection, which is read
    // whole: what does not fit one prompt is split across parts.
    let entries = if normalized_scope == "week" {
        select_week_entries(runtime).await?
    } else {
        if conversation_ids.is_empty() {
            return Err(AppError::InvalidInput(
                "At least one conversation ID is required for journal synthesis".to_string(),
            ));
        }

        let mut selected = Vec::new();
        for conversation_id in &conversation_ids {
            let conversation =
                conversation::get_conversation_impl(runtime, conversation_id.clone()).await?;
            let Some(conversation) = conversation else {
                continue;
            };

            let messages =
                conversation::get_conversation_messages_impl(runtime, conversation_id.clone())
                    .await?;
            let transcript = build_synthesis_transcript(&messages);
            let (sources, prompt_messages) = source_keys_by_message(&messages);
            if transcript.is_empty() {
                continue;
            }

            let id = conversation.id.to_string();
            selected.push(JournalSynthesisEntry {
                conversation_id: id.clone(),
                title: conversation.title,
                updated_at: conversation.updated_at.to_rfc3339(),
                message_count: messages.len(),
                transcript,
                kind: SynthesisEntryKind::Conversation,
                source_id: id,
                sources,
                messages: prompt_messages,
            });
        }
        selected
    };

    if entries.is_empty() {
        let message = if normalized_scope == "week" {
            "Nothing from the past week to synthesize."
        } else {
            "No synthesizable message content found in selected conversations"
        };
        return Err(AppError::InvalidInput(message.to_string()));
    }

    let mut entries = entries;
    let sources = assign_synthesis_source_ids(&mut entries);
    let llm = runtime.get_or_load_llm().await?;
    let part_chars = synthesis_part_chars(llm.as_ref());
    let prompts = plan_journal_map_prompts(&entries, part_chars);
    Ok(SynthesisPlan {
        part_chars,
        scope: normalized_scope,
        entry_count: entries.len(),
        prompts,
        conversation_ids: entries
            .iter()
            .filter(|entry| entry.kind == SynthesisEntryKind::Conversation)
            .map(|entry| entry.conversation_id.clone())
            .collect(),
        citations: build_synthesis_citations(&entries),
        sources,
    })
}

/// The prompts say "use only the provided entry transcripts", and each part
/// runs in a scratch conversation that belongs to no space the entries came
/// from: anything retrieved for it is by definition from the wrong place.
fn closed_book() -> ToolPreferences {
    ToolPreferences {
        followup_mode: true,
        turn_mode: Some("followup".to_string()),
        closed_book: true,
        ..ToolPreferences::default()
    }
}

/// What a synthesis needs from the app: its entries, and model turns in
/// scratch conversations of their own.
#[async_trait]
pub(crate) trait SynthesisWork: Send + Sync {
    async fn plan(&self, selection: &SynthesizeJournalEntriesRequestDto) -> Result<SynthesisPlan>;
    /// A conversation for one part, removed once the part is written.
    async fn open_scratch(&self) -> Result<String>;
    async fn answer(&self, conversation_id: &str, prompt: &str) -> Result<ChatResponse>;
    async fn remove_scratch(&self, conversation_id: String);
}

/// The app's entries and models.
struct AppSynthesis {
    runtime: Arc<dyn ChatRuntime>,
}

#[async_trait]
impl SynthesisWork for AppSynthesis {
    async fn plan(&self, selection: &SynthesizeJournalEntriesRequestDto) -> Result<SynthesisPlan> {
        plan_synthesis(&*self.runtime, selection).await
    }

    /// A scratch chat follows the Settings default, the model the plan was
    /// sized for.
    async fn open_scratch(&self) -> Result<String> {
        Ok(ChatPolicy::create_conversation(
            &*self.runtime,
            CreateConversationRequestDto {
                title: "Journal synthesis".to_string(),
                system_prompt: None,
            },
        )
        .await?
        .conversation
        .id)
    }

    async fn answer(&self, conversation_id: &str, prompt: &str) -> Result<ChatResponse> {
        run_unwatched_turn(
            &*self.runtime,
            conversation_id.to_string(),
            prompt.to_string(),
            closed_book(),
        )
        .await
    }

    async fn remove_scratch(&self, conversation_id: String) {
        if let Err(error) = self.runtime.delete_conversation(&conversation_id).await {
            warn!(%error, "Could not remove a synthesis scratch conversation");
        }
    }
}

/// Runs saved syntheses: gathers the entries once, writes each part's notes
/// and saves them as it goes, then merges them into the staged result. The
/// renderer saves that to the destination after its open editors have saved.
pub struct JournalSynthesisJob {
    work: Arc<dyn SynthesisWork>,
}

impl JournalSynthesisJob {
    pub fn new(runtime: Arc<dyn ChatRuntime>) -> Self {
        Self::with(Arc::new(AppSynthesis { runtime }))
    }

    pub(crate) fn with(work: Arc<dyn SynthesisWork>) -> Self {
        Self { work }
    }

    async fn report(&self, context: &JobContext, current: u32, activity: &SynthesisActivityDto) {
        let message = match activity.stage {
            SynthesisStage::Gathering => "Gathering conversation context".to_string(),
            SynthesisStage::Reading => match (activity.chunk_index, activity.chunk_count) {
                (Some(index), Some(count)) if count > 1 => {
                    format!("Reviewing part {index} of {count}")
                }
                _ => "Reviewing the material".to_string(),
            },
            SynthesisStage::Writing => "Writing the synthesis".to_string(),
        };
        let saved = match encode(activity) {
            Ok(activity) => context.progress(current, &message, Some(&activity)).await,
            Err(error) => Err(error),
        };
        if let Err(error) = saved {
            warn!(job_id = context.id(), %error, "Could not save synthesis progress");
        }
    }

    /// Removes the scratch conversation an earlier attempt left behind.
    async fn discard_scratch(&self, context: &JobContext) -> Result<()> {
        let scratch: Scratch = context
            .checkpoint(SCRATCH)
            .await?
            .map(decode)
            .transpose()?
            .unwrap_or_default();
        if let Some(id) = scratch.conversation_id {
            self.work.remove_scratch(id).await;
            context
                .put_checkpoint(SCRATCH, &encode(&Scratch::default())?)
                .await?;
        }
        Ok(())
    }

    /// One model turn in a scratch conversation of its own. `None` when the
    /// job stopped first.
    async fn write(&self, context: &JobContext, prompt: &str) -> Result<Option<String>> {
        let scratch = self.work.open_scratch().await?;
        context
            .put_checkpoint(
                SCRATCH,
                &encode(&Scratch {
                    conversation_id: Some(scratch.clone()),
                })?,
            )
            .await?;
        let answered = context
            .until_cancelled(self.work.answer(&scratch, prompt))
            .await;
        self.work.remove_scratch(scratch).await;
        context
            .put_checkpoint(SCRATCH, &encode(&Scratch::default())?)
            .await?;
        match answered {
            None => Ok(None),
            Some(answered) => extract_latest_assistant_message(&answered?)
                .map(Some)
                .ok_or_else(|| AppError::Other("The model wrote nothing for this part.".into())),
        }
    }
}

#[async_trait]
impl JobHandler for JournalSynthesisJob {
    async fn run(&self, context: &JobContext) -> Result<JobOutcome> {
        let request: SynthesisRequest = decode(context.job().requested.clone())?;
        self.discard_scratch(context).await?;
        let plan: SynthesisPlan = match context.checkpoint(PLAN).await? {
            Some(saved) => decode(saved)?,
            None => {
                self.report(
                    context,
                    0,
                    &SynthesisActivityDto {
                        stage: SynthesisStage::Gathering,
                        entry_count: None,
                        chunk_index: None,
                        chunk_count: None,
                    },
                )
                .await;
                let Some(plan) = context
                    .until_cancelled(self.work.plan(&request.selection))
                    .await
                else {
                    return Ok(JobOutcome::Stopped);
                };
                let plan = plan?;
                if !context.put_checkpoint(PLAN, &encode(&plan)?).await? {
                    return Ok(JobOutcome::Stopped);
                }
                plan
            }
        };

        let parts = plan.prompts.len();
        let mut outputs = Vec::with_capacity(parts);
        for (index, prompt) in plan.prompts.iter().enumerate() {
            if let Some(saved) = context.checkpoint(&step_key(index)).await? {
                outputs.push(decode::<StepOutput>(saved)?.output);
                continue;
            }
            let share = (WRITING - GATHERED) * u32::try_from(index).unwrap_or(u32::MAX)
                / u32::try_from(parts.max(1)).unwrap_or(u32::MAX);
            self.report(
                context,
                GATHERED + share,
                &SynthesisActivityDto {
                    stage: SynthesisStage::Reading,
                    entry_count: Some(plan.entry_count),
                    chunk_index: Some(index + 1),
                    chunk_count: Some(parts),
                },
            )
            .await;
            let Some(output) = self.write(context, prompt).await? else {
                return Ok(JobOutcome::Stopped);
            };
            if !context
                .put_checkpoint(
                    &step_key(index),
                    &encode(&StepOutput {
                        output: output.clone(),
                    })?,
                )
                .await?
            {
                return Ok(JobOutcome::Stopped);
            }
            outputs.push(output);
        }

        self.report(
            context,
            WRITING,
            &SynthesisActivityDto {
                stage: SynthesisStage::Writing,
                entry_count: Some(plan.entry_count),
                chunk_index: None,
                chunk_count: Some(parts),
            },
        )
        .await;
        // Notes too many for one brief are merged level by level, each merge
        // saved like a part, until one prompt holds them all.
        let mut level = 0usize;
        loop {
            let groups = group_for_merge(&outputs, plan.part_chars);
            if groups.len() <= 1 {
                break;
            }
            let mut merged = Vec::with_capacity(groups.len());
            for (index, group) in groups.iter().enumerate() {
                if let [only] = group.as_slice() {
                    merged.push(only.clone());
                    continue;
                }
                let key = format!("merge:{level}:{index}");
                if let Some(saved) = context.checkpoint(&key).await? {
                    merged.push(decode::<StepOutput>(saved)?.output);
                    continue;
                }
                let prompt = build_journal_merge_prompt(group, index + 1, groups.len());
                let Some(output) = self.write(context, &prompt).await? else {
                    return Ok(JobOutcome::Stopped);
                };
                if !context
                    .put_checkpoint(
                        &key,
                        &encode(&StepOutput {
                            output: output.clone(),
                        })?,
                    )
                    .await?
                {
                    return Ok(JobOutcome::Stopped);
                }
                merged.push(output);
            }
            outputs = merged;
            level += 1;
        }
        let Some(synthesis) = self
            .write(context, &build_journal_reduce_prompt(&outputs))
            .await?
        else {
            return Ok(JobOutcome::Stopped);
        };
        let result = SynthesizeJournalEntriesResponseDto {
            synthesis,
            scope: plan.scope,
            entry_count: plan.entry_count,
            chunk_count: parts,
            conversation_ids: plan.conversation_ids,
            citations: plan.citations,
            sources: plan.sources,
        };
        if !context.stage(encode(&result)?).await? {
            return Ok(JobOutcome::Stopped);
        }
        Ok(JobOutcome::Completed {
            result_ref: request.destination.subject(),
            message: "Synthesis ready".into(),
        })
    }
}

/// What a synthesis job was asked: the entries, where the result goes, and
/// how the renderer labels it.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct SynthesisRequest {
    selection: SynthesizeJournalEntriesRequestDto,
    destination: SynthesisDestinationDto,
    title: String,
    heading: String,
}

fn synthesis_dto(job: &JobRecord) -> Result<JournalSynthesisDto> {
    let request: SynthesisRequest = decode(job.requested.clone())?;
    Ok(JournalSynthesisDto {
        job: JobDto::from(job),
        title: request.title,
        heading: request.heading,
        destination: request.destination,
        conversation_ids: request.selection.conversation_ids,
        activity: job.activity.clone().and_then(|value| decode(value).ok()),
    })
}

async fn synthesis_job(jobs: &JobRuntime, job_id: &str) -> Result<JobRecord> {
    let job = jobs.store().get(job_id).await?;
    if job.kind != JOURNAL_SYNTHESIS {
        return Err(AppError::NotFound("Synthesis not found".into()));
    }
    Ok(job)
}

/// Starts a synthesis as a job. The destination is fixed now: the result is
/// saved there however long the job takes, across restarts.
pub async fn synthesize_journal_entries_impl(
    request: StartJournalSynthesisRequestDto,
    jobs: &Arc<JobRuntime>,
) -> Result<JournalSynthesisDto> {
    let subject = request.destination.subject();
    let requested = encode(&SynthesisRequest {
        selection: request.request,
        destination: request.destination,
        title: request.title,
        heading: request.heading,
    })?;
    let job = jobs
        .submit(&NewJob {
            kind: JOURNAL_SYNTHESIS.into(),
            subject_id: Some(subject),
            operation_id: uuid::Uuid::new_v4().to_string(),
            payload_hash: format!("{:x}", Sha256::digest(requested.to_string().as_bytes())),
            requested,
            progress_total: PROGRESS_TOTAL,
            message: "Waiting to synthesize".into(),
        })
        .await?;
    synthesis_dto(&job)
}

/// Every synthesis not yet saved or dismissed, newest first.
pub async fn list_journal_syntheses_impl(jobs: &JobRuntime) -> Result<Vec<JournalSynthesisDto>> {
    jobs.store()
        .list_latest(&[JOURNAL_SYNTHESIS], 50, 0)
        .await?
        .iter()
        .map(synthesis_dto)
        .collect()
}

/// A finished synthesis's result, for the renderer to save.
pub async fn get_journal_synthesis_result_impl(
    job_id: &str,
    jobs: &JobRuntime,
) -> Result<SynthesizeJournalEntriesResponseDto> {
    let job = synthesis_job(jobs, job_id).await?;
    match (job.status, job.staged_result) {
        (JobStatus::Completed, Some(result)) => decode(result),
        _ => Err(AppError::InvalidState(
            "This synthesis has not finished.".into(),
        )),
    }
}

/// Records that a finished synthesis was saved to its destination, by
/// removing it. False when it already was: a result is saved once.
pub async fn mark_journal_synthesis_applied_impl(job_id: &str, jobs: &JobRuntime) -> Result<bool> {
    let job = match synthesis_job(jobs, job_id).await {
        Ok(job) => job,
        Err(AppError::NotFound(_)) => return Ok(false),
        Err(error) => return Err(error),
    };
    if job.status != JobStatus::Completed {
        return Err(AppError::InvalidState(
            "This synthesis has not finished.".into(),
        ));
    }
    jobs.store().delete(job_id).await?;
    Ok(true)
}

/// Removes a synthesis that ended without a result. One still running is
/// refused.
pub async fn dismiss_journal_synthesis_impl(job_id: &str, jobs: &JobRuntime) -> Result<()> {
    synthesis_job(jobs, job_id).await?;
    jobs.store().delete(job_id).await
}

/// Tries a failed or interrupted synthesis again, keeping the parts it wrote.
pub async fn retry_journal_synthesis_impl(
    job_id: &str,
    jobs: &Arc<JobRuntime>,
) -> Result<JournalSynthesisDto> {
    synthesis_job(jobs, job_id).await?;
    let job = jobs
        .retry(job_id, &uuid::Uuid::new_v4().to_string())
        .await?;
    synthesis_dto(&job)
}

#[cfg(test)]
mod job_tests;

#[cfg(test)]
mod week_scope_tests;
