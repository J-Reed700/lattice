//! One audit request: measured against the Verifier's window before it is
//! built into a batch ([`WindowSizer`]), then sent with or without the web
//! tools ([`ask`]) through the contract driver ([`Audit`]).

use std::sync::{Mutex, PoisonError};

use async_trait::async_trait;

use lattice_core::application::ports::llm_port::{
    CompletionResponse, Finish, InferencePriority, SamplingOverride, ANSWER_EFFORT,
};
use lattice_core::application::ports::LLMPort;
use lattice_core::application::services::context_assembler::budget::{
    BudgetAllocation, BudgetRequest, ModelCapacity,
};
use lattice_core::application::services::grounded_generation::{
    self, CallOptions, GroundedGenerationError, GroundedRequest,
};
use lattice_core::application::services::model_contract::{
    Contract, ContractId, PromptInput, Transport, CORRECTION_LEAD,
};
use lattice_core::shared::error::AppError;

use super::batches::{Item, Sizer};
use super::policy::{self, Rendered, SYSTEM_PROMPT};
use super::tools::{self, VerifierWeb};
use super::{AuditClaim, AuditPassage, AuditUnit};

/// The audit's one model call.
pub(super) struct VerifierAudit;

impl Contract for VerifierAudit {
    const ID: ContractId = ContractId::new("verifier-audit");
    type Input = PromptInput;
}

/// What one request came back with: the final reply, carrying the tokens of
/// every round the request took.
#[derive(Debug, Default)]
pub(super) struct Answer {
    pub response: CompletionResponse,
    /// Pages fetched and saved during the request, by label.
    pub fetched: Vec<(String, AuditPassage)>,
    /// Why web search could not be used, when it could not.
    pub web_note: Option<String>,
}

#[derive(Debug)]
pub(super) enum AskError {
    /// The window cannot hold the request with its answer.
    TooLarge,
    Model(AppError),
}

impl From<AppError> for AskError {
    /// The backend refusing a prompt its window cannot hold halves the batch
    /// rather than failing the job: dense batches can be refused though the
    /// estimate fit them.
    fn from(error: AppError) -> Self {
        match error {
            AppError::PromptExceedsWindow { .. } => Self::TooLarge,
            error => Self::Model(error),
        }
    }
}

/// `text`, cut at a character boundary to at most `tokens` as `llm` counts.
pub(super) fn fit_text(llm: &dyn LLMPort, text: &str, tokens: usize) -> String {
    let mut kept = text;
    while !kept.is_empty() && llm.count_tokens(kept) > tokens {
        let counted = llm.count_tokens(kept).max(1);
        let target = kept.len().saturating_mul(tokens) / counted;
        let mut end = target.min(kept.len().saturating_sub(1));
        while end > 0 && !kept.is_char_boundary(end) {
            end -= 1;
        }
        kept = kept.get(..end).unwrap_or_default();
    }
    kept.to_owned()
}

/// Measures batches for one audit against the pinned Verifier.
pub(super) struct WindowSizer<'a> {
    pub llm: &'a dyn LLMPort,
    pub claims: &'a [AuditClaim],
    pub units: &'a [AuditUnit],
    /// The tool schemas' tokens when web search is offered, else 0.
    pub tool_tokens: usize,
    verdict_tokens: usize,
    envelope_tokens: usize,
    system_tokens: usize,
}

impl<'a> WindowSizer<'a> {
    pub(super) fn new(
        llm: &'a dyn LLMPort,
        claims: &'a [AuditClaim],
        units: &'a [AuditUnit],
        web: bool,
    ) -> Self {
        let tool_tokens = if web {
            llm.count_tokens(&serde_json::to_string(&tools::definitions()).unwrap_or_default())
        } else {
            0
        };
        Self {
            llm,
            claims,
            units,
            tool_tokens,
            verdict_tokens: policy::verdict_reserve(llm),
            envelope_tokens: policy::envelope_reserve(llm),
            system_tokens: llm.count_tokens(SYSTEM_PROMPT),
        }
    }

    /// The answer reservation for `items`: the envelope and one largest
    /// verdict per item.
    pub(super) fn reserve(&self, items: usize) -> usize {
        self.envelope_tokens + items * self.verdict_tokens
    }

    fn task_tokens(&self, items: &[Item]) -> usize {
        self.llm
            .count_tokens(&policy::render(self.claims, self.units, items).task)
    }
}

