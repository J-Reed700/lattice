//! One audit request: measured against the Verifier's window before it is
//! built into a batch ([`WindowSizer`]), then sent with or without the web
//! tools ([`ask`]).

use lattice_core::application::ports::llm_port::{InferencePriority, SamplingOverride};
use lattice_core::application::ports::LLMPort;
use lattice_core::application::services::context_assembler::budget::{
    BudgetAllocation, BudgetRequest, ModelCapacity,
};
use lattice_core::application::services::grounded_generation::{
    self, CallOptions, GroundedGenerationError, GroundedRequest,
};
use lattice_core::shared::error::AppError;

pub(super) use crate::features::learning::lessons::structured_call::cut_off;

use super::batches::{Item, Sizer};
use super::policy::{self, Rendered, SYSTEM_PROMPT};
use super::tools::{self, VerifierWeb};
use super::{AuditClaim, AuditPassage, AuditUnit};

/// What one request came back with.
#[derive(Debug, Default)]
pub(super) struct Answer {
    pub text: String,
    /// The answer stopped at its output limit.
    pub cut_off: bool,
    pub input_tokens: u64,
    pub output_tokens: u64,
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
        reasoning_effort: Some("low".into()),
        ..Default::default()
    };
    match grounded_generation::generate(llm, request).await {
        Ok(output) => Ok(Answer {
            cut_off: cut_off(&output.finish_reason),
            text: output.text,
            input_tokens: output.input_tokens,
            output_tokens: output.output_tokens,
            ..Answer::default()
        }),
        Err(GroundedGenerationError::Budget(_)) => Err(AskError::TooLarge),
        Err(GroundedGenerationError::Model(error)) => Err(error.into()),
    }
}
