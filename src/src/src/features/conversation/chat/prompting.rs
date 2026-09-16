use crate::features::search::dto::SearchResultDto;
use crate::features::settings::dto::LLMPromptSettingsDto;

use super::SearchFlags;

pub(super) struct PromptMessageBuilder<'a> {
    prompt_settings: &'a LLMPromptSettingsDto,
    question: &'a str,
    is_greeting: bool,
    force_kb_search: bool,
    force_web_search: bool,
    followup_context: Option<String>,
    kb_context: Option<String>,
    linked_web_sources_context: Option<String>,
    web_context: Option<String>,
    web_search_error: Option<String>,
    kb_unavailable_reason: Option<String>,
}

impl<'a> PromptMessageBuilder<'a> {
    pub(super) fn new(
        prompt_settings: &'a LLMPromptSettingsDto,
        question: &'a str,
        is_greeting: bool,
        search_flags: SearchFlags,
    ) -> Self {
        Self {
            prompt_settings,
            question,
            is_greeting,
            force_kb_search: search_flags.force_kb_search,
            force_web_search: search_flags.force_web_search,
            followup_context: None,
            kb_context: None,
            linked_web_sources_context: None,
            web_context: None,
            web_search_error: None,
            kb_unavailable_reason: None,
        }
    }

    pub(super) fn with_followup_context(mut self, followup_context: Option<String>) -> Self {
        self.followup_context = followup_context;
        self
    }

    pub(super) fn with_kb_context(mut self, kb_context: Option<String>) -> Self {
        self.kb_context = kb_context;
        self
    }

    pub(super) fn with_linked_web_sources_context(
        mut self,
        linked_web_sources_context: Option<String>,
    ) -> Self {
        self.linked_web_sources_context = linked_web_sources_context;
        self
    }

    pub(super) fn with_web_context(mut self, web_context: Option<String>) -> Self {
        self.web_context = web_context;
        self
    }

    pub(super) fn with_web_search_error(mut self, web_search_error: Option<String>) -> Self {
        self.web_search_error = web_search_error;
        self
    }

    pub(super) fn with_kb_unavailable_reason(
        mut self,
        kb_unavailable_reason: Option<String>,
    ) -> Self {
        self.kb_unavailable_reason = kb_unavailable_reason;
        self
    }

    pub(super) fn build(self) -> String {
        if let Some(context_text) = self.followup_context {
            return render_prompt_template(
                &self.prompt_settings.rag_prompt_template,
                &context_text,
                self.question,
            );
        }

        let has_kb_context = self.kb_context.is_some();
        let has_web_context = self.web_context.is_some();
        let mut context_sections: Vec<String> = Vec::new();
        if has_kb_context && has_web_context {
            context_sections.push(
                "Priority rule: Prefer Knowledge Base Results first. Use Web Results only to supplement missing details.".to_string(),
            );
        }
        if let Some(context_text) = self.kb_context.as_ref() {
            context_sections.push(format!("Knowledge Base Results:\n{}", context_text));
        }
        if let Some(context_text) = self.linked_web_sources_context.as_ref() {
            context_sections.push(format!(
                "Linked Conversation Web Sources:\n{}",
                context_text
            ));
        }
        if let Some(context_text) = self.web_context.as_ref() {
            context_sections.push(format!("Web Results (Supplemental):\n{}", context_text));
        }

        if !context_sections.is_empty() {
            return render_prompt_template(
                &self.prompt_settings.rag_prompt_template,
                &context_sections.join("\n\n"),
                self.question,
            );
        }

        if self.is_greeting {
            return render_prompt_template(
                &self.prompt_settings.greeting_prompt_template,
                "",
                self.question,
            );
        }

        if self.force_web_search && self.web_context.is_none() && self.kb_context.is_none() {
            let web_failure_note = self
                .web_search_error
                .as_deref()
                .unwrap_or("No web results were returned");
            return format!(
                "The user explicitly requested web search, but live web retrieval is currently unavailable ({web_failure_note}). \
Respond in a natural, personable tone. Give your best-effort answer from your built-in knowledge, clearly stating that live web lookup failed for this turn and citations are unavailable. \
Do not mention the knowledge base unless asked.\n\nQuestion: {question}",
                question = self.question
            );
        }

        if self.force_kb_search && self.kb_context.is_none() {
            let kb_reason = self
                .kb_unavailable_reason
                .as_deref()
                .unwrap_or("no matching knowledge base documents were found");
            return format!(
                "The user explicitly requested a knowledge base search, but it could not be fulfilled because {kb_reason}. \
Respond conversationally, explain this clearly in one sentence, and offer a concrete next step (for example: upload documents, assign docs to the current space, or switch scope).\n\nQuestion: {question}",
                kb_reason = kb_reason,
                question = self.question,
            );
        }

        render_prompt_template(
            &self.prompt_settings.no_context_prompt_template,
            "",
            self.question,
        )
    }
}

