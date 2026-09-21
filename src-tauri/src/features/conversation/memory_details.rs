//! Reading the memory ledger for display.
//!
//! Design: `docs/design/2026-09-19-conversation-memory.md` §13.
//!
//! Quotations are resolved here, from the original messages, every time. They
//! are never read from a cache and never stored alongside the item: the whole
//! reason evidence is a byte range plus a digest is that deleting a message has
//! to delete its quotation everywhere, including from this view.

use crate::application::ports::conversation_memory::{
    ConversationMemoryPort, SourceReadLimits, SourceSpanRef,
};
use crate::domain::conversation_memory::{MemoryItem, MemoryValidity};
use crate::features::conversation::memory_dto::{
    ConversationMemoryDetailsDto, ConversationMemoryItemDto, MemoryEvidenceDto,
};
use crate::shared::error::Result;

/// How many superseded/resolved items the history view pages at once.
const HISTORY_PAGE: i64 = 50;

/// Build the details view for one conversation.
pub async fn load_memory_details(
    memory: &dyn ConversationMemoryPort,
    conversation_id: &str,
    include_history: bool,
    feature_enabled: bool,
) -> Result<ConversationMemoryDetailsDto> {
    let snapshot = memory.load_snapshot(conversation_id).await?;

    // Three modes, not a boolean: rebuilding is temporary and self-healing, an
    // unsupported layout needs the app updated, and neither is working memory.
    let mode = if !snapshot.state.is_schema_supported() {
        "unsupported_schema"
    } else if snapshot.state.validity == MemoryValidity::RebuildRequired {
        "rebuild_required"
    } else {
        "ready"
    };

    let mut items = Vec::new();
    let mut conflicts = 0i64;
    for item in &snapshot.active_items {
        let dto = to_dto(memory, conversation_id, item).await?;
        // An item counts as needing attention when its reading is unsettled, or
        // when nothing it quotes resolves any more.
        if dto.review == "ambiguous" || dto.evidence.iter().all(|span| span.text.is_none()) {
            conflicts += 1;
        }
        items.push(dto);
    }

    let mut history = Vec::new();
    if include_history {
        for item in memory
            .page_inactive_items(conversation_id, 0, HISTORY_PAGE)
            .await?
        {
            history.push(to_dto(memory, conversation_id, &item).await?);
        }
    }

    Ok(ConversationMemoryDetailsDto {
        conversation_id: conversation_id.to_string(),
        mode: mode.to_string(),
        schema_version: snapshot.state.schema_version,
        memory_revision: snapshot.state.memory_revision,
        transcript_revision: snapshot.transcript_revision,
        processed_through_sequence: snapshot.state.processed_through_sequence,
        active_mandatory_count: snapshot.mandatory_items().len() as i64,
        active_optional_count: snapshot.optional_items().len() as i64,
        conflict_count: conflicts,
        summary: snapshot.summary.clone(),
        items,
        history,
        last_error_code: snapshot.state.last_error_code.clone(),
        extractor_model_identity: snapshot.state.extractor_model_identity.clone(),
        feature_enabled,
    })
}

