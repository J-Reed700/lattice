//! Building one turn's bounded, typed model input from memory plus recall.
//!
//! Design: `docs/design/2026-09-19-conversation-memory.md` §10, §18.
//!
//! This is the seam between the chat feature and the shared
//! [`ContextAssembler`]. The assembler owns the budget and the selection order
//! and knows nothing about Tauri, settings or providers; this module gathers the
//! inputs it needs and reports what happened.
//!
//! It runs only when `settings.llm.bounded_conversation_memory` is on (§18).
//! While the switch is off, the existing string-context path is untouched, and
//! [`build_memory_plan`] returns `None`. When enabled, even a conversation with
//! no ledger must be checked: its raw history may already exceed the budget.

use std::sync::Arc;

use lattice_core::application::ports::conversation_memory::{
    ConversationMemoryPort, ConversationMemoryReadPort, SourceReadLimits,
};
use lattice_core::application::ports::llm_port::chars_within_tokens;
use lattice_core::application::ports::LLMPort;
use lattice_core::application::services::context_assembler::{
    BudgetAllocation, ContextAssembler, ContextPlan, ContextRequest, InlineEvidence, ModelCapacity,
    RankedEvidence, TokenAccounting, TokenCounter,
};
use lattice_core::domain::conversation::memory::{MemorySnapshot, SourceMessage};
use lattice_core::shared::error::{AppError, Result};

use super::history_tools::{self, HistoryToolBudget, HistoryToolScope, RecallRequest};

/// Framing the assembler adds to every recent message beyond its words; a
/// candidate's cost is never below it, so it also bounds how many candidates
/// a pool can be worth reading.
const RECENT_MESSAGE_FRAMING_TOKENS: usize = 4;

/// Everything one assembled turn produced, plus what it could not do.
#[derive(Debug)]
pub struct MemoryTurnContext {
    pub plan: ContextPlan,
    /// The §9.3 availability flag: whether recall ran, found nothing, or was
    /// unavailable. Rendered whether or not anything was found, because "no
    /// hits" and "the index is down" lead to different answers.
    pub recall_note: &'static str,
}

/// Prepare the context that generation is allowed to use. Keeping orchestration
/// independent of the DI container lets tests exercise the actual failure gate.
/// A candidate requiring compaction is never a sendable plan.
pub async fn prepare_memory_turn<Build, BuildFuture, Compact, CompactFuture>(
    mut build: Build,
    compact: Compact,
) -> Result<Option<MemoryTurnContext>>
where
    Build: FnMut() -> BuildFuture,
    BuildFuture: std::future::Future<Output = Result<Option<MemoryTurnContext>>>,
    Compact: FnOnce() -> CompactFuture,
    CompactFuture: std::future::Future<Output = Result<()>>,
{
    let initial = match build().await {
        Err(AppError::ConcurrentModification { .. }) => build().await?,
        other => other?,
    };
    if !initial
        .as_ref()
        .is_some_and(|turn| turn.plan.compaction_required || !turn.plan.accounting.fits())
    {
        return Ok(initial);
    }

    compact()
        .await
        .map_err(|error| incomplete_history(Some(&error)))?;
    // Recheck even a successful no-op or partial commit. Neither proves that
    // the missing source is now represented, and there is only one pass per turn.
    let rebuilt = build().await?;
    match rebuilt {
        Some(turn) if !turn.plan.compaction_required && turn.plan.accounting.fits() => {
            Ok(Some(turn))
        }
        _ => Err(incomplete_history(None)),
    }
}

fn incomplete_history(cause: Option<&AppError>) -> AppError {
    let cause = cause.map(|error| format!(" {error}")).unwrap_or_default();
    AppError::InvalidState(format!(
        "This turn was stopped because earlier messages are not yet covered by conversation \
         memory and cannot all fit in the prompt. Your messages are saved.{cause} \
         Configure a utility model if needed, run /compact to process the remaining history \
         and retry, or choose a larger-context model."
    ))
}

