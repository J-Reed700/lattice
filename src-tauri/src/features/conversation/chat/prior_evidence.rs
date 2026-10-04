//! Restore original source passages from completed turns, not the assistant's
//! prose. Citation numbers belong to an answer, so recalled sources receive
//! fresh numbers alongside this turn's new evidence.

use std::collections::HashSet;
use std::sync::Arc;

use crate::application::ports::LLMPort;
use crate::domain::conversation::{ConversationMessage, MessageRole};
use crate::features::qa::dto::SourceDto;
use crate::features::search::engine::query_expansion::dictionaries::select_informative_terms;
use crate::interfaces::di::Container;
use crate::shared::error::Result;
use crate::shared::text::build_excerpt;

use super::retrieval::WEB_SOURCE_PREFIX;

const MAX_SOURCES: usize = 8;
const PASSAGE_CHARS: usize = 2400;

/// Search the entire saved source history before applying a prompt budget.
/// An old citation is not excluded merely because newer research followed it.
/// Document evidence still obeys the current scope and explicit focus.
pub(super) async fn recall(
    container: &Container,
    conversation_id: &str,
    question: &str,
    allowed_documents: &HashSet<String>,
    existing: &[SourceDto],
    token_budget: usize,
    llm: &Arc<dyn LLMPort>,
) -> Result<Vec<SourceDto>> {
    if token_budget == 0 {
        return Ok(Vec::new());
    }
    let Some(aggregate) = container
        .conversation_history()
        .get_conversation(conversation_id)
        .await?
    else {
        return Ok(Vec::new());
    };
    let mut terms = query_terms(question);
    if terms.len() < 3 {
        if let Some(previous) = aggregate
            .messages()
            .iter()
            .rev()
            .find(|message| message.is_completed() && message.role == MessageRole::User)
        {
            terms.extend(query_terms(&previous.content));
        }
    }
    let candidates = rank_sources(aggregate.messages(), &terms, allowed_documents, existing);
    let mut recalled = Vec::new();
    let mut used = 0;
    for mut source in candidates.into_iter().take(MAX_SOURCES) {
        if source.document_id.starts_with(WEB_SOURCE_PREFIX) {
            let url = source.path.as_deref().unwrap_or(&source.file_path);
            if let Some(page) =
                super::source_snapshots::archived_page(container, conversation_id, url).await
            {
                source.content = page.content;
            }
        }
        source.content = build_excerpt(&source.content, &terms, PASSAGE_CHARS);
        source.excerpt = Some(source.content.clone());
        let cost = llm.count_tokens(&render_source(&source));
        if used + cost > token_budget {
            continue;
        }
        used += cost;
        recalled.push(source);
    }
    Ok(recalled)
}

fn query_terms(question: &str) -> Vec<String> {
    let cleaned = super::retrieval::search_text_without_url_tracking(question);
    let terms = cleaned
        .split(|ch: char| !ch.is_alphanumeric())
        .filter(|term| term.len() >= 3)
        .map(str::to_lowercase)
        .collect();
    select_informative_terms(terms, 24)
}

fn rank_sources(
    messages: &[ConversationMessage],
    terms: &[String],
    allowed_documents: &HashSet<String>,
    existing: &[SourceDto],
) -> Vec<SourceDto> {
    let seen: HashSet<String> = existing.iter().map(source_key).collect();
    let mut positions = std::collections::HashMap::<String, usize>::new();
    let mut ranked: Vec<(usize, SourceDto)> = Vec::new();
    for message in messages
        .iter()
        .rev()
        .filter(|message| message.is_completed() && message.role == MessageRole::Assistant)
    {
        let Some(metadata) = message
            .metadata
            .as_deref()
            .and_then(|json| serde_json::from_str::<serde_json::Value>(json).ok())
        else {
            continue;
        };
        let Some(sources) = metadata
            .get("sources")
            .and_then(serde_json::Value::as_array)
        else {
            continue;
        };
        for value in sources {
            let Ok(mut source) = serde_json::from_value::<SourceDto>(value.clone()) else {
                continue;
            };
            if !source.document_id.starts_with(WEB_SOURCE_PREFIX)
                && !allowed_documents.contains(&source.document_id)
            {
                continue;
            }
            if source.content.trim().is_empty() {
                continue;
            }
            let title = source.file_name.to_lowercase();
            let body = source.content.to_lowercase();
            let score: usize = terms
                .iter()
                .map(|term| {
                    usize::from(title.contains(term)) * 3 + usize::from(body.contains(term))
                })
                .sum();
            let key = source_key(&source);
            if score == 0 || seen.contains(&key) {
                continue;
            }
            source.citation_id = None;
            // Old web-result-N ids are reused by every search. Give recalled
            // passages a distinct identity before the shared citation map runs.
            if !source.chunk_id.starts_with("recalled:") {
                source.chunk_id = format!("recalled:{}:{}", source.document_id, source.chunk_id);
            }
            if let Some(index) = positions.get(&key) {
                if let Some(existing) = ranked.get_mut(*index) {
                    // A later answer may carry only a clipped excerpt. Keep
                    // the original, fuller evidence when it is still saved.
                    if source.content.len() > existing.1.content.len() {
                        *existing = (score, source);
                    }
                }
            } else {
                positions.insert(key, ranked.len());
                ranked.push((score, source));
            }
        }
    }
    // Stable ties retain newest-first order; relevance beats recency.
    ranked.sort_by_key(|entry| std::cmp::Reverse(entry.0));
    ranked.into_iter().map(|(_, source)| source).collect()
}

