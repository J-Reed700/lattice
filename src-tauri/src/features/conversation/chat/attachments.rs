//! The files a reader attaches to a message.
//!
//! Attaching a file imports it into the library, which is not the same thing
//! as the turn reading it. Those two were confused until this module existed:
//! the file was imported and indexed, its name was stamped on the message, and
//! whether the model ever saw a word of it came down to whether ordinary
//! retrieval happened to rank it. On a turn with web search forced on, the
//! vault is not searched at all — so "here it is, are you not receiving it?"
//! was answered with "nothing came through", with the file sitting indexed in
//! the library the whole time.
//!
//! So attachments do not go through retrieval. The documents this turn names
//! are loaded, carried into the prompt whole as far as the window allows, and
//! cited like any other source. Retrieval still runs alongside for whatever
//! else the question needs; these are simply always there.
//!
//! Unlike the composer's `@` focus, these ids are not intersected with the
//! conversation's space scope. Focus names documents from a picker that can
//! show more than this chat may read, so it has to fail closed; an attachment
//! is a file the reader is importing into this chat as they send, and the
//! import puts it in this chat's space before the message goes out. Failing
//! closed here would only bring back the bug above in a quieter form.

use std::collections::HashSet;
use std::sync::Arc;

use tracing::{info, warn};

use crate::application::ports::LLMPort;
use crate::domain::entities::document::Document;
use crate::features::conversation::chat::ports::ChatStorage;
use crate::features::conversation::ConversationServiceTrait;
use crate::features::qa::dto::SourceDto;
use crate::shared::text::{build_excerpt, safe_truncate};

use super::document_text::{assemble_document_text, truncate_to_token_budget};
use super::retrieval::infer_category;

/// The share of a turn's retrieval budget attachments may claim.
///
/// They are the reader's own material and outrank anything a search turns up,
/// but a turn that attaches a book must still have room to search, cite and
/// answer. What is left over goes back to retrieval, so a small attachment
/// costs a small amount.
const ATTACHMENT_BUDGET_SHARE: f64 = 0.6;

/// Appended to an attachment the window could not hold, so the model reports a
/// partial read as partial instead of answering as though it saw the end.
const TRUNCATION_NOTE: &str = "\n\n[Attachment truncated here: the rest did not fit the context window. Say so if the answer depends on the part that is missing.]";

/// One attached file, as far as the window allowed it in.
#[derive(Debug, Clone)]
pub(super) struct AttachedDocument {
    /// The citation the reader sees; its `content` is the carried text.
    pub source: SourceDto,
    /// Whether the budget cut the text short.
    pub truncated: bool,
}

/// What this turn's attachments contribute: text for the prompt, sources for
/// the citation list, and the names of any file whose text could not be read.
#[derive(Debug, Clone, Default)]
pub(super) struct TurnAttachments {
    documents: Vec<AttachedDocument>,
    /// Attached, but with no readable text yet — a still-extracting import, or
    /// a file the extractor could make nothing of.
    unreadable: Vec<String>,
}

impl TurnAttachments {
    /// Files that reached the message but not the library in time — an import
    /// still running when the message was sent, or one that failed.
    ///
    /// They are named in the prompt all the same. A file the reader can see on
    /// their own message must never be answered as a file that never arrived.
    pub(super) fn still_importing(names: &[String]) -> Self {
        Self {
            documents: Vec::new(),
            unreadable: names.to_vec(),
        }
    }

    pub(super) fn is_empty(&self) -> bool {
        self.documents.is_empty() && self.unreadable.is_empty()
    }

    /// The ids carried whole. Retrieval passages from these documents are
    /// dropped: the whole text is already in the prompt, and a second copy of
    /// three paragraphs of it buys nothing but tokens and a duplicate footnote.
    pub(super) fn carried_document_ids(&self) -> HashSet<String> {
        self.documents
            .iter()
            .map(|attached| attached.source.document_id.clone())
            .collect()
    }

    /// The citations, for the turn's source list. Numbering happens later, over
    /// the whole list, so these go in before `assign_citation_ids` runs.
    pub(super) fn sources(&self) -> Vec<SourceDto> {
        self.documents
            .iter()
            .map(|attached| attached.source.clone())
            .collect()
    }

    pub(super) fn carried_count(&self) -> usize {
        self.documents.len()
    }

    pub(super) fn truncated_count(&self) -> usize {
        self.documents
            .iter()
            .filter(|attached| attached.truncated)
            .count()
    }

    pub(super) fn unreadable_names(&self) -> &[String] {
        &self.unreadable
    }