/// Assemble a bounded typed plan for this turn, or `None` to leave the existing
/// path in charge.
///
/// `None` means only that the rollout switch is off. An empty or rebuilding
/// ledger must still account for raw source before permitting generation.
///
/// # Errors
///
/// Propagates the assembler's budget errors — an oversized current message, an
/// unusable model, or active requirements that do not fit. Those are reported to
/// the user rather than resolved by dropping a constraint.
pub async fn build_memory_plan(
    memory: &dyn ConversationMemoryPort,
    recall: &dyn ConversationMemoryReadPort,
    llm: &Arc<dyn LLMPort>,
    conversation_id: &str,
    system_policy: &str,
    current_input: &str,
    tool_schema_tokens: usize,
    document_evidence: Vec<RankedEvidence>,
    enabled: bool,
) -> Result<Option<MemoryTurnContext>> {
    build_memory_plan_for_query(
        memory,
        recall,
        llm,
        conversation_id,
        TurnPrompt {
            system_policy,
            current_input,
            inline_evidence: None,
            tool_schema_tokens,
            document_evidence,
        },
        enabled,
        current_input,
        chrono::Local::now().date_naive(),
    )
    .await
}

/// The turn's own part of the prompt, which the memory plan is built around.
pub struct TurnPrompt<'a> {
    pub system_policy: &'a str,
    /// The message as it reads with none of `inline_evidence`.
    pub current_input: &'a str,
    /// The evidence the caller rendered into the message; the assembler cuts
    /// it, lowest-ranked first, before it refuses a request that does not fit.
    pub inline_evidence: Option<InlineEvidence<'a>>,
    /// The room the tool schemas take, which ride along with every round.
    pub tool_schema_tokens: usize,
    pub document_evidence: Vec<RankedEvidence>,
}

