use std::collections::HashSet;
use std::sync::Arc;

use tracing::{debug, warn};

use crate::features::conversation::chat::ports::ChatRuntime;
use crate::features::conversation::ConversationServiceTrait;
use crate::features::function_calling::domain::FunctionResult;
use crate::features::function_calling::dto::{GetDocumentOutput, SemanticSearchOutput};
use crate::features::search::dto::SearchResultDto;
use crate::shared::error::Result;

pub(super) async fn load_space_document_scope(
    container: &dyn ChatRuntime,
    conversation_id: &str,
) -> Option<super::SpaceDocumentScope> {
    let repository = crate::features::conversation::repository::ConversationRepository::new(
        container.db_pool().clone(),
    );
    let (space_id, document_ids) = match repository.retrieval_document_scope(conversation_id).await
    {
        Ok(Some(scope)) => scope,
        Ok(None) => return None,
        Err(error) => {
            warn!(error = %error, conversation_id, "Failed to load conversation document scope");
            return None;
        }
    };

    debug!(
        conversation_id = conversation_id,
        space_id = space_id.as_str(),
        scoped_document_count = document_ids.len(),
        "Loaded hard space document scope"
    );

    Some(super::SpaceDocumentScope {
        space_id,
        document_ids,
    })
}

pub(super) async fn build_hyde_context_window_for_conversation(
    conv_service: &Arc<dyn ConversationServiceTrait>,
    conversation_id: &str,
    question: &str,
) -> Option<String> {
    let aggregate = match conv_service.get_conversation(conversation_id).await {
        Ok(Some(aggregate)) => aggregate,
        Ok(None) => return None,
        Err(e) => {
            warn!(
                conversation_id = conversation_id,
                error = %e,
                "Failed loading conversation for HyDE context"
            );
            return None;
        }
    };

    query_planning_context(aggregate.messages(), question)
}

fn query_planning_context(
    messages: &[crate::domain::conversation::ConversationMessage],
    question: &str,
) -> Option<String> {
    // This is a query-planning view, not the answering model's memory. Keep
    // recent turns and relevant older turns instead of sending an unbounded
    // transcript to a small utility model (which caused rewrite timeouts).
    let cleaned = super::search_text_without_url_tracking(question);
    let terms = super::select_informative_terms(super::tokenize_keyword_terms(&cleaned), 16);
    let eligible: Vec<_> = messages
        .iter()
        .enumerate()
        .filter(|(_, message)| message.is_completed() && !message.content.trim().is_empty())
        .collect();
    let mut selected: Vec<_> = eligible.iter().rev().take(4).copied().collect();
    let mut older: Vec<_> = eligible
        .iter()
        .rev()
        .skip(4)
        .filter_map(|(index, message)| {
            let text = message.content.to_lowercase();
            let score = terms
                .iter()
                .filter(|term| text.contains(term.as_str()))
                .count();
            (score > 0).then_some((score, *index, *message))
        })
        .collect();
    older.sort_by_key(|entry| std::cmp::Reverse(entry.0));
    selected.extend(
        older
            .into_iter()
            .take(4)
            .map(|(_, index, message)| (index, message)),
    );
    selected.sort_by_key(|(index, _)| *index);
    let entries: Vec<_> = selected
        .into_iter()
        .map(|(_, message)| {
            format!(
                "{}: {}",
                message.role,
                crate::shared::text::build_excerpt(&message.content, &terms, 1000)
            )
        })
        .collect();
    (!entries.is_empty()).then(|| entries.join("\n"))
}

pub(super) async fn persist_document_references(
    conv_service: &Arc<dyn ConversationServiceTrait>,
    conversation_id: &str,
    results: &[SearchResultDto],
) -> Result<()> {
    let mut seen: HashSet<(String, String)> = HashSet::new();

    for result in results {
        let doc_id = match result.document_id.as_deref() {
            Some(id) if !id.is_empty() => id,
            _ => continue,
        };

        let key = (doc_id.to_string(), result.id.clone());
        if !seen.insert(key) {
            continue;
        }

        if let Err(e) = conv_service
            .add_document_reference(
                conversation_id,
                doc_id.to_string(),
                Some(result.id.clone()),
                Some(result.score),
            )
            .await
        {
            warn!(
                error = %e,
                conversation_id = conversation_id,
                document_id = doc_id,
                "Failed to add conversation document reference"
            );
        }
    }

    Ok(())
}

