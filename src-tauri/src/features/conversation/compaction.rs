//! The one place a compaction job is built, plus the automatic trigger.
//!
//! Design: `docs/design/2026-09-19-conversation-memory.md` §7.1, §7.3, §12.
//!
//! Both entry points construct the job here: the user's `/compact` and the
//! automatic trigger that fires when context assembly would otherwise drop
//! unprocessed source. Two construction sites would mean two summary budgets and
//! — worse — two `CompactionSlots` maps, and those slots are the only thing
//! stopping one conversation from compacting twice at once.

use std::sync::Arc;

use tracing::info;

use crate::application::ports::LLMPort;
use crate::application::services::conversation_memory::{
    CompactionConfig, CompactionJob, CompactionRequest, CompactionTrigger, TokenCounter,
};
use crate::interfaces::di::Container;
use crate::shared::error::{AppError, Result};

/// Messages kept raw when a request names no number.
pub const KEEP_RECENT_DEFAULT: i64 = 4;

/// Floor for the keep-recent count: always leave at least this many raw.
pub const KEEP_RECENT_MIN: i64 = 2;

/// Build the compaction job for this container, or `None` when no utility model
/// is available.
///
/// The caller decides whether missing compaction is blocking: a turn may
/// continue without it only if all unprocessed source still fits in its prompt.
pub async fn build_job(
    container: &Container,
    keep_recent_messages: Option<i64>,
) -> Result<Option<CompactionJob>> {
    let Some(utility_llm) = container.get_or_load_utility_llm().await? else {
        return Ok(None);
    };

    // The candidate summary has to fit its pool in the **continuation** model's
    // prompt, so both the counter and the budget come from that model rather than
    // the utility one. Recomputed per call because switching model changes both,
    // and a stale budget accepts a summary the next turn cannot carry.
    let continuation_llm = container.get_or_load_llm().await?;
    let count_tokens: TokenCounter = {
        let llm = Arc::clone(&continuation_llm);
        Arc::new(move |text: &str| llm.count_tokens(text))
    };

    Ok(Some(CompactionJob::with_slots(
        container.conversation_memory(),
        utility_llm,
        count_tokens,
        CompactionConfig {
            summary_token_budget: summary_pool_for(&continuation_llm),
            keep_recent_messages: keep_recent_messages
                .unwrap_or(KEEP_RECENT_DEFAULT)
                .max(KEEP_RECENT_MIN) as usize,
            ..Default::default()
        },
        container.compaction_slots(),
    )))
}

/// The summary pool for a model, from the shared budget rules.
///
/// Uses the same allocation the context assembler applies when it charges the
/// prompt, so a summary this job accepts is one the next turn can actually
/// carry. A model too small to plan for falls back to a small floor rather than
/// failing compaction outright: the job rejects an oversized summary anyway, so
/// the floor costs a rejected candidate, not a wrong commit.
pub fn summary_pool_for(llm: &Arc<dyn LLMPort>) -> usize {
    use crate::application::services::context_assembler::{BudgetAllocation, ModelCapacity};
    let capacity = ModelCapacity::new(llm.model_name(), llm.max_context_tokens());
    BudgetAllocation::plan(&capacity, 0)
        .map(|allocation| allocation.summary)
        .unwrap_or(256)
}

/// Compact because assembly would otherwise drop unprocessed source (§7.1).
///
/// Runs at most one pass. A successful job (including a partial commit or no-op)
/// is not permission to generate: the caller must reassemble and verify that no
/// unprocessed source is missing. Errors leave the transcript intact and prevent
/// generation from using the incomplete pre-compaction plan.
pub async fn compact_for_turn(container: &Container, conversation_id: &str) -> Result<()> {
    let job = build_job(container, None).await?.ok_or_else(|| {
        AppError::ServiceNotAvailable(
            "No utility model is available to compact this conversation.".into(),
        )
    })?;

    let outcome = job
        .run(
            conversation_id,
            CompactionRequest {
                trigger: CompactionTrigger::Automatic,
                ..Default::default()
            },
        )
        .await?;

    info!(
        conversation_id,
        status = ?outcome.status,
        memory_revision = outcome.memory_revision,
        processed_through_sequence = outcome.processed_through_sequence,
        active_mandatory = outcome.active_mandatory_count,
        active_optional = outcome.active_optional_count,
        summary_tokens = outcome.summary_tokens,
        conflicts_recorded = outcome.conflicts_recorded,
        segments_rejected = outcome.segments_rejected.len(),
        rejection_code = ?outcome.rejection_code,
        review_degraded = outcome.review_degraded,
        resume_after_sequence = ?outcome.resume_after_sequence,
        "Automatic compaction ran before this turn"
    );

    Ok(())
}
