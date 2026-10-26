use std::collections::HashSet;
use std::sync::Arc;

use tracing::warn;

use crate::features::conversation::chat::ports::ChatRuntime;
use crate::features::conversation::ConversationServiceTrait;
use lattice_core::application::contracts::sources::SourceDto;
use lattice_core::application::ports::LLMPort;
use lattice_core::domain::conversation::{ConversationMessage, DocumentReference, MessageRole};
use lattice_core::domain::entities::document::Document;
use lattice_core::shared::text::build_excerpt;

use super::super::document_text::{assemble_document_text, truncate_to_token_budget};
use super::{infer_category, select_informative_terms, tokenize_keyword_terms};

/// The document an earlier turn discussed, reopened for a follow-up.
///
/// Rendered only once the turn's sources are numbered, under the number its
/// source was given: an attachment on the same message goes first in the list,
/// so the document is not necessarily `[1]`.
#[derive(Debug)]
pub struct FollowupDocument {
    document_id: String,
    file_name: String,
    content: String,
}

impl FollowupDocument {
    pub(in crate::features::conversation::chat) fn render(
        &self,
        sources: &[SourceDto],
    ) -> Option<String> {
        let label = sources
            .iter()
            .find(|source| source.document_id == self.document_id)?
            .citation_id?;
        Some(format!(
            "[{label}] Document: {}\nDocument ID: {}\nContent: {}",
            self.file_name, self.document_id, self.content
        ))
    }

    /// The document as [`Self::render`] shows it, under a placeholder label:
    /// what it takes of the prompt, counted before the sources are numbered.
    pub(in crate::features::conversation::chat) fn render_for_count(&self) -> String {
        format!(
            "[0] Document: {}
Document ID: {}
Content: {}",
            self.file_name, self.document_id, self.content
        )
    }
}

/// Whether what the conversation already read of the document under
/// discussion (its name and the passages cited from it) shares a term with
/// the follow-up.
///
/// The cited passages, not the whole text: a whole book shares some term with
/// almost any question, so testing all of it stopped the gate filtering at
/// all, and it built a token set of the entire book every turn. The scan
/// stops at the first anchor it meets.
fn document_mentions_an_anchor(
    file_name: &str,
    passages: &[String],
    anchors: &HashSet<String>,
) -> bool {
    std::iter::once(file_name)
        .chain(passages.iter().map(String::as_str))
        .any(|text| mentions_any(text, anchors))
}

/// Whether `text` holds any of `anchors` as a word, read word by word and
/// stopping at the first.
fn mentions_any(text: &str, anchors: &HashSet<String>) -> bool {
    let mut word = String::new();
    for ch in text.chars().chain(std::iter::once(' ')) {
        if ch.is_alphanumeric() {
            word.extend(ch.to_lowercase());
        } else if !word.is_empty() {
            if anchors.contains(&word) {
                return true;
            }
            word.clear();
        }
    }
    false
}

/// The passages the latest answer cited from `document_id`.
fn cited_passages(messages: &[ConversationMessage], document_id: &str) -> Vec<String> {
    let Some(answer) = messages
        .iter()
        .rev()
        .find(|message| message.is_completed() && message.role == MessageRole::Assistant)
    else {
        return Vec::new();
    };
    let Some(metadata) = answer
        .metadata
        .as_deref()
        .and_then(|json| serde_json::from_str::<serde_json::Value>(json).ok())
    else {
        return Vec::new();
    };
    metadata
        .get("sources")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|value| serde_json::from_value::<SourceDto>(value.clone()).ok())
        .filter(|source| source.document_id == document_id)
        .map(|source| source.content)
        .filter(|content| !content.trim().is_empty())
        .collect()
}

/// Room kept for the line saying the document goes on and where.
const CONTINUES_NOTE_TOKENS: usize = 64;