    /// A short digest of what the attachments are about, for the utility model
    /// that rewrites this turn into a web query.
    ///
    /// "Please reference this previous chat that I have attached, to do
    /// research" says nothing about the subject — the subject is in the file.
    /// Rewriting from the message alone turned a question about growing food
    /// indoors in Chicago into seven searches about reviewing chat logs,
    /// eDiscovery and Microsoft Teams. The rewriter needs to see what arrived.
    pub(super) fn subject_digest(&self, max_chars: usize) -> Option<String> {
        if self.documents.is_empty() {
            return None;
        }
        let share = (max_chars / self.documents.len()).max(1);
        let mut digest = String::from("Files attached to this message:\n");
        for attached in &self.documents {
            digest.push_str(&format!(
                "- {}: {}\n",
                attached.source.file_name,
                safe_truncate(attached.source.content.trim(), share).trim()
            ));
        }
        Some(digest)
    }

    /// What the carried text costs, for the budget the rest of the turn gets.
    pub(super) fn prompt_tokens(&self, llm: &Arc<dyn LLMPort>) -> usize {
        self.documents
            .iter()
            .map(|attached| llm.count_tokens(&attached.source.content))
            .sum()
    }

    /// The prompt section, numbered with the citation ids the sources ended up
    /// with. Call after `assign_citation_ids`: an attachment the model is told
    /// to cite as `[2]` has to be the source the reader's `[2]` opens.
    ///
    /// `sources` is the turn's final list rather than this struct's copy,
    /// because numbering happens over that list.
    pub(super) fn render(&self, sources: &[SourceDto]) -> Option<String> {
        if self.is_empty() {
            return None;
        }

        let mut section = String::from(
            "Attached Files (the reader attached these to this message; they are \
this turn's primary material — read them before the results below, and do not \
say a file is missing when it is listed here):\n",
        );

        for attached in &self.documents {
            let citation = sources
                .iter()
                .find(|source| source.chunk_id == attached.source.chunk_id)
                .and_then(|source| source.citation_id)
                .or(attached.source.citation_id);
            let label = match citation {
                Some(id) => format!("[{id}] "),
                None => String::new(),
            };
            section.push_str(&format!(
                "\n{label}Attached file: {}\nDocument ID: {}\nContent: {}{}\n",
                attached.source.file_name,
                attached.source.document_id,
                attached.source.content,
                if attached.truncated {
                    TRUNCATION_NOTE
                } else {
                    ""
                },
            ));
        }

        if !self.unreadable.is_empty() {
            section.push_str(&format!(
                "\nAttached but not read this turn (still importing, no text could be \
extracted, or no room was left in the context window): {}. Say that this file \
arrived and could not be read — do not claim no file was attached.\n",
                self.unreadable.join(", ")
            ));
        }

        Some(section)
    }
}

/// Load this turn's attachments and carry them as far as `token_budget` allows.
///
/// Documents are read in the order the reader attached them, each taking an
/// even share of what is left, so the first attachment cannot starve the last.
/// Unused budget rolls forward: two short files and one long one leave the long
/// one nearly everything.
#[allow(clippy::too_many_arguments)]
pub(super) async fn build_turn_attachments(
    container: &dyn ChatStorage,
    conv_service: &Arc<dyn ConversationServiceTrait>,
    conversation_id: &str,
    document_ids: &[String],
    token_budget: usize,
    llm: &Arc<dyn LLMPort>,
    highlight_terms: &[String],
    excerpt_chars: usize,
) -> TurnAttachments {
    let ordered_ids = distinct_ids(document_ids);
    if ordered_ids.is_empty() {
        return TurnAttachments::default();
    }

    let mut loaded: Vec<Document> = Vec::new();
    let mut unreadable: Vec<String> = Vec::new();
    for id in &ordered_ids {
        match container.document_repository().find_by_id(id).await {
            Ok(Some(document)) => loaded.push(document),
            Ok(None) => {
                warn!(
                    document_id = id.as_str(),
                    conversation_id, "Attached document is not in the library"
                );
            }
            Err(error) => {
                warn!(
                    %error,
                    document_id = id.as_str(),
                    conversation_id,
                    "Could not load an attached document"
                );
            }
        }
    }

    let mut readable: Vec<(Document, String)> = Vec::new();
    for document in loaded {
        let text = assemble_document_text(&document);
        if text.trim().is_empty() {
            unreadable.push(document.file_name().to_string());
            continue;
        }
        readable.push((document, text));
    }

    let mut documents: Vec<AttachedDocument> = Vec::new();
    let mut remaining_budget = token_budget;
    let mut remaining_documents = readable.len();
    for (document, text) in readable {
        // An even share of what is left, so what the file before it did not
        // need is still on the table.
        let share = remaining_budget / remaining_documents.max(1);
        let carried = truncate_to_token_budget(&text, share, llm);
        remaining_documents = remaining_documents.saturating_sub(1);
        remaining_budget = remaining_budget.saturating_sub(llm.count_tokens(&carried));

        if carried.trim().is_empty() {
            // No room left at all. Naming it is still worth a line: the reader
            // can see the file arrived and the model will not deny it.
            unreadable.push(document.file_name().to_string());
            continue;
        }

        let truncated = carried.chars().count() < text.chars().count();
        documents.push(AttachedDocument {
            source: build_attachment_source(&document, carried, highlight_terms, excerpt_chars),
            truncated,
        });
    }

    let attachments = TurnAttachments {
        documents,
        unreadable,
    };

    // Recorded on the conversation so the next turn can pick the file up as
    // the document under discussion without it being attached again.
    for attached in &attachments.documents {
        if let Err(error) = conv_service
            .add_document_reference(
                conversation_id,
                attached.source.document_id.clone(),
                Some(attached.source.chunk_id.clone()),
                Some(attached.source.score),
            )
            .await
        {
            warn!(
                %error,
                conversation_id,
                document_id = attached.source.document_id.as_str(),
                "Failed to record an attachment as a conversation document reference"
            );
        }
    }

    if !attachments.is_empty() {
        info!(
            conversation_id,
            requested = ordered_ids.len(),
            carried = attachments.carried_count(),
            truncated = attachments.truncated_count(),
            unreadable = attachments.unreadable_names().len(),
            budget_tokens = token_budget,
            "Carried this turn's attachments into the prompt"
        );
    }

    attachments
}

