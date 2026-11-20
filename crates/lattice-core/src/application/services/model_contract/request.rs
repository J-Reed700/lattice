//! The requests a model is sent. What a request says is private and built in
//! this module's parent only; how it is sent stays public.

use std::time::Duration;

use serde::Serialize;
use tokio_util::sync::CancellationToken;

use super::shape::Schema;
use super::ContractCall;
use crate::application::ports::llm_port::{
    CompletionInput, InferencePriority, SamplingOverride, ThinkingMode, ToolChoice, ToolDefinition,
};
use crate::application::services::grounded_generation::{
    CallOptions, EvidencePassage, EvidenceSelection, HistoryItem,
};

mod steps;

/// One request to a model.
#[derive(Debug, Clone, Serialize)]
pub struct CompletionRequest {
    pub(super) input: Vec<CompletionInput>,
    pub(super) tools: Vec<ToolDefinition>,
    #[serde(rename = "json_schema")]
    pub(super) schema: Option<Schema>,
    pub reasoning_effort: Option<String>,
    /// The model call this request belongs to; never sent to a provider.
    #[serde(skip)]
    pub(super) contract: Option<ContractCall>,
    /// How the model may use the offered tools. `None` leaves the provider's
    /// default (auto). `Some(ToolChoice::None)` keeps the tools in the prompt,
    /// so the cached prefix stays valid, while forbidding calls this round.
    pub(super) tool_choice: Option<ToolChoice>,
    /// Ask providers that support it for displayable reasoning. This controls
    /// summaries or explicit thinking text only; opaque provider state used for
    /// replay remains separate in `CompletionResponse::replay`.
    pub include_reasoning: bool,
    /// Sampling for this request. `None` keeps the provider's configuration.
    pub sampling: Option<SamplingOverride>,
    /// Whether the model thinks before answering. Off unless the caller asks:
    /// see [`ThinkingMode`] for why tool rounds never think.
    pub thinking: ThinkingMode,
    /// Caps generated tokens for this request, overriding the model's own
    /// ceiling when it is lower. A model that thinks first spends its thinking
    /// out of the same ceiling, so there the model's ceiling alone applies.
    ///
    /// The context assembler reserves output room out of the model's window, and
    /// a reservation nothing enforces is only bookkeeping: without this the model
    /// may generate past what the budget set aside and overrun the window it was
    /// measured against. `None` keeps the provider's configured limit.
    ///
    /// A ceiling, never a demand: when the prompt leaves less room in the
    /// window, the answer gets the room left. No request is refused for its
    /// answer's size, only for a prompt that leaves no room at all.
    pub(super) max_output_tokens: Option<u32>,
    /// Text that ends the answer where the model writes it, sent as the
    /// provider's stop sequences; the answer stops before the first one. A
    /// bound the caller can name in the output itself, such as the first label
    /// past the end of a numbered list. Empty sends none.
    pub(super) stop: Vec<String>,
    /// Caps wall-clock time across all attempts. Stalls are detected separately,
    /// so this only needs to exceed the longest legitimate generation.
    #[serde(skip)]
    pub time_budget: Option<Duration>,
    /// Interactive, cancellable workflows may opt out of elapsed-time limits.
    /// Transport errors, stall detection, and retry limits still apply.
    #[serde(skip)]
    pub no_time_limit: bool,
    /// Ask for the log-probabilities of the first generated token and its top
    /// alternatives. A classifier reads its answer's probability from these
    /// rather than trusting one sampled word. Providers that cannot report
    /// them ignore the flag and return `None`.
    pub want_logprobs: bool,
    /// Who is waiting on this call. The backend's scheduler admits higher
    /// priorities first and keeps a slot free for interactive work.
    pub priority: InferencePriority,
    /// Fires when the caller no longer wants the answer. A queued request
    /// leaves the queue and an in-flight one is aborted, both with an error.
    #[serde(skip)]
    pub cancel: Option<CancellationToken>,
    /// Requests sharing a key share a prompt prefix (one conversation, one
    /// course). A llama-server backend sends them to the slot that last served
    /// the key, so the server reuses that prefix from its KV cache.
    pub cache_key: Option<String>,
}

impl CompletionRequest {
    /// A request that says nothing, sent with the defaults.
    pub(super) fn empty() -> Self {
        Self {
            input: Vec::new(),
            tools: Vec::new(),
            schema: None,
            reasoning_effort: None,
            contract: None,
            tool_choice: None,
            include_reasoning: false,
            sampling: None,
            thinking: ThinkingMode::default(),
            max_output_tokens: None,
            stop: Vec::new(),
            time_budget: None,
            no_time_limit: false,
            want_logprobs: false,
            priority: InferencePriority::default(),
            cancel: None,
            cache_key: None,
        }
    }