/// [`build_memory_plan`] for a message that may carry its evidence inline.
/// `query` is the message as the user wrote it, which recall searches for.
pub async fn build_memory_plan_for_query(
    memory: &dyn ConversationMemoryPort,
    recall: &dyn ConversationMemoryReadPort,
    llm: &Arc<dyn LLMPort>,
    conversation_id: &str,
    prompt: TurnPrompt<'_>,
    enabled: bool,
    query: &str,
    today: chrono::NaiveDate,
) -> Result<Option<MemoryTurnContext>> {
    if !enabled {
        return Ok(None);
    }
    let TurnPrompt {
        system_policy,
        current_input,
        inline_evidence,
        tool_schema_tokens,
        document_evidence,
    } = prompt;

    let snapshot = memory.load_snapshot(conversation_id).await?;

    let counter = {
        let llm = Arc::clone(llm);
        Arc::new(move |text: &str| llm.count_tokens(text))
    };
    let assembler = ContextAssembler::new(counter);

    // `count_tokens` is documented as an approximation on this port, so the plan
    // says so. A margin chosen against an estimate is not a guarantee, and a
    // provider overflow is reported separately rather than being folded into the
    // accounting as if it had been predicted.
    let capacity = ModelCapacity::new(llm.model_name(), llm.max_context_tokens())
        .with_accounting(TokenAccounting::Estimated);
    // The pools this turn's plan will divide, priced before anything is read:
    // the recent-history pool decides how far back candidates are read, and
    // the recall pool how much recall may bring back.
    let pools = BudgetAllocation::plan(
        &capacity,
        &assembler.fixed_parts(system_policy, current_input, tool_schema_tokens, today),
    )?;
    let recall_bytes = chars_within_tokens(pools.recall, llm.chars_per_token()).max(1);

    // An invalidated ledger cannot cover any original messages, even if an
    // older watermark remains on the snapshot.
    let watermark = if snapshot.is_usable() {
        snapshot.state.processed_through_sequence
    } else {
        0
    };
    let RecentCandidates {
        messages: mut recent,
        unread_source,
        reaches_start,
    } = load_recent(
        memory,
        conversation_id,
        &snapshot,
        watermark,
        pools.recent_history,
        &|text: &str| llm.count_tokens(text),
    )
    .await?;
    // The pending current message is supplied separately, possibly enriched
    // with documents. Compare against the raw query before removing it.
    if recent.last().is_some_and(|m| {
        m.status == "pending"
            && m.role == lattice_core::domain::conversation::memory::SourceRole::User
            && m.content == query
    }) {
        recent.pop();
    }

    // Recall runs against the *original* transcript, and is told what the prompt
    // already carries so it does not pay twice for the same span.
    let recent_text: Vec<String> = recent
        .iter()
        .map(|message| message.content.clone())
        .collect();
    let active_text: Vec<String> = snapshot
        .mandatory_items()
        .iter()
        .map(|item| item.label.clone())
        .collect();
    let scope = HistoryToolScope::new(
        conversation_id,
        HistoryToolBudget {
            max_response_bytes: recall_bytes,
            deadline: None,
        },
    );
    let recalled = history_tools::recall_for_turn(
        recall,
        &scope,
        &RecallRequest {
            user_input: query,
            recent_turns: &recent_text,
            active_memory: &active_text,
            max_bytes: recall_bytes,
        },
    )
    .await?;

    let prepared = memory.prepare_memory(conversation_id, query).await?;
    if prepared
        .as_ref()
        .and_then(|p| p.revisions)
        .is_some_and(|pair| pair != (snapshot.state.memory_revision, snapshot.transcript_revision))
    {
        return Err(AppError::ConcurrentModification {
            resource: "conversation memory".into(),
            details: "Memory changed during prompt preparation; retry".into(),
        });
    }
    let request = ContextRequest {
        conversation_id,
        system_policy,
        current_input,
        inline_evidence,
        tool_schema_tokens,
        snapshot: Some(&snapshot),
        recent: &recent,
        recent_reaches_start: reaches_start,
        processed_through_sequence: watermark,
        recalled: recalled.passages(),
        retrieval: recalled.diagnostics.clone(),
        recall_status: Some(recalled.availability().note()),
        document_evidence,
        today,
        capacity,
    };
    let plan = assembler.assemble_with_memory(&request, prepared.as_ref())?;
    let mut plan = settle_exact_count(llm, plan, tool_schema_tokens, |counter| {
        ContextAssembler::new(counter).assemble_with_memory(&request, prepared.as_ref())
    })
    .await?;

    // The assembler reports what it dropped; it cannot report what it never saw.
    // Source below the candidate window is unprocessed and has nothing standing
    // in for it, so it counts as needing compaction just as much as a message the
    // assembler evicted.
    plan.compaction_required |= unread_source;

    Ok(Some(MemoryTurnContext {
        plan,
        recall_note: recalled.availability().note(),
    }))
}

/// Count the finished plan with the backend's own tokenizer, where it has one.
///
/// Selection runs on the calibrated estimate, because it counts every
/// candidate; whether the request fits is the one decision that is final, so
/// that is made on an exact count. When the estimate read low and the plan no
/// longer fits, it is planned again once with the estimate scaled by what the
/// tokenizer measured, and that plan is counted exactly in turn. An
/// unreachable tokenizer leaves the estimated plan as it was.
async fn settle_exact_count(
    llm: &Arc<dyn LLMPort>,
    plan: ContextPlan,
    tool_schema_tokens: usize,
    replan: impl Fn(TokenCounter) -> Result<ContextPlan>,
) -> Result<ContextPlan> {
    if !llm.counts_tokens_exactly() {
        return Ok(plan);
    }
    let measure = |plan: &ContextPlan| {
        let text = plan.tokenizable_text();
        let estimated = llm.count_tokens(&text);
        async move { (llm.count_tokens_exact(&text).await, estimated) }
    };
    let (exact, estimated) = measure(&plan).await;
    let exact = match exact {
        Ok(exact) => exact,
        Err(error) => {
            tracing::warn!(%error, "Exact token count unavailable; keeping the estimated plan");
            return Ok(plan);
        }
    };
    let mut settled = plan.clone();
    settled.settle_exact_count(exact, tool_schema_tokens);
    if settled.accounting.fits() || exact <= estimated {
        return Ok(settled);
    }

    let scale = exact as f64 / estimated.max(1) as f64;
    tracing::info!(
        estimated,
        exact,
        "The estimate read low and the plan did not fit; planning again at the measured ratio"
    );
    let scaled: TokenCounter = {
        let llm = Arc::clone(llm);
        Arc::new(move |text: &str| (llm.count_tokens(text) as f64 * scale).ceil() as usize)
    };
    let mut replanned = replan(scaled)?;
    match measure(&replanned).await {
        (Ok(exact), _) => replanned.settle_exact_count(exact, tool_schema_tokens),
        (Err(error), _) => {
            tracing::warn!(%error, "Exact token count unavailable for the re-plan");
        }
    }
    Ok(replanned)
}