/// Render the retrieved knowledge-base chunks for the prompt.
///
/// `citation_ids` maps a document id to the number that document occupies in
/// the source list the UI will render. Chunks are labelled with **that**
/// number rather than their position in this list.
///
/// The two used to be numbered independently: this function numbered the
/// budgeted per-chunk list `[1..k]`, while the UI resolved `[n]` by position
/// in a per-document, deduplicated, score-sorted list. So as soon as one
/// document contributed two chunks — or budget-trimming dropped any — the
/// footnotes pointed at the wrong documents. Citations are the trust
/// primitive of a document-chat app; they have to resolve to the document the
/// model was actually shown.
///
/// Several chunks from one document now share that document's number, which
/// is correct: the footnote identifies the source, not the chunk.
pub(super) fn build_kb_context(
    budgeted_results: &[&SearchResultDto],
    citation_ids: &std::collections::HashMap<String, u32>,
) -> Option<String> {
    if budgeted_results.is_empty() {
        return None;
    }

    Some(
        budgeted_results
            .iter()
            .enumerate()
            .map(|(i, result)| {
                let doc_name = result.path.as_ref().unwrap_or(&result.title);
                let doc_id_line = result
                    .document_id
                    .as_deref()
                    .map(|id| format!("\nDocument ID: {}", id))
                    .unwrap_or_default();

                // Fall back to positional numbering only when a chunk's
                // document isn't in the source list at all, which would mean
                // the UI has nothing to resolve against either.
                let citation = result
                    .document_id
                    .as_deref()
                    .and_then(|id| citation_ids.get(id).copied())
                    .unwrap_or((i + 1) as u32);

                format!(
                    "[{}] Document: {}{}\nContent: {}",
                    citation, doc_name, doc_id_line, result.content
                )
            })
            .collect::<Vec<_>>()
            .join("\n\n"),
    )
}

pub(super) fn render_prompt_template(template: &str, context: &str, question: &str) -> String {
    template
        .replace("{context}", context)
        .replace("{question}", question)
}

pub(super) fn enforce_numeric_citation_format(template: &str) -> String {
    if template.contains("[#]") {
        format!(
            "{template}\n\nCitation rule: Use numeric citations like [1], [2], [3]. Do not output [#] or footnote syntax such as [^1]."
        )
    } else {
        template.to_string()
    }
}

pub(super) fn render_tool_followup_prompt(
    template: &str,
    question: &str,
    previous_response: &str,
) -> String {
    let rendered = template.replace("{question}", question);
    if previous_response.trim().is_empty() {
        rendered.replace("{previous_response}", "")
    } else {
        rendered.replace("{previous_response}", previous_response)
    }
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]
mod citation_numbering_tests {
    use super::*;
    use crate::features::qa::dto::SourceDto;
    use std::collections::HashMap;

