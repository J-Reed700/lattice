use crate::application::dtos::search_dto::SearchResultDto;
use crate::application::dtos::settings::LLMPromptSettingsDto;

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

pub(super) fn build_kb_context(budgeted_results: &[&SearchResultDto]) -> Option<String> {
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
                format!(
                    "[{}] Document: {}{}\nContent: {}",
                    i + 1,
                    doc_name,
                    doc_id_line,
                    result.content
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
