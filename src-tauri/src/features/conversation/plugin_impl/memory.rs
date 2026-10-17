//! Conversation memory, compaction and knowledge management.

use crate::features::conversation::dto::{
    CompactConversationRequestDto, CompactConversationResponseDto,
};
use crate::interfaces::di::Container;
use lattice_core::application::ports::llm_port::ModelRequirements;
use lattice_core::shared::ipc::ApiError;

/// Read the bounded-memory ledger for a conversation, for the details view.
///
/// Read-only. There is deliberately no counterpart that writes a memory item:
/// a free-form editor is a way to create a requirement with no source behind it,
/// and every authoritative item here is traceable to something the user wrote.
/// A correction is an ordinary message and goes through the same validation.
pub async fn get_conversation_memory_impl(
    request: crate::features::conversation::memory_dto::GetConversationMemoryRequestDto,
    container: &Container,
) -> Result<crate::features::conversation::memory_dto::ConversationMemoryDetailsDto, ApiError> {
    let conversation_id = request.conversation_id.trim();
    if conversation_id.is_empty() {
        return Err(ApiError::from(
            lattice_core::shared::error::AppError::InvalidInput(
                "Conversation ID is required to read memory".to_string(),
            ),
        ));
    }
    let repository = crate::features::conversation::repository::ConversationRepository::new(
        container.db_pool().clone(),
    );
    // Whether the staged-rollout switch is on is part of the answer: the UI must
    // not describe memory as active while it is off.
    let enabled = container
        .get_settings_use_case()
        .execute()
        .await
        .map(|settings| settings.llm.bounded_conversation_memory)
        .unwrap_or(false);
    crate::features::conversation::memory_details::load_memory_details(
        &repository,
        conversation_id,
        request.include_history.unwrap_or(false),
        enabled,
    )
    .await
    .map_err(ApiError::from)
}

/// Compact a conversation's older messages.
///
/// Routed through [`CompactionJob`], the only compaction implementation: it
/// extracts source-backed memory items and a bounded working summary and commits
/// both in one transaction. The rollout switch gates whether a *turn* consults
/// the ledger, not whether `/compact` builds one, so an explicit request always
/// does the real work.
///
/// This deliberately does not go through the chat pipeline: that path validates
/// its input as a user-typed query (10k characters), generates a title, and runs
/// the whole router and retrieval stack — none of which applies to an internal
/// summarization, and the length cap alone would reject every conversation long
/// enough to be worth compacting.
///
/// [`CompactionJob`]: crate::features::conversation::memory::CompactionJob
pub async fn compact_conversation_impl(
    request: CompactConversationRequestDto,
    container: &Container,
) -> Result<CompactConversationResponseDto, ApiError> {
    let conversation_id = request.conversation_id.trim();
    if conversation_id.is_empty() {
        return Err(ApiError::from(
            lattice_core::shared::error::AppError::InvalidInput(
                "Conversation ID is required to compact".to_string(),
            ),
        ));
    }

    // The summary is sized for the chat's own model, the one its next turn
    // runs on; the pin holds that model until the job is done. §12: without a
    // utility model, say that compaction cannot proceed rather than committing
    // something unvalidated. A `None` is only an error on this path, because
    // here the user asked.
    let continuation = container
        .chat_llm_for(conversation_id, None, ModelRequirements::default())
        .await
        .map_err(ApiError::from)?;
    let job = crate::features::conversation::compaction::build_job(
        container,
        std::sync::Arc::clone(&continuation.llm),
        request.keep_recent_messages,
    )
    .await
    .map_err(ApiError::from)?
    .ok_or_else(|| {
        ApiError::from(lattice_core::shared::error::AppError::ServiceNotAvailable(
            "No model is available to compact this conversation".to_string(),
        ))
    })?;

    let outcome = job
        .run(
            conversation_id,
            crate::features::conversation::memory::CompactionRequest {
                trigger: crate::features::conversation::memory::CompactionTrigger::Manual,
                keep_recent_messages: request.keep_recent_messages.map(|value| value as usize),
                ..Default::default()
            },
        )
        .await
        .map_err(ApiError::from)?;

    compaction_response(conversation_id, &outcome)
}