impl Sizer for WindowSizer<'_> {
    /// Measured by the planner the batch is sent through, so a batch that
    /// fits here is never refused there; and the answer's reservation must
    /// be kept whole, not clamped.
    fn fits(&self, items: &[Item]) -> bool {
        let reserve = self.reserve(items.len());
        let task = policy::render(self.claims, self.units, items).task;
        if self.tool_tokens > 0 {
            return tools::opening_fits(self.llm, SYSTEM_PROMPT, task, self.tool_tokens, reserve);
        }
        let capacity = ModelCapacity::new(self.llm.model_name(), self.llm.max_context_tokens())
            .with_output_limit(reserve);
        let answer_kept = BudgetAllocation::plan(&capacity, &BudgetRequest::new(0, 0, 0))
            .is_ok_and(|allocation| allocation.output_reserved >= reserve);
        let mut request = GroundedRequest::new(SYSTEM_PROMPT, task);
        request.output_tokens = reserve;
        answer_kept && grounded_generation::evidence_room(self.llm, &request).is_ok()
    }

    fn needed(&self, items: &[Item]) -> usize {
        self.system_tokens + self.tool_tokens + self.task_tokens(items) + self.reserve(items.len())
    }
}

/// Send one batch: with the web tools when `web` is given, else as one
/// grounded request the planner checks before sending.
pub(super) async fn ask(
    llm: &dyn LLMPort,
    rendered: &Rendered,
    reserve: usize,
    web: Option<&dyn VerifierWeb>,
) -> Result<Answer, AskError> {
    let schema = policy::schema(rendered, web.is_some());
    if let Some(web) = web {
        return tools::ask(
            llm,
            SYSTEM_PROMPT,
            rendered.task.clone(),
            schema,
            reserve,
            web,
        )
        .await;
    }
    let mut request = GroundedRequest::new(SYSTEM_PROMPT, rendered.task.clone());
    request.output_tokens = reserve;
    request.call = CallOptions {
        priority: InferencePriority::Verification,
        sampling: Some(SamplingOverride::deterministic()),
        no_time_limit: true,
        json_schema: Some(schema),
        reasoning_effort: Some(ANSWER_EFFORT.into()),
        ..Default::default()
    };
    match grounded_generation::generate(llm, request).await {
        Ok(output) => Ok(Answer {
            response: CompletionResponse {
                text: output.text,
                finish_reason: output.finish_reason,
                input_tokens: output.input_tokens,
                output_tokens: output.output_tokens,
                ..Default::default()
            },
            ..Answer::default()
        }),
        Err(GroundedGenerationError::Budget(_)) => Err(AskError::TooLarge),
        Err(GroundedGenerationError::Model(error)) => Err(error.into()),
    }
}

/// What the rounds of one batch spent, and what the Verifier fetched.
#[derive(Default)]
pub(super) struct Spent {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub web_note: Option<String>,
    pub fetched: Vec<(String, AuditPassage)>,
}

/// The audit's transport: one batch sent with its answer reserved, asked
/// again once with the refusal quoted.
pub(super) struct Audit<'a> {
    pub reserve: usize,
    pub web: Option<&'a dyn VerifierWeb>,
    pub items: usize,
    spent: Mutex<Spent>,
}

impl<'a> Audit<'a> {
    pub(super) fn new(reserve: usize, web: Option<&'a dyn VerifierWeb>, items: usize) -> Self {
        Self {
            reserve,
            web,
            items,
            spent: Mutex::default(),
        }
    }

    pub(super) fn fetched(&self) -> Vec<(String, AuditPassage)> {
        self.spent
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .fetched
            .clone()
    }

    pub(super) fn into_spent(self) -> Spent {
        self.spent
            .into_inner()
            .unwrap_or_else(PoisonError::into_inner)
    }
}

#[async_trait]
impl Transport for Audit<'_> {
    type Request = Rendered;

    async fn send(
        &self,
        llm: &dyn LLMPort,
        request: &Rendered,
    ) -> Result<CompletionResponse, AppError> {
        let answer = ask(llm, request, self.reserve, self.web)
            .await
            .map_err(|error| match error {
                AskError::Model(error) => error,
                AskError::TooLarge => AppError::WindowTooSmall(format!(
                    "The Verifier's window of {} tokens cannot hold this statement's section and                      passages with its answer.",
                    llm.max_context_tokens()
                )),
            })?;
        let mut spent = self.spent.lock().unwrap_or_else(PoisonError::into_inner);
        spent.input_tokens += answer.response.input_tokens;
        spent.output_tokens += answer.response.output_tokens;
        if answer.web_note.is_some() {
            spent.web_note = answer.web_note;
        }
        spent.fetched = answer.fetched;
        if answer.response.finish() == Finish::CutOff && self.items > 1 {
            return Err(AppError::WindowTooSmall(
                "The Verifier's answer was cut off at its output limit.".into(),
            ));
        }
        Ok(answer.response)
    }

    fn correction(
        &self,
        request: &Rendered,
        previous: &CompletionResponse,
        refusal: &str,
    ) -> Result<Rendered, AppError> {
        let mut next = request.clone();
        next.task = format!(
            "{}

Your previous answer:
{}

{CORRECTION_LEAD} {refusal}",
            request.task, previous.text
        );
        Ok(next)
    }

    fn larger(&self, _: &Rendered) -> Option<Rendered> {
        None
    }
}