/// The newest messages, oldest first, as assembler candidates.
struct RecentCandidates {
    messages: Vec<SourceMessage>,
    /// Source above the processed watermark went unread.
    unread_source: bool,
    /// Nothing older than `messages` exists, so the assembler can tell when
    /// its window carries the whole transcript.
    reaches_start: bool,
}

/// The newest messages as assembler candidates, and what was left unread.
///
/// Paged from the end rather than loading the aggregate: prompt construction
/// does not read the entire conversation. Pages are read newest first until
/// the candidates outweigh the recent-history pool (`pool_tokens`) beyond the
/// newest message, which the assembler keeps whatever it costs. How far back
/// that reaches is the window's business, not a message count: a large
/// window carries hundreds of short turns verbatim before anything needs
/// compaction. Messages the ledger already covers are not missing, so only an
/// unread message above the watermark counts as unread source.
async fn load_recent(
    memory: &dyn ConversationMemoryPort,
    conversation_id: &str,
    snapshot: &MemorySnapshot,
    watermark: i64,
    pool_tokens: usize,
    count_tokens: &(dyn Fn(&str) -> usize + Sync),
) -> Result<RecentCandidates> {
    let cost = |message: &SourceMessage| {
        count_tokens(&message.content).saturating_add(RECENT_MESSAGE_FRAMING_TOKENS)
    };
    let mut pages: Vec<Vec<SourceMessage>> = Vec::new();
    let mut through = snapshot.latest_sequence;
    let mut tokens = 0usize;
    let mut target: Option<usize> = None;
    let mut has_more;
    loop {
        let page = memory
            .page_recent_source_messages(conversation_id, 0, through, SourceReadLimits::DEFAULT)
            .await?;
        let Some(oldest) = page.messages.first() else {
            has_more = page.has_more;
            break;
        };
        let oldest_sequence = oldest.sequence;
        // The newest message is kept whatever it costs, so the pool is counted
        // on top of it.
        let target = *target.get_or_insert_with(|| {
            page.messages
                .last()
                .map_or(0, cost)
                .saturating_add(pool_tokens)
        });
        tokens = page
            .messages
            .iter()
            .map(cost)
            .fold(tokens, usize::saturating_add);
        has_more = page.has_more;
        pages.push(page.messages);
        if !has_more || tokens > target {
            break;
        }
        through = oldest_sequence - 1;
    }
    let read: Vec<SourceMessage> = pages.into_iter().rev().flatten().collect();
    let unread_source = match read.first() {
        Some(oldest_read) if has_more && oldest_read.sequence > watermark + 1 => {
            // Sequences can have gaps (deleted messages), so ask whether any
            // message actually sits between the watermark and what was read.
            let probe = memory
                .page_recent_source_messages(
                    conversation_id,
                    watermark,
                    oldest_read.sequence - 1,
                    SourceReadLimits::new(1, 0),
                )
                .await?;
            !probe.messages.is_empty()
        }
        Some(_) => false,
        None => has_more,
    };
    // A failed assistant output is not an answer that was returned, so it is not
    // replayed as one. A pending or failed *user* message is kept: the user still
    // said it, and a generation failure must not erase their instruction.
    Ok(RecentCandidates {
        reaches_start: !has_more,
        messages: read
            .into_iter()
            .filter(SourceMessage::is_eligible_source)
            .collect(),
        unread_source,
    })
}

#[cfg(test)]
mod tests;
