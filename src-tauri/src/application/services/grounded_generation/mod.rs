//! One path from gathered evidence to a model's answer.
//!
//! A generator hands over its instructions, its task and the evidence it
//! gathered, each passage under a stable id. [`BudgetAllocation`] divides the
//! model's window — instructions, history, evidence and the output
//! reservation — and this service selects the evidence that fits, renders one
//! typed request, calls the model at the caller's priority, and, when asked,
//! checks claims in the answer against the evidence it was given.
//!
//! Nothing is clipped. A request whose fixed parts, or whose required
//! evidence, do not fit is refused with a typed [`PromptBudgetExceeded`], and
//! evidence that selection left out is reported by id rather than dropped
//! quietly.

use std::sync::Arc;
use std::time::Duration;

use tokio_util::sync::CancellationToken;

use crate::application::ports::llm_port::{
    CompletionInput, CompletionRequest, InferencePriority, SamplingOverride,
};
use crate::application::ports::LLMPort;
use crate::application::services::claim_verification::{CheckPolicy, ClaimChecker, ClaimJudgment};
use crate::application::services::context_assembler::{
    BudgetAllocation, BudgetError, BudgetRequest, ContextAccounting, EvidenceBudget, HistoryCharge,
    ModelCapacity, PromptBudgetExceeded, TokenAccounting, MESSAGE_FRAMING_TOKENS,
};
use crate::shared::error::AppError;

/// Output allowance for one claim verdict: a label and two short lines.
const JUDGE_MAX_OUTPUT_TOKENS: u32 = 512;

/// One piece of evidence, under the id its caller resolves it by.
#[derive(Debug, Clone, PartialEq)]
pub struct EvidencePassage {
    /// Stable id: a chunk id, a message id. Reported back in
    /// [`GroundedOutput::used_evidence_ids`] when the passage was carried.
    pub id: String,
    /// Written before the text, e.g. `[3]` or `USER:`. May be empty.
    pub label: String,
    pub text: String,
    /// Higher is kept longer under [`EvidenceSelection::BestFirst`]. Ties keep
    /// input order.
    pub rank: f32,
}

impl EvidencePassage {
    fn render(&self) -> String {
        if self.label.is_empty() {
            self.text.clone()
        } else {
            format!("{} {}", self.label, self.text)
        }
    }
}

/// `text` as passages of at most `max_chars`, in order, for a caller whose
/// evidence is one long text: paragraphs where it has them, and long
/// paragraphs cut at whitespace. With [`EvidenceSelection::Prefix`] the model
/// then reads as much of the opening as the window holds, ending on a whole
/// passage. Ids are `{id_prefix}:{n}`; earlier passages rank higher.
pub(crate) fn passages_from_text(
    id_prefix: &str,
    text: &str,
    max_chars: usize,
) -> Vec<EvidencePassage> {
    let max_chars = max_chars.max(1);
    let mut pieces: Vec<String> = Vec::new();
    for paragraph in text.split("\n\n").map(str::trim).filter(|p| !p.is_empty()) {
        let mut rest = paragraph;
        while rest.chars().count() > max_chars {
            let end = rest
                .char_indices()
                .nth(max_chars)
                .map_or(rest.len(), |(index, _)| index);
            let cut = rest
                .get(..end)
                .and_then(|head| head.rfind(char::is_whitespace))
                .filter(|space| *space > 0)
                .unwrap_or(end);
            let (head, tail) = rest.split_at(cut);
            pieces.push(head.trim().to_string());
            rest = tail.trim_start();
        }
        if !rest.is_empty() {
            pieces.push(rest.to_string());
        }
    }
    let count = pieces.len().max(1) as f32;
    pieces
        .into_iter()
        .enumerate()
        .map(|(index, text)| EvidencePassage {
            id: format!("{id_prefix}:{index}"),
            label: String::new(),
            text,
            rank: 1.0 - index as f32 / count,
        })
        .collect()
}

/// What happens to evidence the window cannot hold.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum EvidenceSelection {
    /// Every passage is carried, or the request is refused.
    #[default]
    All,
    /// Highest rank first: a passage that does not fit is left out and the
    /// next is tried.
    BestFirst,
    /// In the order given, stopping at the first that does not fit, so what is
    /// carried is a contiguous opening of the material.
    Prefix,
}

/// A message of earlier conversation or memory sent ahead of the task.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryItem {
    pub role: String,
    pub content: String,
}