pub(super) async fn build_followup_context(
    container: &dyn ChatRuntime,
    conv_service: &Arc<dyn ConversationServiceTrait>,
    conversation_id: &str,
    document_context: &[DocumentReference],
    highlight_terms: &[String],
    token_budget: usize,
    llm: &Arc<dyn LLMPort>,
    excerpt_chars: usize,
) -> Option<(FollowupDocument, Vec<SourceDto>)> {
    // No room for the document is no document: a fixed excerpt in its place
    // overran the budget it was given.
    if token_budget == 0 {
        return None;
    }
    let last_ref = super::conversation_helpers::followup_reference(document_context)?;
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

    let mut read_before = match conv_service.get_conversation(conversation_id).await {
        Ok(Some(aggregate)) => cited_passages(aggregate.messages(), &doc_id),
        _ => Vec::new(),
    };
    let referenced = last_ref
        .chunk_id
        .as_deref()
        .and_then(|id| {
            document
                .chunks()
                .iter()
                .find(|chunk| chunk.id().as_str() == id)
        })
        .map(|chunk| chunk.content().to_string());
    read_before.extend(referenced);
    if !followup_anchor_terms.is_empty()
        && !read_before.is_empty()
        && !document_mentions_an_anchor(document.file_name(), &read_before, &followup_anchor_terms)
    {
        warn!(
            conversation_id = conversation_id,
            document_id = doc_id.as_str(),
            anchor_terms = ?followup_anchor_terms,
            "Skipping follow-up document reuse due to zero topical anchor overlap"
        );
        return None;
    }

    // What does not fit is said, with where to read on, never cut silently.
    let trimmed_content = if llm.count_tokens(&content) <= token_budget {
        content.clone()
    } else {
        let shown = truncate_to_token_budget(
            &content,
            token_budget.saturating_sub(CONTINUES_NOTE_TOKENS),
            llm,
        );
        let shown_chars = shown.chars().count();
        format!(
            "{shown}\n[The document continues: the first {shown_chars} of its {} characters are shown. Call get_document with document_id=\"{doc_id}\" and offset={shown_chars} to read on.]",
            content.chars().count()
        )
    };

    let mut sources = build_followup_sources(&document, last_ref, highlight_terms, excerpt_chars);
    if let Some(source) = sources.first_mut() {
        source.content = trimmed_content.to_string();
        source.excerpt = None;
        source.page_number = None;
        source.chunk_excerpts = None;
    }

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

    let followup = FollowupDocument {
        document_id: doc_id,
        file_name: document.file_name().to_string(),
        content: trimmed_content,
    };
    Some((followup, sources))
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
        page_number: None,
        from_ocr: false,
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
        citation_id: None,

        web_snapshot: None,
    }]
}

#[cfg(test)]
mod followup_label_tests {
    use super::*;

    fn source(document_id: &str) -> SourceDto {
        SourceDto {
            page_number: None,
            from_ocr: false,
            document_id: document_id.to_string(),
            chunk_id: format!("{document_id}-chunk"),
            content: String::new(),
            score: 1.0,
            path: None,
            position: None,
            file_name: document_id.to_string(),
            file_path: document_id.to_string(),
            mime_type: "text/plain".to_string(),
            category: "Text".to_string(),
            file_size_bytes: 0,
            modified_at: String::new(),
            excerpt: None,
            highlights: None,
            section: None,
            chunk_index: None,
            chunk_excerpts: None,
            citation_id: None,

            web_snapshot: None,
        }
    }

    /// The follow-up text was hardcoded as [1]; with a file attached to the
    /// same message the attachment is [1] and the document is [2].
    #[test]
    fn a_followup_behind_an_attachment_is_labelled_two() {
        let mut sources = vec![source("attached"), source("earlier")];
        super::super::assign_citation_ids(&mut sources);
        let document = FollowupDocument {
            document_id: "earlier".to_string(),
            file_name: "earlier.md".to_string(),
            content: "text".to_string(),
        };

        let rendered = document.render(&sources).unwrap_or_default();

        assert!(
            rendered.starts_with("[2] Document: earlier.md\n"),
            "{rendered}"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn anchors(text: &str) -> HashSet<String> {
        super::super::tokenize_overlap_terms(text)
            .into_iter()
            .collect()
    }

    /// A follow-up about a passage the last answer cited far into a long
    /// document is still about that document.
    #[test]
    fn an_anchor_in_a_cited_passage_keeps_the_document() {
        let passage = format!(
            "{} The appendix covers photosynthesis in detail.",
            "Filler about unrelated matters. ".repeat(200)
        );
        assert!(document_mentions_an_anchor(
            "notes.pdf",
            &[passage],
            &anchors("photosynthesis")
        ));
    }

    /// The whole text of a book shares some word with almost any question,
    /// so testing all of it never filtered anything. Only what was cited
    /// from it counts.
    #[test]
    fn a_term_only_elsewhere_in_the_book_does_not_keep_it() {
        assert!(!document_mentions_an_anchor(
            "notes.pdf",
            &["A chapter about rivers and their deltas.".to_string()],
            &anchors("photosynthesis")
        ));
        assert!(mentions_any(
            "Rivers, deltas; PHOTOSYNTHESIS.",
            &anchors("photosynthesis")
        ));
        assert!(!mentions_any("photosynthetic", &anchors("photosynthesis")));
    }
}
