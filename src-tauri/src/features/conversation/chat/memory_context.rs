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

use crate::application::ports::conversation_memory::{
    ConversationMemoryPort, ConversationMemoryReadPort, SourceReadLimits,
};
use crate::application::ports::LLMPort;
use crate::application::services::context_assembler::{
    ContextAssembler, ContextPlan, ContextRequest, ModelCapacity, RankedEvidence, TokenAccounting,
};
use crate::domain::conversation_memory::{MemorySnapshot, SourceMessage};
use crate::shared::error::{AppError, Result};

use super::history_tools::{self, HistoryToolBudget, HistoryToolScope, RecallRequest};

/// How many of the newest messages are offered to the assembler as candidates.
///
/// A ceiling on *work*, not on what the prompt carries: the assembler decides
/// how many actually fit. Reading the whole archive to then discard most of it
/// is the unbounded-load problem this design removes.
const RECENT_CANDIDATE_MESSAGES: usize = 64;

/// Byte ceiling on recalled passages for one turn.
const RECALL_BYTE_BUDGET: usize = 8 * 1024;

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
    let initial = build().await?;
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
#[allow(clippy::too_many_arguments)]
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
    if !enabled {
        return Ok(None);
    }

    let snapshot = memory.load_snapshot(conversation_id).await?;

    // An invalidated ledger cannot cover any original messages, even if an
    // older watermark remains on the snapshot.
    let watermark = if snapshot.is_usable() {
        snapshot.state.processed_through_sequence
    } else {
        0
    };
    let unreachable_source = watermark
        < snapshot
            .latest_sequence
            .saturating_sub(RECENT_CANDIDATE_MESSAGES as i64);
    let (recent, unread_source) = load_recent(memory, conversation_id, &snapshot).await?;

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
            max_response_bytes: RECALL_BYTE_BUDGET,
            deadline: None,
        },
    );
    let recalled = history_tools::recall_for_turn(
        recall,
        &scope,
        &RecallRequest {
            user_input: current_input,
            recent_turns: &recent_text,
            active_memory: &active_text,
            max_bytes: RECALL_BYTE_BUDGET,
        },
    )
    .await?;

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

    let mut plan = assembler.assemble(&ContextRequest {
        conversation_id,
        system_policy,
        current_input,
        tool_schema_tokens,
        snapshot: Some(&snapshot),
        recent: &recent,
        processed_through_sequence: watermark,
        recalled: recalled.passages(),
        retrieval: recalled.diagnostics.clone(),
        document_evidence,
        capacity,
    })?;

    // The assembler reports what it dropped; it cannot report what it never saw.
    // Source below the candidate window is unprocessed and has nothing standing
    // in for it, so it counts as needing compaction just as much as a message the
    // assembler evicted.
    plan.compaction_required |= unreachable_source || unread_source;

    Ok(Some(MemoryTurnContext {
        plan,
        recall_note: recalled.availability().note(),
    }))
}

/// The newest messages, oldest first, as assembler candidates.
///
/// Paged from the end rather than loading the aggregate: the whole point is that
/// prompt construction stops reading the entire conversation.
async fn load_recent(
    memory: &dyn ConversationMemoryPort,
    conversation_id: &str,
    snapshot: &MemorySnapshot,
) -> Result<(Vec<SourceMessage>, bool)> {
    let from = snapshot
        .latest_sequence
        .saturating_sub(RECENT_CANDIDATE_MESSAGES as i64)
        .max(0);
    let page = memory
        .page_source_messages(
            conversation_id,
            from,
            snapshot.latest_sequence,
            SourceReadLimits::new(RECENT_CANDIDATE_MESSAGES, 512 * 1024),
        )
        .await?;
    // A failed assistant output is not an answer that was returned, so it is not
    // replayed as one. A pending or failed *user* message is kept: the user still
    // said it, and a generation failure must not erase their instruction.
    // The byte limit can cut this page short even with fewer than 64 messages.
    // Do not mistake a successfully loaded prefix for the entire candidate tail.
    Ok((
        page.messages
            .into_iter()
            .filter(SourceMessage::is_eligible_source)
            .collect(),
        page.has_more,
    ))
}

#[cfg(test)]
mod tests;