/// Map a run onto the compaction DTO.
fn compaction_response(
    conversation_id: &str,
    outcome: &crate::features::conversation::memory::CompactionOutcome,
) -> Result<CompactConversationResponseDto, ApiError> {
    use crate::features::conversation::memory::CompactionStatus;
    use crate::features::conversation::memory_dto::CompactionMemoryDto;

    if outcome.status == CompactionStatus::NothingToCompact {
        return Err(ApiError::from(
            lattice_core::shared::error::AppError::InvalidInput(
                "Nothing to compact: this conversation has no older messages that are not \
                 already covered."
                    .to_string(),
            ),
        ));
    }

    let summary_text = outcome.summary.clone().unwrap_or_default();
    let summary_tokens = outcome.summary_tokens.max(1) as i64;
    let record = lattice_core::domain::conversation::CompactionRecord {
        id: uuid::Uuid::new_v4().to_string(),
        conversation_id: lattice_core::shared::types::ConversationId::from_string(
            conversation_id.to_string(),
        )
        .map_err(|error| {
            ApiError::from(lattice_core::shared::error::AppError::InvalidInput(
                error.to_string(),
            ))
        })?,
        summary_text,
        up_to_message_id: outcome.boundary_message_id.clone().unwrap_or_default(),
        original_message_count: outcome.source_messages_read.max(1) as i64,
        // Reported as what was read, not as a savings claim: the summary-only
        // ratio is not total prompt savings and is not presented as one.
        original_tokens: (outcome.source_messages_read.max(1) as i64) * 4,
        summary_tokens,
        compression_ratio: 1.0_f64
            .min(summary_tokens as f64 / ((outcome.source_messages_read.max(1) as f64) * 4.0)),
        created_at: chrono::Utc::now(),
    };

    Ok(CompactConversationResponseDto {
        compaction: crate::features::conversation::dto::CompactionRecordDto::from_record(&record),
        memory: CompactionMemoryDto {
            memory_revision: outcome.memory_revision,
            active_mandatory_count: outcome.active_mandatory_count as i64,
            active_optional_count: outcome.active_optional_count as i64,
            mode: if outcome.review_degraded {
                // The reviewer could not be reached, so its subjects were
                // recorded ambiguous rather than supported. Worth surfacing.
                "degraded".to_string()
            } else {
                "ready".to_string()
            },
            memory_tokens: 0,
            processed_message_count: outcome.source_messages_read as i64,
            more_source_remains: outcome.status == CompactionStatus::Partial,
        },
    })
}

/// Memory changes preserve exact user-authored evidence in the transcript.
pub async fn manage_knowledge_impl(
    request: crate::features::conversation::knowledge_dto::KnowledgeRequestDto,
    container: &Container,
) -> Result<crate::features::conversation::knowledge_dto::KnowledgeResponseDto, ApiError> {
    let repository = crate::features::conversation::repository::ConversationRepository::new(
        container.db_pool().clone(),
    );
    let response = repository
        .knowledge(&request)
        .await
        .map_err(ApiError::from)?;
    if matches!(request.action.as_str(), "remember" | "correct") {
        if let Some(item) = response.items.iter().find(|item| {
            item.conversation_id == request.conversation_id
                && item.state == "active"
                && Some(item.label.as_str()) == request.text.as_deref().map(str::trim)
        }) {
            if let Some(evidence) = item.evidence.first() {
                if let Some(text) = &evidence.text {
                    crate::features::conversation::chat::index_memory_note(
                        std::sync::Arc::new(container.clone()),
                        request.conversation_id.clone(),
                        evidence.message_id.clone(),
                        text.clone(),
                    );
                }
            }
        }
    }
    Ok(response)
}