/// How the call queues at the backend and how it samples.
#[derive(Debug, Clone, Default)]
pub struct CallOptions {
    pub priority: InferencePriority,
    pub cancel: Option<CancellationToken>,
    pub cache_key: Option<String>,
    pub sampling: Option<SamplingOverride>,
    pub time_budget: Option<Duration>,
}

/// Claims to check against the evidence the answer was given.
pub(crate) struct ClaimCheck {
    pub policy: CheckPolicy,
    /// Checked in order. Empty checks the whole answer as one claim.
    pub claims: Vec<String>,
    /// The judge. `None` lets the generating model judge its own answer.
    pub judge: Option<Arc<dyn LLMPort>>,
}

/// Everything one grounded call needs.
pub(crate) struct GroundedRequest {
    /// The system message.
    pub instructions: String,
    /// What the model is asked to do, written before the evidence.
    pub task: String,
    /// Written on the line above the evidence, e.g. `Passages:`. May be empty.
    pub evidence_heading: String,
    pub evidence: Vec<EvidencePassage>,
    pub selection: EvidenceSelection,
    /// A ceiling the caller sets on evidence below what the window allows,
    /// for work whose cost it wants bounded on a large model.
    pub evidence_limit: Option<usize>,
    /// Written after the evidence, e.g. a response contract. May be empty.
    pub closing: String,
    /// Sent whole ahead of the task; part of the request's fixed cost.
    pub history: Vec<HistoryItem>,
    /// Generation reservation, enforced on the request.
    pub output_tokens: usize,
    pub call: CallOptions,
    pub verification: Option<ClaimCheck>,
}

impl GroundedRequest {
    /// A request with every optional part empty.
    pub fn new(instructions: impl Into<String>, task: impl Into<String>) -> Self {
        Self {
            instructions: instructions.into(),
            task: task.into(),
            evidence_heading: String::new(),
            evidence: Vec::new(),
            selection: EvidenceSelection::All,
            evidence_limit: None,
            closing: String::new(),
            history: Vec::new(),
            output_tokens: 0,
            call: CallOptions::default(),
            verification: None,
        }
    }
}

/// One claim and the judge's ruling on it.
#[derive(Debug, Clone)]
// Read by the generators that ask for a claim check; this wave's callers do not.
#[allow(dead_code)]
pub(crate) struct ClaimVerdictRecord {
    pub claim: String,
    pub judgment: ClaimJudgment,
}

/// The answer, and what it was built from.
#[derive(Debug, Clone)]
pub(crate) struct GroundedOutput {
    pub text: String,
    /// Ids of the passages the request carried, in the order given.
    pub used_evidence_ids: Vec<String>,
    pub accounting: ContextAccounting,
    /// Present when the request asked for a claim check.
    // Read by the generators that ask for a claim check; this wave's callers do not.
    #[allow(dead_code)]
    pub verdicts: Option<Vec<ClaimVerdictRecord>>,
}