    /// The empty request a scheduler measures a backend with.
    pub fn probe() -> Self {
        Self::empty()
    }

    pub fn input(&self) -> &[CompletionInput] {
        &self.input
    }

    pub fn tools(&self) -> &[ToolDefinition] {
        &self.tools
    }

    /// The schema the answer is held to.
    pub fn schema(&self) -> Option<&Schema> {
        self.schema.as_ref()
    }

    pub fn tool_choice(&self) -> Option<ToolChoice> {
        self.tool_choice
    }

    pub fn max_output_tokens(&self) -> Option<u32> {
        self.max_output_tokens
    }

    pub fn stop(&self) -> &[String] {
        &self.stop
    }

    pub fn contract(&self) -> Option<&ContractCall> {
        self.contract.as_ref()
    }

    /// This request as a call of `contract`.
    pub(super) fn of_contract(&self, contract: &ContractCall) -> Self {
        Self {
            contract: Some(contract.clone()),
            ..self.clone()
        }
    }
}

/// Everything one grounded call needs.
#[derive(Debug, Clone)]
pub struct GroundedRequest {
    /// The system message.
    pub(super) instructions: String,
    /// What the model is asked to do, written before the evidence.
    pub(super) task: String,
    /// Written on the line above the evidence, e.g. `Passages:`. May be empty.
    pub(super) evidence_heading: String,
    pub(super) evidence: Vec<EvidencePassage>,
    pub(super) selection: EvidenceSelection,
    /// A ceiling the caller sets on evidence below what the window allows,
    /// for work whose cost it wants bounded on a large model.
    pub(super) evidence_limit: Option<usize>,
    /// Written after the evidence, e.g. a response contract. May be empty.
    pub(super) closing: String,
    /// Sent whole ahead of the task; part of the request's fixed cost.
    pub(super) history: Vec<HistoryItem>,
    /// Generation reservation, enforced on the request. Zero takes the
    /// planner's default for the model.
    pub(super) output_tokens: usize,
    /// How many times the planner's reservation, `output_tokens` or its
    /// default, this request is given. A larger send raises it.
    pub(super) output_scale: usize,
    /// The answer may also use the input room the request leaves unused, so a
    /// reasoning model writing long structured output is not cut off by an
    /// estimate. `output_tokens` is then the least it is promised.
    pub(super) output_fills_window: bool,
    /// Constrains the answer to this schema.
    pub(super) schema: Option<Schema>,
    pub call: CallOptions,
}

impl GroundedRequest {
    /// A request with every optional part empty.
    pub(super) fn of(instructions: String, task: String) -> Self {
        Self {
            instructions,
            task,
            evidence_heading: String::new(),
            evidence: Vec::new(),
            selection: EvidenceSelection::All,
            evidence_limit: None,
            closing: String::new(),
            history: Vec::new(),
            output_tokens: 0,
            output_scale: 1,
            output_fills_window: false,
            schema: None,
            call: CallOptions::default(),
        }
    }

    pub fn instructions(&self) -> &str {
        &self.instructions
    }

    pub fn task(&self) -> &str {
        &self.task
    }

    pub fn evidence_heading(&self) -> &str {
        &self.evidence_heading
    }

    pub fn evidence(&self) -> &[EvidencePassage] {
        &self.evidence
    }

    pub fn selection(&self) -> EvidenceSelection {
        self.selection
    }

    pub fn evidence_limit(&self) -> Option<usize> {
        self.evidence_limit
    }

    pub fn closing(&self) -> &str {
        &self.closing
    }

    pub fn history(&self) -> &[HistoryItem] {
        &self.history
    }

    pub fn output_tokens(&self) -> usize {
        self.output_tokens
    }

    pub fn output_scale(&self) -> usize {
        self.output_scale
    }

    pub fn output_fills_window(&self) -> bool {
        self.output_fills_window
    }

    /// The schema the answer is held to.
    pub fn schema(&self) -> Option<&Schema> {
        self.schema.as_ref()
    }

    /// The request sent for this grounded call, with the planner's `input` and
    /// output ceiling.
    pub(crate) fn completion(
        &self,
        input: Vec<CompletionInput>,
        max_output_tokens: Option<u32>,
    ) -> CompletionRequest {
        let call = self.call.clone();
        CompletionRequest {
            input,
            priority: call.priority,
            cancel: call.cancel,
            cache_key: call.cache_key,
            sampling: call.sampling,
            time_budget: call.time_budget,
            no_time_limit: call.no_time_limit,
            schema: self.schema.clone(),
            reasoning_effort: call.reasoning_effort,
            thinking: call.thinking,
            max_output_tokens,
            ..CompletionRequest::empty()
        }
    }
}