    fn chunk(id: &str, document_id: &str, title: &str, content: &str) -> SearchResultDto {
        SearchResultDto {
            id: id.to_string(),
            title: title.to_string(),
            content: content.to_string(),
            score: 0.9,
            path: Some(title.to_string()),
            document_id: Some(document_id.to_string()),
            position: None,
            vector_score: None,
            bm25_score: None,
            vector_rank: None,
            bm25_rank: None,
            metadata: HashMap::new(),
        }
    }

    fn source(document_id: &str, file_name: &str, citation_id: u32) -> SourceDto {
        SourceDto {
            document_id: document_id.to_string(),
            chunk_id: format!("{}-chunk", document_id),
            content: String::new(),
            score: 0.9,
            path: Some(file_name.to_string()),
            position: None,
            file_name: file_name.to_string(),
            file_path: file_name.to_string(),
            mime_type: "text/plain".to_string(),
            category: "Text".to_string(),
            file_size_bytes: 0,
            modified_at: String::new(),
            excerpt: None,
            highlights: None,
            section: None,
            chunk_index: None,
            chunk_excerpts: None,
            citation_id: Some(citation_id),
        }
    }

    /// The exact scenario from the audit: one document contributes three
    /// chunks and a second contributes one. The prompt used to number the
    /// four *chunks* [1][2][3][4] while the UI resolved [n] against a
    /// two-entry, per-document list — so [3] and [4] pointed at nothing
    /// sensible and [2] opened the wrong file.
    #[test]
    fn chunks_from_one_document_share_that_document_s_citation_number() {
        let results = [
            chunk("c1", "doc-a", "alpha.md", "first chunk of A"),
            chunk("c2", "doc-a", "alpha.md", "second chunk of A"),
            chunk("c3", "doc-a", "alpha.md", "third chunk of A"),
            chunk("c4", "doc-b", "beta.md", "only chunk of B"),
        ];
        let refs: Vec<&SearchResultDto> = results.iter().collect();

        // The source list the UI will render, numbered once.
        let sources = vec![
            source("doc-a", "alpha.md", 1),
            source("doc-b", "beta.md", 2),
        ];
        let ids =
            crate::features::conversation::chat::retrieval::citation_ids_by_document(&sources);

        let context = build_kb_context(&refs, &ids).expect("context");

        // Every chunk of doc-a is labelled [1]; doc-b's is [2].
        assert_eq!(context.matches("[1] Document: alpha.md").count(), 3);
        assert_eq!(context.matches("[2] Document: beta.md").count(), 1);

        // And crucially, no number is emitted that the UI cannot resolve.
        assert!(
            !context.contains("[3]") && !context.contains("[4]"),
            "prompt must not cite numbers absent from the source list:\n{}",
            context
        );
    }

    /// Budget trimming drops chunks from the prompt. The surviving chunks
    /// must keep their document's number rather than being renumbered from 1.
    #[test]
    fn budget_trimming_does_not_renumber_surviving_chunks() {
        // doc-a was dropped by the token budget; only doc-b's chunk survives.
        let results = [chunk("c4", "doc-b", "beta.md", "only chunk of B")];
        let refs: Vec<&SearchResultDto> = results.iter().collect();

        let sources = vec![
            source("doc-a", "alpha.md", 1),
            source("doc-b", "beta.md", 2),
        ];
        let ids =
            crate::features::conversation::chat::retrieval::citation_ids_by_document(&sources);

        let context = build_kb_context(&refs, &ids).expect("context");

        assert!(
            context.contains("[2] Document: beta.md"),
            "surviving chunk must keep its document's number, got:\n{}",
            context
        );
        assert!(
            !context.contains("[1]"),
            "a trimmed prompt must not renumber from 1:\n{}",
            context
        );
    }

    #[test]
    fn falls_back_to_position_when_a_document_is_absent_from_the_source_list() {
        let results = [chunk("c1", "doc-unknown", "orphan.md", "content")];
        let refs: Vec<&SearchResultDto> = results.iter().collect();
        let ids: HashMap<String, u32> = HashMap::new();

        let context = build_kb_context(&refs, &ids).expect("context");
        assert!(context.contains("[1] Document: orphan.md"));
    }
}