#[derive(Debug, thiserror::Error)]
pub enum GroundedGenerationError {
    /// The window cannot hold the request: the model is too small, or the
    /// fixed parts or required evidence overflow.
    #[error(transparent)]
    Budget(#[from] BudgetError),
    /// The model call failed.
    #[error(transparent)]
    Model(AppError),
}

impl GroundedGenerationError {
    /// The overflow, when that is why the request was refused.
    pub fn overflow(&self) -> Option<&PromptBudgetExceeded> {
        match self {
            Self::Budget(BudgetError::Overflow(overflow)) => Some(overflow),
            _ => None,
        }
    }
}

impl From<GroundedGenerationError> for AppError {
    fn from(error: GroundedGenerationError) -> Self {
        match error {
            GroundedGenerationError::Budget(error) => error.into(),
            GroundedGenerationError::Model(error) => error,
        }
    }
}

/// The evidence room a request would get before any evidence is added: for a
/// caller that must split its material into parts the window can hold.
pub(crate) fn evidence_room(
    llm: &dyn LLMPort,
    request: &GroundedRequest,
) -> Result<usize, GroundedGenerationError> {
    Ok(plan(llm, request)?.budget.total())
}

/// The blank line written between two passages.
const PASSAGE_SEPARATOR_TOKENS: usize = 1;

/// The cost a passage is charged when carried, its separator included.
pub(crate) fn passage_tokens(llm: &dyn LLMPort, passage: &EvidencePassage) -> usize {
    llm.count_tokens(&passage.render()) + PASSAGE_SEPARATOR_TOKENS
}

struct Planned {
    allocation: BudgetAllocation,
    budget: EvidenceBudget,
    instruction_tokens: usize,
    task_tokens: usize,
    history_tokens: usize,
}

fn message_tokens(llm: &dyn LLMPort, role: &str, content: &str) -> usize {
    llm.count_tokens(role) + llm.count_tokens(content) + MESSAGE_FRAMING_TOKENS
}

fn plan(llm: &dyn LLMPort, request: &GroundedRequest) -> Result<Planned, GroundedGenerationError> {
    let instruction_tokens = if request.instructions.trim().is_empty() {
        0
    } else {
        message_tokens(llm, "system", &request.instructions)
    };
    let task_tokens = message_tokens(
        llm,
        "user",
        &[
            request.task.as_str(),
            request.evidence_heading.as_str(),
            request.closing.as_str(),
        ]
        .join("\n\n"),
    );
    let history_tokens: usize = request
        .history
        .iter()
        .map(|item| message_tokens(llm, &item.role, &item.content))
        .sum();
    let mut capacity = ModelCapacity::new(llm.model_name(), llm.max_context_tokens())
        .with_accounting(TokenAccounting::Estimated);
    if request.output_tokens > 0 {
        capacity = capacity.with_output_limit(request.output_tokens);
    }
    let allocation = BudgetAllocation::plan(
        &capacity,
        &BudgetRequest::new(instruction_tokens, 0, task_tokens)
            .with_history(HistoryCharge::Carried(history_tokens)),
    )?;
    // History is sent whole, so a history the window cannot hold is the same
    // refusal as an instruction that does not fit.
    if history_tokens > allocation.available {
        return Err(BudgetError::Overflow(PromptBudgetExceeded {
            required: allocation.fixed + history_tokens,
            available: allocation.input_budget,
        })
        .into());
    }
    let room = request
        .evidence_limit
        .map_or(allocation.evidence, |limit| limit.min(allocation.evidence));
    Ok(Planned {
        budget: EvidenceBudget::new(room),
        allocation,
        instruction_tokens,
        task_tokens,
        history_tokens,
    })
}

/// Indices of the passages carried, in input order.
fn select(
    passages: &[EvidencePassage],
    costs: &[usize],
    selection: EvidenceSelection,
    budget: &mut EvidenceBudget,
) -> Vec<usize> {
    let mut kept = Vec::new();
    match selection {
        EvidenceSelection::All => {
            let total: usize = costs.iter().sum();
            if total <= budget.remaining() {
                budget.charge(total);
                kept.extend(0..passages.len());
            }
        }
        EvidenceSelection::Prefix => {
            for (index, cost) in costs.iter().enumerate() {
                if *cost > budget.remaining() {
                    break;
                }
                budget.charge(*cost);
                kept.push(index);
            }
        }
        EvidenceSelection::BestFirst => {
            let mut order: Vec<usize> = (0..passages.len()).collect();
            // Stable, so equal ranks keep the caller's order.
            order.sort_by(|a, b| {
                let rank = |index: &usize| passages.get(*index).map_or(0.0, |p| p.rank);
                rank(b)
                    .partial_cmp(&rank(a))
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            let ranked_costs: Vec<(usize, usize)> = order
                .into_iter()
                .filter_map(|index| costs.get(index).map(|cost| (index, *cost)))
                .collect();
            kept = budget
                .select(&ranked_costs, |(_, cost)| *cost)
                .into_iter()
                .map(|(index, _)| *index)
                .collect();
            kept.sort_unstable();
        }
    }
    kept
}

fn render_user_message(request: &GroundedRequest, carried: &[&EvidencePassage]) -> String {
    let body = carried
        .iter()
        .map(|passage| passage.render())
        .collect::<Vec<_>>()
        .join("\n\n");
    let evidence = match (body.is_empty(), request.evidence_heading.is_empty()) {
        (true, _) => String::new(),
        (false, true) => body,
        (false, false) => format!("{}\n{body}", request.evidence_heading),
    };
    [
        request.task.as_str(),
        evidence.as_str(),
        request.closing.as_str(),
    ]
    .into_iter()
    .filter(|part| !part.is_empty())
    .collect::<Vec<_>>()
    .join("\n\n")
}

/// Plan, select, call and optionally check one grounded request.
///
/// # Errors
///
/// - [`GroundedGenerationError::Budget`] when the model cannot hold the
///   request: instructions, task and history too large, evidence the caller
///   required whole that does not fit, or evidence of which nothing fits.
/// - [`GroundedGenerationError::Model`] when the call fails or answers with a
///   tool call nobody offered.
pub(crate) async fn generate(
    llm: &dyn LLMPort,
    request: GroundedRequest,
) -> Result<GroundedOutput, GroundedGenerationError> {
    let Planned {
        allocation,
        mut budget,
        instruction_tokens,
        task_tokens,
        history_tokens,
    } = plan(llm, &request)?;

    let costs: Vec<usize> = request
        .evidence
        .iter()
        .map(|passage| passage_tokens(llm, passage))
        .collect();
    let kept = select(&request.evidence, &costs, request.selection, &mut budget);
    // Evidence was the point of the call. Carrying none of it — or less than
    // the caller required — would answer ungrounded under a grounded name.
    if !request.evidence.is_empty() && kept.is_empty() {
        let needed = match request.selection {
            EvidenceSelection::All => costs.iter().sum(),
            EvidenceSelection::Prefix => costs.first().copied().unwrap_or(0),
            EvidenceSelection::BestFirst => costs.iter().copied().min().unwrap_or(0),
        };
        return Err(BudgetError::Overflow(PromptBudgetExceeded {
            required: allocation.fixed + history_tokens + needed,
            available: allocation.fixed + history_tokens + budget.total(),
        })
        .into());
    }

    let carried: Vec<&EvidencePassage> = kept
        .iter()
        .filter_map(|index| request.evidence.get(*index))
        .collect();
    let mut accounting = ContextAccounting::from_allocation(&allocation, llm.model_name());
    accounting.fixed_policy = instruction_tokens;
    accounting.current_input = task_tokens;
    accounting.recent_history = history_tokens;
    accounting.document_evidence = budget.total() - budget.remaining();
    accounting.total_input =
        instruction_tokens + task_tokens + history_tokens + accounting.document_evidence;
    accounting.evicted = request
        .evidence
        .iter()
        .enumerate()
        .filter(|(index, _)| !kept.contains(index))
        .map(|(_, passage)| format!("evidence:{}", passage.id))
        .collect();

    let mut input = Vec::new();
    if instruction_tokens > 0 {
        input.push(CompletionInput::Message {
            role: "system".into(),
            content: request.instructions.clone(),
        });
    }
    input.extend(request.history.iter().map(|item| CompletionInput::Message {
        role: item.role.clone(),
        content: item.content.clone(),
    }));
    input.push(CompletionInput::Message {
        role: "user".into(),
        content: render_user_message(&request, &carried),
    });

    let call = request.call.clone();
    let response = llm
        .complete(&CompletionRequest {
            input,
            priority: call.priority,
            cancel: call.cancel,
            cache_key: call.cache_key,
            sampling: call.sampling,
            time_budget: call.time_budget,
            max_output_tokens: u32::try_from(allocation.output_reserved).ok(),
            ..Default::default()
        })
        .await
        .map_err(GroundedGenerationError::Model)?;
    if !response.tool_calls.is_empty() {
        return Err(GroundedGenerationError::Model(AppError::InvalidState(
            "Unexpected tool call in grounded generation".into(),
        )));
    }

    let verdicts = match &request.verification {
        Some(check) => Some(verify(llm, check, &response.text, &carried).await),
        None => None,
    };
    Ok(GroundedOutput {
        text: response.text,
        used_evidence_ids: carried.iter().map(|passage| passage.id.clone()).collect(),
        accounting,
        verdicts,
    })
}

async fn verify(
    llm: &dyn LLMPort,
    check: &ClaimCheck,
    answer: &str,
    carried: &[&EvidencePassage],
) -> Vec<ClaimVerdictRecord> {
    let judge = check.judge.as_deref().unwrap_or(llm);
    let checker = ClaimChecker::new(
        judge,
        SamplingOverride::deterministic(),
        JUDGE_MAX_OUTPUT_TOKENS,
        check.policy,
    );
    let passages: Vec<String> = carried.iter().map(|passage| passage.text.clone()).collect();
    let claims: Vec<String> = if check.claims.is_empty() {
        vec![answer.trim().to_string()]
    } else {
        check.claims.clone()
    };
    let mut verdicts = Vec::with_capacity(claims.len());
    for claim in claims {
        let judgment = checker
            .check_passages_without_deadline(&claim, &passages)
            .await;
        verdicts.push(ClaimVerdictRecord { claim, judgment });
    }
    verdicts
}

#[cfg(test)]
mod tests;
