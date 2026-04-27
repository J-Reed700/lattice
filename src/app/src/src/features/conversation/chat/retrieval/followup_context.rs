use std::collections::HashSet;
use std::sync::Arc;

use tracing::warn;

use crate::features::qa::dto::SourceDto;
use crate::application::ports::LLMPort;
use crate::domain::conversation::{ConversationMessage, DocumentReference, MessageRole};
use crate::domain::entities::document::Document;
use crate::features::conversation::ConversationServiceTrait;
use crate::interfaces::di::Container;
use crate::shared::text_utils::{build_excerpt, safe_truncate};

use super::{infer_category, select_informative_terms, tokenize_keyword_terms};

pub(super) async fn build_followup_context(
    container: &Container,
    conv_service: &Arc<dyn ConversationServiceTrait>,
    conversation_id: &str,
    document_context: &[DocumentReference],
    highlight_terms: &[String],
    token_budget: usize,
    llm: &Arc<dyn LLMPort>,
    excerpt_chars: usize,
) -> Option<(String, Vec<SourceDto>)> {
    let last_ref = document_context.last()?;
    let doc_id = last_ref.document_id.clone();
    let followup_anchor_terms =
        load_followup_turn_anchor_terms(conv_service, conversation_id).await;

    let document = match container.document_repository().find_by_id(&doc_id).await {
        Ok(Some(doc)) => doc,
        Ok(None) => {
            warn!(
                document_id = doc_id.as_str(),
                "Follow-up document not found"
            );
            return None;
        }
        Err(e) => {
            warn!(error = %e, document_id = doc_id.as_str(), "Failed to load follow-up document");
            return None;
        }
    };

    let content = assemble_document_text(&document);
    if content.trim().is_empty() {
        return None;
    }

    if !followup_anchor_terms.is_empty() {
        let mut anchor_haystack = String::new();
        anchor_haystack.push_str(document.file_name());
        anchor_haystack.push(' ');
        anchor_haystack.push_str(&safe_truncate(&content, 18_000));
        let haystack_terms: HashSet<String> = super::tokenize_overlap_terms(&anchor_haystack)
            .into_iter()
            .collect();
        let anchor_hits = followup_anchor_terms
            .iter()
            .filter(|term| haystack_terms.contains(term.as_str()))
            .count();
        if anchor_hits == 0 {
            warn!(
                conversation_id = conversation_id,
                document_id = doc_id.as_str(),
                anchor_terms = ?followup_anchor_terms,
                "Skipping follow-up document reuse due to zero topical anchor overlap"
            );
            return None;
        }
    }

    let trimmed_content = if token_budget == 0 {
        safe_truncate(&content, excerpt_chars)
    } else {
        truncate_to_token_budget(&content, token_budget, llm)
    };

    let context_text = format!(
        "[1] Document: {}\nDocument ID: {}\nContent: {}",
        document.file_name(),
        doc_id,
        trimmed_content
    );

    let sources = build_followup_sources(&document, last_ref, highlight_terms, excerpt_chars);

    if let Err(e) = conv_service
        .add_document_reference(
            conversation_id,
            doc_id.clone(),
            last_ref.chunk_id.clone(),
            last_ref.relevance_score,
        )
        .await
    {
        warn!(
            error = %e,
            conversation_id = conversation_id,
            document_id = doc_id.as_str(),
            "Failed to persist follow-up document reference"
        );
    }

    Some((context_text, sources))
}

pub(super) fn extract_turn_anchor_terms(user_text: &str, assistant_text: &str) -> HashSet<String> {
    let user_terms: HashSet<String> =
        select_informative_terms(tokenize_keyword_terms(user_text), 16)
            .into_iter()
            .filter(|term| term.len() >= 7 && term.chars().any(|c| c.is_ascii_alphabetic()))
            .collect();
    if user_terms.is_empty() {
        return HashSet::new();
    }

    let mut anchors = HashSet::new();
    for term in tokenize_keyword_terms(assistant_text).into_iter() {
        if term.len() >= 7
            && term.chars().any(|c| c.is_ascii_alphabetic())
            && user_terms.contains(&term)
        {
            anchors.insert(term);
            if anchors.len() >= 12 {
                break;
            }
        }
    }
    anchors
}

