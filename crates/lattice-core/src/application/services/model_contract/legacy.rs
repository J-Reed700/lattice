//! The old way to build a request and to change what one says, for the calls
//! that have not moved onto [`super::ask`]. Every use outside tests is counted
//! by `scripts/rust-architecture-check` against a baseline that may only
//! shrink; this module is deleted when the count is zero.
//!
//! Written `legacy::…` at every use, never imported by item, so the count is
//! the number of uses.

use std::time::Duration;

use tokio_util::sync::CancellationToken;

use super::request::{CompletionRequest, GroundedRequest};
use super::shape::Schema;
use super::ContractCall;
use crate::application::ports::llm_port::{
    CompletionInput, InferencePriority, SamplingOverride, ThinkingMode, ToolChoice, ToolDefinition,
};
use crate::application::services::grounded_generation::{
    EvidencePassage, EvidenceSelection, HistoryItem,
};

/// The fields a [`CompletionRequest`] had while they were public.
#[derive(Debug, Clone, Default)]
pub struct Completion {
    pub input: Vec<CompletionInput>,
    pub tools: Vec<ToolDefinition>,
    pub json_schema: Option<serde_json::Value>,
    pub reasoning_effort: Option<String>,
    pub contract: Option<ContractCall>,
    pub tool_choice: Option<ToolChoice>,
    pub include_reasoning: bool,
    pub sampling: Option<SamplingOverride>,
    pub thinking: ThinkingMode,
    pub max_output_tokens: Option<u32>,
    pub stop: Vec<String>,
    pub time_budget: Option<Duration>,
    pub no_time_limit: bool,
    pub want_logprobs: bool,
    pub priority: InferencePriority,
    pub cancel: Option<CancellationToken>,
    pub cache_key: Option<String>,
}

/// The request with these fields.
pub fn completion(fields: Completion) -> CompletionRequest {
    CompletionRequest {
        input: fields.input,
        tools: fields.tools,
        schema: fields.json_schema.map(schema),
        reasoning_effort: fields.reasoning_effort,
        contract: fields.contract,
        tool_choice: fields.tool_choice,
        include_reasoning: fields.include_reasoning,
        sampling: fields.sampling,
        thinking: fields.thinking,
        max_output_tokens: fields.max_output_tokens,
        stop: fields.stop,
        time_budget: fields.time_budget,
        no_time_limit: fields.no_time_limit,
        want_logprobs: fields.want_logprobs,
        priority: fields.priority,
        cancel: fields.cancel,
        cache_key: fields.cache_key,
    }
}

/// A request taken apart into the fields it had.
pub fn fields(request: CompletionRequest) -> Completion {
    Completion {
        input: request.input,
        tools: request.tools,
        json_schema: request.schema.map(Schema::into_json),
        reasoning_effort: request.reasoning_effort,
        contract: request.contract,
        tool_choice: request.tool_choice,
        include_reasoning: request.include_reasoning,
        sampling: request.sampling,
        thinking: request.thinking,
        max_output_tokens: request.max_output_tokens,
        stop: request.stop,
        time_budget: request.time_budget,
        no_time_limit: request.no_time_limit,
        want_logprobs: request.want_logprobs,
        priority: request.priority,
        cancel: request.cancel,
        cache_key: request.cache_key,
    }
}

/// What a [`CompletionRequest`] says, open to change.
pub struct Says<'a> {
    pub input: &'a mut Vec<CompletionInput>,
    pub tools: &'a mut Vec<ToolDefinition>,
    pub schema: &'a mut Option<Schema>,
    pub tool_choice: &'a mut Option<ToolChoice>,
    pub max_output_tokens: &'a mut Option<u32>,
    pub stop: &'a mut Vec<String>,
}

pub fn says(request: &mut CompletionRequest) -> Says<'_> {
    Says {
        input: &mut request.input,
        tools: &mut request.tools,
        schema: &mut request.schema,
        tool_choice: &mut request.tool_choice,
        max_output_tokens: &mut request.max_output_tokens,
        stop: &mut request.stop,
    }
}

/// A schema written as JSON.
pub fn schema(json: serde_json::Value) -> Schema {
    Schema::made(json)
}

/// A grounded request with every optional part empty.
pub fn grounded(instructions: impl Into<String>, task: impl Into<String>) -> GroundedRequest {
    GroundedRequest::of(instructions.into(), task.into())
}

/// What a [`GroundedRequest`] says, open to change.
pub struct GroundedSays<'a> {
    pub instructions: &'a mut String,
    pub task: &'a mut String,
    pub evidence_heading: &'a mut String,
    pub evidence: &'a mut Vec<EvidencePassage>,
    pub selection: &'a mut EvidenceSelection,
    pub evidence_limit: &'a mut Option<usize>,
    pub closing: &'a mut String,
    pub history: &'a mut Vec<HistoryItem>,
    pub output_tokens: &'a mut usize,
    pub output_fills_window: &'a mut bool,
    pub schema: &'a mut Option<Schema>,
}

pub fn grounded_says(request: &mut GroundedRequest) -> GroundedSays<'_> {
    GroundedSays {
        instructions: &mut request.instructions,
        task: &mut request.task,
        evidence_heading: &mut request.evidence_heading,
        evidence: &mut request.evidence,
        selection: &mut request.selection,
        evidence_limit: &mut request.evidence_limit,
        closing: &mut request.closing,
        history: &mut request.history,
        output_tokens: &mut request.output_tokens,
        output_fills_window: &mut request.output_fills_window,
        schema: &mut request.schema,
    }
}
