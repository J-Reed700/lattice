use std::collections::HashSet;
use std::sync::Arc;

use tracing::{debug, warn};

use crate::features::conversation::chat::ports::ChatRuntime;
use crate::features::conversation::ConversationServiceTrait;
use lattice_core::application::contracts::search::SearchResultDto;
use lattice_core::application::contracts::tools::FunctionResult;
use lattice_core::application::contracts::tools::{GetDocumentOutput, SemanticSearchOutput};
use lattice_core::shared::error::Result;

pub(super) async fn load_space_document_scope(
    container: &dyn ChatRuntime,
    conversation_id: &str,
) -> Option<super::SpaceDocumentScope> {
    let repository = container.chat_records();
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
    planner: &dyn lattice_core::application::ports::LLMPort,
    within_tokens: Option<usize>,
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

    // The window share, bounded by what a call with a deadline reads in time.
    let room = lattice_core::application::services::history_share::auxiliary_history_room(
        planner, question,
    );
    let room = within_tokens.map_or(room, |within| room.min(within));
    query_planning_context(aggregate.messages(), question, room, &|text| {
        planner.count_tokens(text)
    })
}

/// The query planner's view of the conversation, within `room` tokens.
///
/// This is a query-planning view, not the answering model's memory: the
/// planner gets a share of its own window, so a small utility model is never
/// handed the whole transcript (which caused rewrite timeouts), and a large
/// one sees more. The newest turns come first, whole, in up to half the room;
/// older turns that share the question's terms, best match first, fill the
/// rest; a line counts what is left out. A newest turn too long for the room
/// on its own is the one thing cut, centred on the question's terms.
fn query_planning_context(
    messages: &[lattice_core::domain::conversation::ConversationMessage],
    question: &str,
    room: usize,
    count_tokens: &dyn Fn(&str) -> usize,
) -> Option<String> {
    let cleaned = super::search_text_without_url_tracking(question);
    let terms = super::select_informative_terms(super::tokenize_keyword_terms(&cleaned), 16);
    let eligible: Vec<_> = messages
        .iter()
        .filter(|message| message.is_completed() && !message.content.trim().is_empty())
        .map(|message| format!("{}: {}", message.role, message.content.trim()))
        .collect();
    let cost = |text: &String| count_tokens(text).saturating_add(1);

    let (recent, _) = lattice_core::application::services::history_share::newest_that_fit(
        &eligible,
        room / 2,
        cost,
    );
    let first_recent = eligible.len() - recent.len();
    let mut used: usize = recent.iter().map(cost).sum();
    let mut selected: Vec<(usize, String)> = recent
        .iter()
        .enumerate()
        .map(|(offset, text)| (first_recent + offset, text.clone()))
        .collect();
    if selected.is_empty() {
        if let Some(newest) = eligible.last() {
            let chars = lattice_core::application::ports::llm_port::chars_within_tokens(
                room.saturating_sub(used),
                4.0,
            );
            let excerpt = lattice_core::shared::text::build_excerpt(newest, &terms, chars);
            if !excerpt.is_empty() {
                debug!(
                    chars = newest.chars().count(),
                    room, "The newest turn is longer than the planner's room; carrying an excerpt"
                );
                used = used.saturating_add(count_tokens(&excerpt));
                selected.push((eligible.len() - 1, excerpt));
            }
        }
    }

    let first_selected = selected.first().map_or(eligible.len(), |(index, _)| *index);
    let mut older: Vec<(usize, usize, &String)> = eligible
        .iter()
        .take(first_selected)
        .enumerate()
        .filter_map(|(index, text)| {
            let lower = text.to_lowercase();
            let score = terms
                .iter()
                .filter(|term| lower.contains(term.as_str()))
                .count();
            (score > 0).then_some((score, index, text))
        })
        .collect();
    older.sort_by_key(|(score, index, _)| (std::cmp::Reverse(*score), std::cmp::Reverse(*index)));
    for (_, index, text) in older {
        let next = used.saturating_add(cost(text));
        if next <= room {
            used = next;
            selected.push((index, text.clone()));
        }
    }
    if selected.is_empty() {
        return None;
    }
    selected.sort_by_key(|(index, _)| *index);
    let omitted = eligible.len() - selected.len();
    let entries: Vec<String> =
        lattice_core::application::services::history_share::omitted_note(omitted)
            .into_iter()
            .chain(selected.into_iter().map(|(_, text)| text))
            .collect();
    Some(entries.join("\n"))
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
    document_context: &[lattice_core::domain::conversation::DocumentReference],
) -> Option<&lattice_core::domain::conversation::DocumentReference> {
    let newest = document_context.iter().map(|r| r.added_at).max()?;
    document_context
        .iter()
        .filter(|r| newest - r.added_at <= SAME_TURN_WINDOW)
        // `max_by` keeps the later of equals, and the list is oldest first.
        .max_by(|a, b| {
            let score = |r: &lattice_core::domain::conversation::DocumentReference| {
                r.relevance_score.unwrap_or(f32::MIN)
            };
            score(a).total_cmp(&score(b))
        })
}

pub(super) async fn load_recent_document_metadata(
    container: &dyn ChatRuntime,
    document_context: &[lattice_core::domain::conversation::DocumentReference],
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
    use lattice_core::domain::conversation::{ConversationAggregate, MessageRole};

    #[test]
    fn planning_is_bounded_but_can_recover_an_old_subject() {
        let mut aggregate = ConversationAggregate::new("test".into(), None).unwrap();
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
        let count = |text: &str| text.chars().count() / 4 + 1;
        let context =
            query_planning_context(aggregate.messages(), "blueberry light", 3_000, &count).unwrap();
        assert!(context.contains("SANSI blueberry grow light setup"));
        assert!(context.contains("latest question"));
        assert!(context.contains("earlier messages not shown"), "{context}");
        assert!(count(&context) <= 3_000 + 40);
    }

    /// A window with room for every turn carries every turn, whole.
    #[test]
    fn planning_carries_every_turn_the_room_holds() {
        let mut aggregate = ConversationAggregate::new("test".into(), None).unwrap();
        for index in 0..20 {
            aggregate
                .add_message(MessageRole::User, format!("turn {index}"), 3)
                .unwrap();
        }
        let count = |text: &str| text.chars().count() / 4 + 1;
        let context =
            query_planning_context(aggregate.messages(), "anything", 100_000, &count).unwrap();
        assert!(
            (0..20).all(|index| context.contains(&format!("turn {index}\n"))
                || context.ends_with(&format!("turn {index}")))
        );
        assert!(!context.contains("not shown"));
    }
}

#[cfg(test)]
mod followup_reference_tests {
    use super::*;
    use lattice_core::domain::conversation::DocumentReference;

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