/// The share of the turn's retrieval budget attachments may take.
pub(super) fn attachment_token_budget(available_for_rag: usize) -> usize {
    (available_for_rag as f64 * ATTACHMENT_BUDGET_SHARE) as usize
}

/// The ids a request named, blanks dropped and duplicates collapsed, in the
/// order they were attached.
fn distinct_ids(document_ids: &[String]) -> Vec<String> {
    let mut seen = HashSet::new();
    document_ids
        .iter()
        .map(|id| id.trim())
        .filter(|id| !id.is_empty())
        .filter(|id| seen.insert(id.to_string()))
        .map(|id| id.to_string())
        .collect()
}

fn build_attachment_source(
    document: &Document,
    carried_text: String,
    highlight_terms: &[String],
    excerpt_chars: usize,
) -> SourceDto {
    // The document's own first chunk, so the citation opens the file at its
    // beginning. A document with no chunks yet stands in for itself.
    let first_chunk = {
        let mut chunks: Vec<_> = document.chunks().iter().collect();
        chunks.sort_by_key(|chunk| chunk.index());
        chunks.first().map(|chunk| {
            (
                chunk.id().as_str().to_string(),
                chunk.index(),
                chunk.section().map(|section| section.to_string()),
            )
        })
    };
    let (chunk_id, chunk_index, section) = match first_chunk {
        Some((id, index, section)) => (id, Some(index), section),
        None => (document.id().as_str().to_string(), None, None),
    };

    let file_path = document.file_path().display().to_string();
    let excerpt = Some(build_excerpt(&carried_text, highlight_terms, excerpt_chars));

    SourceDto {
        document_id: document.id().as_str().to_string(),
        chunk_id,
        content: carried_text,
        // An attachment is not a search hit; it is the material itself, so it
        // sorts and reads as the strongest evidence the turn has.
        score: 1.0,
        path: if file_path.is_empty() {
            None
        } else {
            Some(file_path.clone())
        },
        position: chunk_index,
        file_name: document.file_name().to_string(),
        file_path: file_path.clone(),
        mime_type: document.mime_type().to_string(),
        category: infer_category(&file_path),
        file_size_bytes: document.size_bytes(),
        modified_at: document.modified_at().to_rfc3339(),
        excerpt,
        highlights: (!highlight_terms.is_empty()).then(|| highlight_terms.to_vec()),
        section,
        chunk_index,
        page_number: None,
        chunk_excerpts: None,
        citation_id: None,

        web_snapshot: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source(file_name: &str, chunk_id: &str, content: &str) -> SourceDto {
        SourceDto {
            document_id: format!("doc-{file_name}"),
            chunk_id: chunk_id.to_string(),
            content: content.to_string(),
            score: 1.0,
            path: None,
            position: None,
            file_name: file_name.to_string(),
            file_path: format!("/vault/{file_name}"),
            mime_type: "text/plain".to_string(),
            category: "Text File".to_string(),
            file_size_bytes: 12,
            modified_at: "2026-09-21T00:00:00Z".to_string(),
            excerpt: None,
            highlights: None,
            section: None,
            chunk_index: None,
            page_number: None,
            chunk_excerpts: None,
            citation_id: None,

            web_snapshot: None,
        }
    }

    fn attachments(documents: Vec<AttachedDocument>, unreadable: Vec<String>) -> TurnAttachments {
        TurnAttachments {
            documents,
            unreadable,
        }
    }

    #[test]
    fn ids_are_deduplicated_and_keep_their_order() {
        let ids = distinct_ids(&[
            "b".to_string(),
            "  ".to_string(),
            "a".to_string(),
            "b".to_string(),
            " a ".to_string(),
        ]);

        assert_eq!(ids, vec!["b".to_string(), "a".to_string()]);
    }

    /// The numbers in the prompt come from the turn's final source list, not
    /// from the copy this module built, so a footnote opens what it names.
    #[test]
    fn rendered_text_uses_the_number_the_source_was_finally_given() {
        let attached = AttachedDocument {
            source: source("notes.txt", "chunk-1", "the whole file"),
            truncated: false,
        };
        let mut numbered = source("notes.txt", "chunk-1", "the whole file");
        numbered.citation_id = Some(4);

        let rendered = attachments(vec![attached], Vec::new())
            .render(&[numbered])
            .expect("an attachment renders a section");

        assert!(rendered.contains("[4] Attached file: notes.txt"));
        assert!(rendered.contains("the whole file"));
        assert!(!rendered.contains("truncated here"));
    }

    /// A partial read has to look partial in the prompt.
    #[test]
    fn a_truncated_attachment_says_so() {
        let attached = AttachedDocument {
            source: source("book.txt", "chunk-1", "chapter one"),
            truncated: true,
        };

        let rendered = attachments(vec![attached], Vec::new())
            .render(&[])
            .expect("an attachment renders a section");

        assert!(rendered.contains("Attachment truncated here"));
    }

    /// The failure this whole module exists for: a file that arrived must
    /// never be reported as a file that did not.
    #[test]
    fn an_unreadable_attachment_is_still_named() {
        let rendered = attachments(Vec::new(), vec!["scan.pdf".to_string()])
            .render(&[])
            .expect("an unreadable attachment still renders a section");

        assert!(rendered.contains("scan.pdf"));
        assert!(rendered.contains("do not claim no file was attached"));
    }

    /// The five-minute import timeout: the message shows a chip, the library
    /// has nothing yet, and the answer must still admit the file exists.
    #[test]
    fn a_file_that_did_not_finish_importing_is_named_too() {
        let pending = TurnAttachments::still_importing(&["late.txt".to_string()]);

        let rendered = pending.render(&[]).expect("a pending attachment renders");

        assert!(!pending.is_empty());
        assert!(rendered.contains("late.txt"));
        assert!(pending.carried_document_ids().is_empty());
    }

    /// The rewriter has to be told what arrived, or it writes a search query
    /// about the reader's request instead of their subject.
    #[test]
    fn the_digest_carries_the_subject_not_the_request() {
        let attached = AttachedDocument {
            source: source(
                "greens.txt",
                "chunk-1",
                "kale, chard and bok choy in Chicago",
            ),
            truncated: false,
        };

        let digest = attachments(vec![attached], Vec::new())
            .subject_digest(400)
            .expect("an attachment has a subject");

        assert!(digest.contains("greens.txt"));
        assert!(digest.contains("kale, chard"));
    }

    /// Nothing readable means nothing to say about the subject; the rewriter
    /// falls back to the message rather than being handed a file name alone.
    #[test]
    fn an_unreadable_attachment_has_no_subject_digest() {
        assert!(attachments(Vec::new(), vec!["scan.pdf".to_string()])
            .subject_digest(400)
            .is_none());
    }

    #[test]
    fn nothing_attached_renders_nothing() {
        assert!(attachments(Vec::new(), Vec::new()).render(&[]).is_none());
    }

    /// Whole attachments are in the prompt already; the same paragraphs coming
    /// back from search are a second copy and a duplicate footnote.
    #[test]
    fn carried_ids_are_reported_for_retrieval_to_skip() {
        let attached = AttachedDocument {
            source: source("notes.txt", "chunk-1", "text"),
            truncated: false,
        };

        let carried = attachments(vec![attached], Vec::new()).carried_document_ids();

        assert!(carried.contains("doc-notes.txt"));
        assert_eq!(carried.len(), 1);
    }

    #[test]
    fn the_budget_leaves_room_for_the_rest_of_the_turn() {
        assert_eq!(attachment_token_budget(1000), 600);
        assert_eq!(attachment_token_budget(0), 0);
    }
}