pub(super) fn source_key(source: &SourceDto) -> String {
    if source.document_id.starts_with(WEB_SOURCE_PREFIX) {
        source.document_id.clone()
    } else {
        // Strip the recall wrapper so carrying a passage again cannot multiply it.
        let prefix = format!("recalled:{}:", source.document_id);
        format!(
            "{}:{}",
            source.document_id,
            source
                .chunk_id
                .strip_prefix(&prefix)
                .unwrap_or(&source.chunk_id)
        )
    }
}

fn render_source(source: &SourceDto) -> String {
    format!(
        "[{}] {}\nDocument ID: {}\nLocation: {}\nPreviously read source passage: {}",
        source.citation_id.unwrap_or(9999),
        source.file_name,
        source.document_id,
        source.path.as_deref().unwrap_or(&source.file_path),
        source.content
    )
}

pub(super) fn render(sources: &[SourceDto]) -> Option<String> {
    if sources.is_empty() {
        return None;
    }
    Some(format!(
        "Evidence retained from earlier in this conversation. These are original source passages, \
         citable with the numbers below. Combine them with new evidence; a new search does not \
         by itself invalidate prior findings. Apply later corrections and reconcile conflicting evidence. Do not reuse citation numbers from earlier assistant messages.\n\n{}",
        sources.iter().map(render_source).collect::<Vec<_>>().join("\n\n")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::conversation::ConversationAggregate;

    fn source(id: &str, body: &str) -> SourceDto {
        SourceDto {
            document_id: id.into(),
            chunk_id: "web-result-1".into(),
            content: body.into(),
            score: 1.0,
            file_name: body.into(),
            file_path: "https://example.com/growing".into(),
            path: None,
            mime_type: "text/html".into(),
            category: "Web Article".into(),
            file_size_bytes: 0,
            modified_at: String::new(),
            excerpt: None,
            highlights: None,
            section: None,
            chunk_index: None,
            chunk_excerpts: None,
            citation_id: Some(1),
            page_number: None,
            position: None,

            web_snapshot: None,
        }
    }

    fn message(source: SourceDto) -> ConversationMessage {
        let mut aggregate = ConversationAggregate::new("test".into(), "test".into(), None).unwrap();
        aggregate
            .add_message(MessageRole::Assistant, "An earlier answer [1]".into(), 10)
            .unwrap();
        let mut message = aggregate.messages().last().unwrap().clone();
        message.metadata = Some(serde_json::json!({"sources": [source]}).to_string());
        message
    }

    #[test]
    fn recalls_old_original_evidence_past_ten_newer_searches_with_new_citation_ids() {
        let old = source(
            "web:https://example.com/grow-lights",
            "Blueberry grow light requirements: original evidence.",
        );
        let mut messages = vec![message(old.clone())];
        for i in 0..30 {
            messages.push(message(source(
                &format!("web:https://example.com/solar/{i}"),
                "Solar panels for electricity",
            )));
        }
        let current = source("web:https://example.com/current", "new source");
        let recalled = rank_sources(
            &messages,
            &query_terms("blueberry grow lights"),
            &HashSet::new(),
            std::slice::from_ref(&current),
        );
        assert_eq!(recalled.len(), 1);
        assert_eq!(recalled[0].content, old.content);
        assert_eq!(recalled[0].citation_id, None);
        assert_ne!(recalled[0].chunk_id, current.chunk_id);
        let mut combined = vec![current];
        combined.extend(recalled);
        super::super::retrieval::assign_citation_ids(&mut combined);
        assert_eq!(combined[1].citation_id, Some(2));
        let prompt = render(&combined[1..]).unwrap();
        assert!(prompt.contains("[2]"));
        assert!(prompt.contains("original evidence"));
        assert!(!prompt.contains("An earlier answer"));
    }

    #[test]
    fn scoped_documents_failed_answers_and_current_duplicates_are_excluded() {
        let web = source(
            "web:https://example.com/grow-lights",
            "Blueberry grow light evidence",
        );
        let doc = source("private-doc", "Blueberry grow light evidence");
        let mut failed = message(source(
            "web:https://example.com/failed",
            "Blueberry evidence",
        ));
        failed.status = "failed".into();
        let messages = vec![message(web.clone()), message(doc), failed];
        let terms = query_terms("blueberry light");
        assert!(rank_sources(
            &messages,
            &terms,
            &HashSet::new(),
            std::slice::from_ref(&web)
        )
        .is_empty());
        let allowed = ["private-doc".to_string()].into_iter().collect();
        let recalled = rank_sources(&messages, &terms, &allowed, &[web]);
        assert_eq!(recalled.len(), 1);
        assert_eq!(recalled[0].document_id, "private-doc");
    }

    #[test]
    fn a_later_excerpt_does_not_erase_the_original_document_passage() {
        let original = source(
            "doc",
            "Blueberry light requirements and the complete original growing instructions.",
        );
        let mut clipped = original.clone();
        clipped.content = "Blueberry light requirements".into();
        clipped.chunk_id = format!("recalled:{}:{}", original.document_id, original.chunk_id);
        let messages = vec![message(original.clone()), message(clipped)];
        let allowed = ["doc".to_string()].into_iter().collect();
        let recalled = rank_sources(&messages, &query_terms("blueberry light"), &allowed, &[]);
        assert_eq!(recalled.len(), 1);
        assert_eq!(recalled[0].content, original.content);
    }

    #[test]
    fn recurring_recall_does_not_duplicate_passage_identity() {
        let original = source("doc", "Blueberry light evidence");
        let mut recalled = original.clone();
        recalled.chunk_id = format!("recalled:{}:{}", original.document_id, original.chunk_id);
        assert_eq!(source_key(&original), source_key(&recalled));
    }
}