pub(super) async fn record_tool_document_references(
    conv_service: &Arc<dyn ConversationServiceTrait>,
    conversation_id: &str,
    tool_name: &str,
    result: &FunctionResult,
) -> Result<()> {
    if !result.success {
        return Ok(());
    }

    let data = match result.data.clone() {
        Some(value) => value,
        None => return Ok(()),
    };

    match tool_name {
        "get_document" => {
            if let Ok(doc) = serde_json::from_value::<GetDocumentOutput>(data) {
                if let Err(e) = conv_service
                    .add_document_reference(conversation_id, doc.document_id.clone(), None, None)
                    .await
                {
                    warn!(
                        error = %e,
                        conversation_id = conversation_id,
                        document_id = doc.document_id.as_str(),
                        "Failed to add document reference from get_document tool"
                    );
                }
            }
        }
        "semantic_search" => {
            if let Ok(output) = serde_json::from_value::<SemanticSearchOutput>(data) {
                for result in output.results.iter() {
                    if let Err(e) = conv_service
                        .add_document_reference(
                            conversation_id,
                            result.document_id.clone(),
                            None,
                            Some(result.score),
                        )
                        .await
                    {
                        warn!(
                            error = %e,
                            conversation_id = conversation_id,
                            document_id = result.document_id.as_str(),
                            "Failed to add document reference from semantic_search tool"
                        );
                    }
                }
            }
        }
        _ => {}
    }

    Ok(())
}

/// How far apart two references can be and still belong to one turn's search.
/// A turn persists its passages in one quick loop; the next turn is a human
/// reply away.
const SAME_TURN_WINDOW: chrono::Duration = chrono::Duration::seconds(60);

/// The document a bare "tell me more" is about: the best-ranked passage of the
/// most recent turn.
///
/// Not simply the newest reference. A turn persists its passages in rank order
/// with neighbour-expansion passages (score 0) at the end, so the newest row is
/// the weakest evidence that turn found.
pub(super) fn followup_reference(
    document_context: &[crate::domain::conversation::DocumentReference],
) -> Option<&crate::domain::conversation::DocumentReference> {
    let newest = document_context.iter().map(|r| r.added_at).max()?;
    document_context
        .iter()
        .filter(|r| newest - r.added_at <= SAME_TURN_WINDOW)
        // `max_by` keeps the later of equals, and the list is oldest first.
        .max_by(|a, b| {
            let score = |r: &crate::domain::conversation::DocumentReference| {
                r.relevance_score.unwrap_or(f32::MIN)
            };
            score(a).total_cmp(&score(b))
        })
}

pub(super) async fn load_recent_document_metadata(
    container: &dyn ChatRuntime,
    document_context: &[crate::domain::conversation::DocumentReference],
) -> Option<super::RecentDocumentMetadata> {
    let last_ref = followup_reference(document_context)?;
    let document_id = last_ref.document_id.clone();
    let doc = container
        .document_repository()
        .find_by_id(&document_id)
        .await
        .ok()??;
    Some(super::RecentDocumentMetadata {
        document_id,
        title: doc.file_name().to_string(),
    })
}

#[cfg(test)]
mod planning_context_tests {
    use super::*;
    use crate::domain::conversation::{ConversationAggregate, MessageRole};

    #[test]
    fn planning_is_bounded_but_can_recover_an_old_subject() {
        let mut aggregate = ConversationAggregate::new("test".into(), "test".into(), None).unwrap();
        aggregate
            .add_message(
                MessageRole::User,
                "SANSI blueberry grow light setup".into(),
                10,
            )
            .unwrap();
        for _ in 0..30 {
            aggregate
                .add_message(
                    MessageRole::Assistant,
                    "unrelated subject ".repeat(1000),
                    1000,
                )
                .unwrap();
        }
        aggregate
            .add_message(MessageRole::User, "latest question".into(), 5)
            .unwrap();
        let context = query_planning_context(aggregate.messages(), "blueberry light").unwrap();
        assert!(context.contains("SANSI blueberry grow light setup"));
        assert!(context.contains("latest question"));
        assert!(context.chars().count() < 8500);
    }
}

#[cfg(test)]
mod followup_reference_tests {
    use super::*;
    use crate::domain::conversation::DocumentReference;

    fn reference(document_id: &str, score: f32, seconds: i64) -> DocumentReference {
        DocumentReference {
            document_id: document_id.to_string(),
            chunk_id: Some(format!("{document_id}-chunk")),
            relevance_score: Some(score),
            added_at: chrono::DateTime::<chrono::Utc>::UNIX_EPOCH
                + chrono::Duration::seconds(seconds),
        }
    }

    /// The previous turn saved its passages in rank order with a score-0
    /// neighbour passage last; "tell me more" reopened that neighbour.
    #[test]
    fn a_followup_reopens_the_best_passage_of_the_last_turn() {
        let refs = [
            reference("earlier-turn", 0.99, 0),
            reference("top", 0.8, 600),
            reference("second", 0.4, 600),
            reference("neighbour", 0.0, 601),
        ];

        let picked = followup_reference(&refs).map(|r| r.document_id.as_str());

        assert_eq!(picked, Some("top"));
    }
}
