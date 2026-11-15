//! One audit request: measured against the Verifier's window before it is
//! built into a batch ([`WindowSizer`]), then sent through the contract
//! driver ([`Audit`]).

use std::sync::{Mutex, PoisonError};

use async_trait::async_trait;

use lattice_core::application::ports::LLMPort;
use lattice_core::application::services::context_assembler::budget::{
    BudgetAllocation, BudgetRequest, ModelCapacity,
};
use lattice_core::application::services::grounded_generation::{self, GroundedRequest};
use lattice_core::application::services::model_contract::ask::{
    Answered, Followup, Sent, Transport,
};
use lattice_core::application::services::model_contract::packed::Packed;
use lattice_core::application::services::model_contract::shown::Scope;
use lattice_core::application::services::model_contract::{legacy, Contract, ContractId};
use lattice_core::shared::error::Result;

use super::batches::{Item, Sizer};
use super::contract::{Claim, Input};
use super::policy::{self, SYSTEM_PROMPT};
use super::tools;
use super::{AuditClaim, AuditUnit};

/// The audit's one model call.
pub(super) struct VerifierAudit;

impl Contract for VerifierAudit {
    const ID: ContractId = ContractId::new("verifier-audit");
    type Input = Input;
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

    /// What `items` ask of the Verifier, before any page is fetched.
    pub(super) fn input(&self, items: &[Item]) -> Input {
        let claims = items
            .iter()
            .filter_map(|item| Claim::of(self.claims, self.units, item))
            .collect();
        Input::of(claims, self.units, Packed::default(), self.reserve(items.len()))
    }

    fn task(&self, items: &[Item]) -> String {
        let mut scope = Scope::for_question();
        let mut showing = scope.request();
        VerifierAudit::says(&self.input(items), &mut showing)
            .map(|says| says.task)
            .unwrap_or_default()
    }

    fn task_tokens(&self, items: &[Item]) -> usize {
        self.llm.count_tokens(&self.task(items))
    }
}

impl Sizer for WindowSizer<'_> {
    /// Measured by the planner the batch is sent through, so a batch that
    /// fits here is never refused there; and the answer's reservation must
    /// be kept whole, not clamped.
    fn fits(&self, items: &[Item]) -> bool {
        let reserve = self.reserve(items.len());
        let task = self.task(items);
        if self.tool_tokens > 0 {
            return tools::opening_fits(self.llm, SYSTEM_PROMPT, task, self.tool_tokens, reserve);
        }
        let capacity = ModelCapacity::new(self.llm.model_name(), self.llm.max_context_tokens())
            .with_output_limit(reserve);
        let answer_kept = BudgetAllocation::plan(&capacity, &BudgetRequest::new(0, 0, 0))
            .is_ok_and(|allocation| allocation.output_reserved >= reserve);
        let mut request = legacy::grounded(SYSTEM_PROMPT, task);
        *legacy::grounded_says(&mut request).output_tokens = reserve;
        answer_kept && grounded_generation::evidence_room(self.llm, &request).is_ok()
    }

    fn needed(&self, items: &[Item]) -> usize {
        self.system_tokens + self.tool_tokens + self.task_tokens(items) + self.reserve(items.len())
    }
}

/// The audit's transport: each request sent as it is, its tokens counted.
#[derive(Default)]
pub(super) struct Audit {
    spent: Mutex<(u64, u64)>,
}

impl Audit {
    /// The input and output tokens the requests sent through this transport cost.
    pub(super) fn into_spent(self) -> (u64, u64) {
        self.spent
            .into_inner()
            .unwrap_or_else(PoisonError::into_inner)
    }

    async fn generate(&self, llm: &dyn LLMPort, request: GroundedRequest) -> Result<Sent> {
        let output = grounded_generation::generate(llm, request.clone()).await?;
        {
            let mut spent = self.spent.lock().unwrap_or_else(PoisonError::into_inner);
            spent.0 += output.input_tokens;
            spent.1 += output.output_tokens;
        }
        Ok(Sent::of(request, output))
    }
}

#[async_trait]
impl Transport for Audit {
    async fn send(&self, llm: &dyn LLMPort, request: GroundedRequest) -> Result<Sent> {
        self.generate(llm, request).await
    }

    async fn follow(&self, llm: &dyn LLMPort, step: Followup) -> Result<Sent> {
        self.generate(llm, step.into_request()).await
    }
}
