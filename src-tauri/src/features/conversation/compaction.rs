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
use std::time::Duration;

use tracing::info;

use crate::application::ports::LLMPort;
use crate::application::services::conversation_memory::{
    CompactionConfig, CompactionJob, CompactionRequest, CompactionTrigger, TokenCounter,
    COMPACTION_DEADLINE,
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
    let utility = match container.get_or_load_utility_llm().await {
        Ok(utility) => utility,
        Err(AppError::ModelLoadFailed(error)) => {
            tracing::warn!(%error, "Utility model failed to load; using the chat model for compaction");
            None
        }
        Err(error) => return Err(error),
    };

    // The candidate summary has to fit its pool in the **continuation** model's
    // prompt, so both the counter and the budget come from that model rather than
    // the utility one. Recomputed per call because switching model changes both,
    // and a stale budget accepts a summary the next turn cannot carry.
    let continuation_llm = container.get_or_load_llm().await?;
    let utility_llm = utility.unwrap_or_else(|| Arc::clone(&continuation_llm));
    let count_tokens: TokenCounter = {
        let llm = Arc::clone(&continuation_llm);
        Arc::new(move |text: &str| llm.count_tokens(text))
    };

    let deadline = compaction_deadline_for(utility_llm.provider_name());

    Ok(Some(CompactionJob::with_slots(
        container.conversation_memory(),
        utility_llm,
        count_tokens,
        CompactionConfig {
            summary_token_budget: summary_pool_for(&continuation_llm),
            deadline,
            keep_recent_messages: keep_recent_messages
                .unwrap_or(KEEP_RECENT_DEFAULT)
                .max(KEEP_RECENT_MIN) as usize,
            ..Default::default()
        },
        container.compaction_slots(),
    )))
}

/// How long one compaction job may run, by where the utility model runs.
///
/// The default suits a remote llama-server at tens of tokens per second. The
/// bundled sidecar on a laptop measured two to two and a half minutes for one
/// extraction over a twelve-turn conversation, so any cycle that needed a
/// repair round overran five minutes and committed nothing; one evaluated
/// conversation lost three of its ten cycles that way. Three attempts at that
/// speed plus review and summary fit in fifteen minutes. The job is a
/// background task, so the cost of the longer bound is GPU time, not a turn.
pub fn compaction_deadline_for(utility_provider: &str) -> Duration {
    if utility_provider == "local-sidecar" {
        LOCAL_SIDECAR_COMPACTION_DEADLINE
    } else {
        COMPACTION_DEADLINE
    }
}

const LOCAL_SIDECAR_COMPACTION_DEADLINE: Duration = Duration::from_secs(15 * 60);

/// The summary pool for a model, from the shared budget rules.
///
/// Uses the same allocation the context assembler applies when it charges the
/// prompt, so a summary this job accepts is one the next turn can actually
/// carry. That allocation is taken after the turn's fixed costs, so they are
/// charged here too: sized against an empty prompt, the pool was 10% of the
/// whole input budget while the turn's was 10% of what the system prompt,
/// tool schemas and question left, and on a small window an accepted summary
/// no longer fit the next turn. A model too small to plan for falls back to a
/// small floor rather than failing compaction outright: the job rejects an
/// oversized summary anyway, so the floor costs a rejected candidate, not a
/// wrong commit.
pub fn summary_pool_for(llm: &Arc<dyn LLMPort>) -> usize {
    let fixed = llm
        .count_tokens(crate::application::services::context_assembler::render::MEMORY_USE_POLICY)
        .saturating_add(TYPICAL_TURN_FIXED_TOKENS);
    summary_pool(llm.model_name(), llm.max_context_tokens(), fixed)
}

/// What a typical turn spends before any history: its system prompt, the tool
/// schemas it is offered, and the question. A representative figure, not a
/// bound — the turn's own plan still drops a summary that does not fit.
const TYPICAL_TURN_FIXED_TOKENS: usize = 1_200;

fn summary_pool(model: &str, context_tokens: usize, fixed: usize) -> usize {
    use crate::application::services::context_assembler::{BudgetAllocation, ModelCapacity};
    let capacity = ModelCapacity::new(model, context_tokens);
    // A window too small for the typical turn is planned with what it can
    // hold; the smaller pool is the safe side of the error.
    BudgetAllocation::plan(&capacity, fixed)
        .or_else(|_| BudgetAllocation::plan(&capacity, fixed / 2))
        .or_else(|_| BudgetAllocation::plan(&capacity, 0))
        .map(|allocation| allocation.summary)
        .unwrap_or(256)
}