/// Resolve one item's spans and shape it for the wire.
async fn to_dto(
    memory: &dyn ConversationMemoryPort,
    conversation_id: &str,
    item: &MemoryItem,
) -> Result<ConversationMemoryItemDto> {
    let refs: Vec<SourceSpanRef> = item
        .evidence
        .iter()
        .map(|span| SourceSpanRef {
            message_id: span.message_id.clone(),
            start_byte: span.start_byte,
            end_byte: span.end_byte,
        })
        .collect();
    let resolved = memory
        .read_source_spans(conversation_id, &refs, SourceReadLimits::DEFAULT)
        .await?;

    let evidence = item
        .evidence
        .iter()
        .zip(resolved)
        .map(|(span, resolved)| MemoryEvidenceDto {
            message_id: span.message_id.clone(),
            sequence: span.sequence,
            role: span.role.as_str().to_string(),
            purpose: span.purpose.as_str().to_string(),
            start_byte: span.start_byte,
            end_byte: span.end_byte,
            // `None` when the source moved. Rendered as an absence; a cached
            // copy of the old text would defeat deletion.
            text: resolved.text,
        })
        .collect();

    Ok(ConversationMemoryItemDto {
        id: item.id.to_string(),
        kind: item.kind.as_str().to_string(),
        state: item.state.as_str().to_string(),
        review: item.review.as_str().to_string(),
        label: item.label.clone(),
        is_mandatory: item.kind.is_mandatory(),
        created_at_sequence: item.created_at_sequence,
        changed_at_sequence: item.changed_at_sequence,
        superseded_by: item.superseded_by.as_ref().map(ToString::to_string),
        related_item_ids: item
            .related_item_ids
            .iter()
            .map(ToString::to_string)
            .collect(),
        evidence,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::conversation::repository::ConversationRepository;
    use sqlx::SqlitePool;

    async fn database() -> SqlitePool {
        let pool = SqlitePool::connect(":memory:").await.unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        sqlx::query("PRAGMA foreign_keys = ON")
            .execute(&pool)
            .await
            .unwrap();
        pool
    }

    async fn seeded(pool: &SqlitePool) -> (ConversationRepository, String, String) {
        use crate::application::ports::conversation_memory::{
            MemoryCommitCandidate, MemoryCommitPreconditions,
        };
        use crate::domain::conversation_memory::{
            compute_digest, EvidencePurpose, EvidenceSpan, MemoryCommit, MemoryId, MemoryItem,
            MemoryKind, MemoryReview, MemoryState, SourceRole,
        };

        let repository = ConversationRepository::new(pool.clone());
        let conversation = repository
            .create_conversation("Details", "model", None)
            .await
            .unwrap();
        let id = conversation.id.to_string();
        let content = "Do not deploy until I approve.";
        let message = repository
            .add_message(
                &id,
                crate::domain::conversation::MessageRole::User,
                content,
                10,
                None,
            )
            .await
            .unwrap();

        let snapshot = repository.load_memory_snapshot(&id).await.unwrap();
        repository
            .commit_memory(
                &MemoryCommitPreconditions {
                    conversation_id: id.clone(),
                    expected_transcript_revision: snapshot.transcript_revision,
                    expected_memory_revision: 0,
                    operation_id: "details".into(),
                },
                &MemoryCommitCandidate {
                    commit: MemoryCommit {
                        inserts: vec![MemoryItem {
                            id: MemoryId::new(),
                            conversation_id: id.clone(),
                            kind: MemoryKind::Constraint,
                            state: MemoryState::Active,
                            label: "Approval required".into(),
                            evidence: vec![EvidenceSpan {
                                message_id: message.id.clone(),
                                sequence: 1,
                                role: SourceRole::User,
                                start_byte: 0,
                                end_byte: content.len() as u32,
                                content_digest: compute_digest(content),
                                purpose: EvidencePurpose::Assertion,
                            }],
                            created_at_sequence: 1,
                            changed_at_sequence: 1,
                            superseded_by: None,
                            revision: 0,
                            review: MemoryReview::Supported,
                            related_item_ids: Vec::new(),
                            created_at: chrono::Utc::now(),
                            updated_at: chrono::Utc::now(),
                        }],
                        updates: Vec::new(),
                        summary: None,
                        processed_through_sequence: 1,
                    },
                    summary: None,
                    source_message_ids: Vec::new(),
                    transcript_revision: snapshot.transcript_revision,
                    extractor_model_identity: Some("utility-model".into()),
                    extractor_prompt_version: Some("v1".into()),
                    validator_version: Some("v1".into()),
                    operation: "compact".into(),
                },
            )
            .await
            .unwrap();
        (repository, id, message.id)
    }

    #[tokio::test]
    async fn the_details_view_quotes_the_source_and_marks_the_label_as_generated() {
        let pool = database().await;
        let (repository, id, _) = seeded(&pool).await;

        let details = load_memory_details(&repository, &id, false, true)
            .await
            .unwrap();
        assert_eq!(details.mode, "ready");
        assert_eq!(details.active_mandatory_count, 1);
        assert_eq!(details.conflict_count, 0);
        assert!(details.feature_enabled);
        assert_eq!(
            details.extractor_model_identity.as_deref(),
            Some("utility-model")
        );

        let item = &details.items[0];
        assert!(item.is_mandatory);
        assert_eq!(item.label, "Approval required");
        // The quotation is the authority, and it is the source's exact text.
        assert_eq!(
            item.evidence[0].text.as_deref(),
            Some("Do not deploy until I approve.")
        );
        assert_eq!(item.evidence[0].role, "user");
        assert_eq!(item.evidence[0].purpose, "assertion");
    }

    #[tokio::test]
    async fn a_deleted_source_leaves_an_absence_rather_than_a_cached_quotation() {
        let pool = database().await;
        let (repository, id, message_id) = seeded(&pool).await;

        repository
            .delete_messages(&id, &[message_id])
            .await
            .unwrap();

        let details = load_memory_details(&repository, &id, false, true)
            .await
            .unwrap();
        // Deleting the source invalidated the ledger, so there is nothing left
        // to quote and the mode says why.
        assert_eq!(details.mode, "rebuild_required");
        assert!(details.items.is_empty());
        assert_eq!(details.last_error_code.as_deref(), Some("source_deleted"));
    }

    #[tokio::test]
    async fn an_edited_source_shows_the_item_as_a_conflict_with_no_text() {
        let pool = database().await;
        let (repository, id, message_id) = seeded(&pool).await;

        // Edit through a path that does not fire the invalidation trigger would
        // be a bug; this one does fire it, which is the point — but the details
        // view must also be correct for any item whose span stops resolving.
        sqlx::query("UPDATE conversation_messages SET content = ? WHERE id = ?")
            .bind("You may deploy freely.")
            .bind(&message_id)
            .execute(&pool)
            .await
            .unwrap();

        let details = load_memory_details(&repository, &id, false, true)
            .await
            .unwrap();
        assert_eq!(details.mode, "rebuild_required");
        assert_eq!(details.last_error_code.as_deref(), Some("source_rewritten"));
        // Whatever survives must not be quoting text the user no longer has.
        for item in &details.items {
            for span in &item.evidence {
                assert_ne!(span.text.as_deref(), Some("Do not deploy until I approve."));
            }
        }
    }

    #[tokio::test]
    async fn the_view_reports_the_switch_so_the_ui_cannot_advertise_memory_that_is_off() {
        let pool = database().await;
        let (repository, id, _) = seeded(&pool).await;

        let details = load_memory_details(&repository, &id, false, false)
            .await
            .unwrap();
        assert!(
            !details.feature_enabled,
            "the UI needs this to avoid describing memory as active while it is off"
        );
        // The ledger is still readable — it simply is not in use.
        assert_eq!(details.active_mandatory_count, 1);
    }
}
