//! What a deep research turn saves as it goes, so a restart carries it on
//! from its last finished round instead of searching and reading again.
//!
//! The turn saves twice over: once when its question is on the thread and
//! its request is planned ([`AssembledTurn`]), then after every round the
//! model finishes ([`ResearchRound`]). A resumed turn skips retrieval and
//! assembly, sends the saved request with every round's calls and results
//! after it, and numbers its rounds on from the saved one.
use super::*;
use crate::application::ports::llm_port::CompletionInput;
use crate::application::ports::ToolDefinition;
use crate::features::conversation::chat::fetch_memory::FetchMemory;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// The turn as it stood before the model's first round.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(in crate::features::conversation::chat) struct AssembledTurn {
    /// The question, saved pending on the thread.
    pub(in crate::features::conversation::chat) user_message_id: String,
    pub(in crate::features::conversation::chat) message_tokens: usize,
    /// Messages of history the turn was planned with.
    pub(in crate::features::conversation::chat) history_len: usize,
    pub(in crate::features::conversation::chat) flags: SearchFlags,
    pub(in crate::features::conversation::chat) router: Option<TurnRouterDto>,
    pub(in crate::features::conversation::chat) input: Vec<CompletionInput>,
    pub(in crate::features::conversation::chat) tools: Vec<ToolDefinition>,
    pub(in crate::features::conversation::chat) max_output_tokens: usize,
    pub(in crate::features::conversation::chat) input_budget: usize,
    pub(in crate::features::conversation::chat) memory_usage: Option<serde_json::Value>,
    pub(in crate::features::conversation::chat) short_circuit_response: Option<String>,
    pub(in crate::features::conversation::chat) sources: Vec<SourceDto>,
    pub(in crate::features::conversation::chat) retrieval_trace: Option<RetrievalTraceDto>,
    pub(in crate::features::conversation::chat) pages_read: FetchMemory,
    pub(in crate::features::conversation::chat) steps: Vec<TurnStepDto>,
    pub(in crate::features::conversation::chat) elapsed_ms: u64,
}

/// Every round the model has finished, as the next round reads it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(in crate::features::conversation::chat) struct ResearchRound {
    /// Rounds finished; a resumed turn starts with the next.
    pub(in crate::features::conversation::chat) completed: usize,
    /// What the rounds added after the planned request: the model's turns,
    /// the calls it made and their results. These are the turn's notes.
    pub(in crate::features::conversation::chat) transcript: Vec<CompletionInput>,
    /// What the rounds searched for.
    pub(in crate::features::conversation::chat) queries: Vec<String>,
    /// Pages the rounds read, each archived on the conversation.
    pub(in crate::features::conversation::chat) fetched: Vec<String>,
    pub(in crate::features::conversation::chat) sources: Vec<SourceDto>,
    pub(in crate::features::conversation::chat) retrieval_trace: Option<RetrievalTraceDto>,
    pub(in crate::features::conversation::chat) pages_read: FetchMemory,
    pub(in crate::features::conversation::chat) steps: Vec<TurnStepDto>,
    pub(in crate::features::conversation::chat) timings: ToolLoopTimingMetrics,
    pub(in crate::features::conversation::chat) elapsed_ms: u64,
}

/// Where a research turn saves itself.
#[async_trait]
pub(in crate::features::conversation::chat) trait ResearchJournal:
    Send + Sync
{
    async fn assembled(&self, turn: &AssembledTurn) -> Result<()>;
    async fn round(&self, round: &ResearchRound) -> Result<()>;
    /// Whether the turn is stopping to carry on later (the app is closing)
    /// rather than ending: its question then stays pending.
    async fn resumes_later(&self) -> bool;
}

/// A research turn's journal, the stop that ends it, and what it saved
/// before a restart.
pub(in crate::features::conversation::chat) struct Research<'a> {
    pub(in crate::features::conversation::chat) journal: &'a dyn ResearchJournal,
    /// Fires when the job is cancelled or the app closes; the turn's own stop
    /// button is its child.
    pub(in crate::features::conversation::chat) cancel: tokio_util::sync::CancellationToken,
    pub(in crate::features::conversation::chat) saved: Option<Saved>,
}

/// What a research turn saved before it stopped.
#[derive(Debug, Clone)]
pub(in crate::features::conversation::chat) struct Saved {
    pub(in crate::features::conversation::chat) assembled: AssembledTurn,
    pub(in crate::features::conversation::chat) round: Option<ResearchRound>,
}

impl Saved {
    /// How long the turn had run when it last saved.
    pub(super) fn elapsed_ms(&self) -> u64 {
        self.round
            .as_ref()
            .map_or(self.assembled.elapsed_ms, |round| round.elapsed_ms)
    }

    pub(super) fn steps(&self) -> Vec<TurnStepDto> {
        self.round
            .as_ref()
            .map_or_else(|| self.assembled.steps.clone(), |round| round.steps.clone())
    }
}

/// Where the tool loop starts a resumed turn.
#[derive(Debug)]
pub(in crate::features::conversation::chat) struct LoopStart {
    pub(in crate::features::conversation::chat) rounds: usize,
    pub(in crate::features::conversation::chat) transcript: Vec<CompletionInput>,
    pub(in crate::features::conversation::chat) queries: Vec<String>,
    pub(in crate::features::conversation::chat) fetched: Vec<String>,
    pub(in crate::features::conversation::chat) timings: ToolLoopTimingMetrics,
}

impl From<ResearchRound> for LoopStart {
    fn from(round: ResearchRound) -> Self {
        Self {
            rounds: round.completed,
            transcript: round.transcript,
            queries: round.queries,
            fetched: round.fetched,
            timings: round.timings,
        }
    }
}

/// A research turn's rounds: where they are saved, and where a resumed turn
/// starts.
pub(in crate::features::conversation::chat) struct Rounds<'a> {
    pub(in crate::features::conversation::chat) journal: &'a dyn ResearchJournal,
    pub(in crate::features::conversation::chat) start: Option<LoopStart>,
}