/// Compact because assembly would otherwise drop unprocessed source (§7.1).
///
/// Runs at most one pass. A successful job (including a partial commit or no-op)
/// is not permission to generate: the caller must reassemble and verify that no
/// unprocessed source is missing. Errors leave the transcript intact and prevent
/// generation from using the incomplete pre-compaction plan. `cancel` is the
/// turn's stop button.
pub async fn compact_for_turn(
    container: &Container,
    conversation_id: &str,
    cancel: tokio_util::sync::CancellationToken,
) -> Result<()> {
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
                cancellation: Some(cancel),
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

/// Consolidation follows a committed answer and cannot change its success.
///
/// Its model calls go out at maintenance priority, so the backend's scheduler
/// keeps them behind the user's next turn and out of the slot it holds for
/// interactive work. The conversation's compaction slot coalesces queued
/// requests through the durable watermark: a consolidation waiting behind
/// another of the same conversation finds nothing left to do.
pub fn consolidate_after_turn(container: Container, conversation_id: String) {
    let cancel = crate::shared::runtime::background::cancellation_token();
    crate::shared::runtime::background::spawn(async move {
        let result: Result<()> = async {
            if !container
                .get_settings_use_case()
                .execute()
                .await?
                .llm
                .bounded_conversation_memory
            {
                return Ok(());
            }
            let built = tokio::select! {
                biased;
                _ = cancel.cancelled() => return Ok(()),
                result = build_job(&container, None) => result,
            };
            let job = built?.ok_or_else(|| {
                AppError::ServiceNotAvailable(
                    "No utility model available for memory consolidation".into(),
                )
            })?;
            job.run(
                &conversation_id,
                CompactionRequest {
                    trigger: CompactionTrigger::Maintenance,
                    cancellation: Some(cancel.clone()),
                    ..Default::default()
                },
            )
            .await?;
            Ok(())
        }
        .await;
        if let Err(error) = result {
            tracing::warn!(conversation_id, %error, "Memory consolidation deferred; answer remains saved");
            let _ = container
                .conversation_memory()
                .record_memory_error(&conversation_id, "maintenance_deferred")
                .await;
        }
    });
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]
mod tests {
    use super::*;
    use crate::application::services::context_assembler::{BudgetAllocation, ModelCapacity};

    /// On a 4k window the turn's fixed costs are most of the prompt; a pool
    /// sized against an empty prompt accepted summaries the turn then dropped.
    #[test]
    fn a_summary_pool_on_a_small_window_fits_the_turn_that_carries_it() {
        let capacity = ModelCapacity::new("small", 4_096);
        let fixed = 1_500;
        let turn_pool = BudgetAllocation::plan(&capacity, fixed).unwrap().summary;
        let empty_prompt_pool = BudgetAllocation::plan(&capacity, 0).unwrap().summary;

        let pool = summary_pool("small", 4_096, fixed);

        assert_eq!(pool, turn_pool);
        assert!(pool < empty_prompt_pool, "{pool} vs {empty_prompt_pool}");
    }

    /// The one-at-a-time maintenance semaphore kept consolidation from crowding
    /// out the next turn. The scheduler does that now, so consolidation must
    /// queue as upkeep — while compaction the user is waiting on must not.
    #[test]
    fn consolidation_queues_as_upkeep_and_a_waited_on_compaction_does_not() {
        use crate::application::ports::llm_port::InferencePriority;
        assert_eq!(
            CompactionTrigger::Maintenance.priority(),
            InferencePriority::Maintenance
        );
        assert_eq!(
            CompactionTrigger::Automatic.priority(),
            InferencePriority::Interactive
        );
        assert_eq!(
            CompactionTrigger::Manual.priority(),
            InferencePriority::Interactive
        );
    }

    /// A laptop sidecar takes minutes per extraction; the remote default
    /// deadline made every repaired cycle commit nothing.
    #[test]
    fn the_bundled_sidecar_gets_a_longer_compaction_deadline_than_a_remote_server() {
        let local = compaction_deadline_for("local-sidecar");
        assert!(local >= Duration::from_secs(12 * 60), "{local:?}");
        assert_eq!(compaction_deadline_for("llamacpp"), COMPACTION_DEADLINE);
        assert_eq!(compaction_deadline_for("openai"), COMPACTION_DEADLINE);
    }

    #[test]
    fn a_window_too_small_for_the_typical_turn_still_gets_a_pool() {
        let pool = summary_pool("tiny", 2_048, 5_000);
        assert!(pool > 0);
        assert!(pool <= summary_pool("tiny", 2_048, 0));
    }
}