pub(super) async fn load_followup_turn_anchor_terms(
    conv_service: &Arc<dyn ConversationServiceTrait>,
    conversation_id: &str,
) -> HashSet<String> {
    let aggregate = match conv_service.get_conversation(conversation_id).await {
        Ok(Some(value)) => value,
        _ => return HashSet::new(),
    };

    let mut recent_assistant: Option<&ConversationMessage> = None;
    let mut recent_user: Option<&ConversationMessage> = None;

    for message in aggregate.messages().iter().rev() {
        match message.role {
            MessageRole::Assistant => {
                if recent_assistant.is_none() {
                    recent_assistant = Some(message);
                }
            }
            MessageRole::User => {
                if recent_assistant.is_some() {
                    recent_user = Some(message);
                    break;
                }
            }
            MessageRole::System => {}
        }
    }

    let Some(user) = recent_user else {
        return HashSet::new();
    };
    let Some(assistant) = recent_assistant else {
        return HashSet::new();
    };

    extract_turn_anchor_terms(&user.content, &assistant.content)
}

fn assemble_document_text(document: &Document) -> String {
    let chunks = document.chunks();
    if chunks.is_empty() {
        return document.content().to_string();
    }

    let mut sorted_chunks: Vec<_> = chunks.iter().collect();
    sorted_chunks.sort_by_key(|c| c.index());
    sorted_chunks
        .iter()
        .map(|c| c.content())
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn truncate_to_token_budget(content: &str, token_budget: usize, llm: &Arc<dyn LLMPort>) -> String {
    if token_budget == 0 {
        return String::new();
    }

    let token_count = llm.count_tokens(content);
    if token_count <= token_budget {
        return content.to_string();
    }

    let total_chars = content.chars().count();
    if total_chars == 0 {
        return String::new();
    }

    let ratio = token_budget as f64 / token_count as f64;
    let mut target_chars = ((total_chars as f64) * ratio).floor() as usize;
    if target_chars == 0 {
        target_chars = 1;
    }

    let mut truncated = safe_truncate(content, target_chars);
    let mut attempts = 0;
    while llm.count_tokens(&truncated) > token_budget && attempts < 3 && target_chars > 1 {
        target_chars = ((target_chars as f64) * 0.8).floor() as usize;
        if target_chars == 0 {
            break;
        }
        truncated = safe_truncate(content, target_chars);
        attempts += 1;
    }

    truncated
}

fn build_followup_sources(
    document: &Document,
    reference: &DocumentReference,
    highlight_terms: &[String],
    excerpt_chars: usize,
) -> Vec<SourceDto> {
    let chunk = reference
        .chunk_id
        .as_deref()
        .and_then(|id| document.chunks().iter().find(|c| c.id().as_str() == id))
        .or_else(|| document.chunks().first());

    let (chunk_id, chunk_content, chunk_index, section) = if let Some(chunk) = chunk {
        (
            chunk.id().as_str().to_string(),
            chunk.content().to_string(),
            Some(chunk.index()),
            chunk.section().map(|s| s.to_string()),
        )
    } else {
        (
            document.id().as_str().to_string(),
            document.content().to_string(),
            None,
            None,
        )
    };

    let excerpt = if chunk_content.trim().is_empty() {
        None
    } else {
        Some(build_excerpt(
            &chunk_content,
            highlight_terms,
            excerpt_chars,
        ))
    };

    let highlights = if highlight_terms.is_empty() {
        None
    } else {
        Some(highlight_terms.to_vec())
    };

    let file_path = document.file_path().display().to_string();
    let category = infer_category(&file_path);

    vec![SourceDto {
        document_id: document.id().as_str().to_string(),
        chunk_id,
        content: chunk_content,
        score: reference.relevance_score.unwrap_or(1.0),
        path: if file_path.is_empty() {
            None
        } else {
            Some(file_path.clone())
        },
        position: chunk_index,
        file_name: document.file_name().to_string(),
        file_path,
        mime_type: document.mime_type().to_string(),
        category,
        file_size_bytes: document.size_bytes(),
        modified_at: document.modified_at().to_rfc3339(),
        excerpt,
        highlights,
        section,
        chunk_index,
        chunk_excerpts: None,
    }]
}

pub(super) fn source_excerpt_text(source: &SourceDto) -> Option<String> {
    let excerpt = source
        .excerpt
        .as_deref()
        .unwrap_or(&source.content)
        .trim()
        .to_string();

    if excerpt.is_empty() {
        None
    } else {
        Some(safe_truncate(&excerpt, 420))
    }
}
