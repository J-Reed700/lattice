use std::borrow::Cow;

use lattice_core::application::contracts::search::SearchResultDto;
use lattice_core::application::contracts::settings::{
    LLMPromptSettingsDto, DEFAULT_HYBRID_RAG_PROMPT_TEMPLATE, DEFAULT_RAG_PROMPT_TEMPLATE,
};
use lattice_core::application::ports::turn_source::TurnSourcePolicy;

use super::retrieval::source_names::marker;
use super::SearchFlags;

pub(super) struct PromptMessageBuilder<'a> {
    prompt_settings: &'a LLMPromptSettingsDto,
    question: &'a str,
    is_greeting: bool,
    user_requested_kb: bool,
    strict_sources: bool,
    force_web_search: bool,
    /// The message is a whole prompt the app wrote, with its own material and
    /// rules, so no template frames it.
    closed_book: bool,
    /// How the turn treats the source its conversation reads beside the chat;
    /// with a template of its own, the context travels in it rather than the
    /// saved document-answer template.
    source: Option<TurnSourcePolicy>,
    /// What the source shows for the turn, such as an Explorer folder's top
    /// level, open file and selection, and the rule for pointing at lines.
    source_context: Option<String>,
    /// The files the reader attached to this message, already carried whole.
    attachment_context: Option<String>,
    followup_context: Option<String>,
    prior_evidence_context: Option<String>,
    kb_context: Option<String>,
    linked_web_sources_context: Option<String>,
    web_context: Option<String>,
    web_search_error: Option<String>,
    kb_unavailable_reason: Option<String>,
    /// Whether the vault was actually searched. Carried so a turn with no
    /// passages can say which of the two things happened.
    kb_attempted: bool,
    /// Why the retrieved passages were judged too weak to answer from, when
    /// they were. `None` means the evidence passed the sufficiency check, or
    /// that no check ran.
    thin_kb_reason: Option<String>,
    /// Set on a deep-research turn that may search again: the searches its
    /// first round already ran.
    deep_research_searches: Option<Vec<String>>,
    /// Some of the evidence the turn found was left out to fit the window.
    evidence_cut: bool,
    /// Whether this turn actually offers document lookup/read tools
    /// (`get_document`, `read_more`, `semantic_search`). Default templates omit
    /// "do not answer yet: call ..." directives when false.
    document_tools_available: bool,
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
            user_requested_kb: search_flags.user_requested_kb,
            strict_sources: search_flags.strict_sources,
            force_web_search: search_flags.force_web_search,
            closed_book: search_flags.closed_book,
            source: None,
            source_context: None,
            attachment_context: None,
            followup_context: None,
            prior_evidence_context: None,
            kb_context: None,
            linked_web_sources_context: None,
            web_context: None,
            web_search_error: None,
            kb_unavailable_reason: None,
            kb_attempted: false,
            thin_kb_reason: None,
            deep_research_searches: None,
            evidence_cut: false,
            document_tools_available: true,
        }
    }

    pub(super) fn with_document_tools(mut self, available: bool) -> Self {
        self.document_tools_available = available;
        self
    }

    /// Say that evidence the turn found was left out because the model's
    /// window could not hold it, so the model neither reports a search that
    /// found nothing nor takes what is left for everything there is.
    pub(super) fn with_evidence_cut(mut self, evidence_cut: bool) -> Self {
        self.evidence_cut = evidence_cut;
        self
    }

    /// A deep-research turn that may search again after its first round,
    /// with the searches that round ran.
    pub(super) fn with_deep_research_searches(mut self, searches: Option<Vec<String>>) -> Self {
        self.deep_research_searches = searches;
        self
    }

    /// The source's block. It leads the context: the source is what the user
    /// is looking at, and the library and the web are outside knowledge.
    pub(super) fn with_source_context(mut self, source_context: Option<String>) -> Self {
        self.source_context = source_context;
        self
    }

    /// A turn whose conversation reads a source beside the chat, whether or
    /// not the source's block survived the cut for room.
    pub(super) fn for_source(mut self, source: Option<TurnSourcePolicy>) -> Self {
        self.source = source;
        self
    }

    /// The files attached to this message. They outrank every other section: the
    /// reader put them there.
    pub(super) fn with_attachment_context(mut self, attachment_context: Option<String>) -> Self {
        self.attachment_context = attachment_context;
        self
    }

    pub(super) fn with_prior_evidence_context(mut self, context: Option<String>) -> Self {
        self.prior_evidence_context = context;
        self
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

    pub(super) fn with_kb_attempted(mut self, kb_attempted: bool) -> Self {
        self.kb_attempted = kb_attempted;
        self
    }

    /// Carries the sufficiency verdict into the prompt, so passages judged too
    /// weak are not presented like strong evidence.
    pub(super) fn with_kb_sufficiency(
        mut self,
        verdict: Option<&super::retrieval::SufficiencyVerdict>,
    ) -> Self {
        self.thin_kb_reason = verdict
            .filter(|verdict| !verdict.sufficient)
            .map(|verdict| describe_thin_evidence(&verdict.reasons));
        self
    }

    /// The message rendered for the turn.
    pub(super) fn build(self) -> String {
        self.message()
    }

    fn message(self) -> String {
        // Reusing the last document is the whole prompt on a bare follow-up —
        // but not when this message brought files of its own. Then the
        // attachment leads and the earlier document is one section among
        // several, because "here it is" means the new file, not the old one.
        if self.attachment_context.is_none()
            && self.source.is_none()
            && self.source_context.is_none()
            && self.prior_evidence_context.is_none()
            && self.web_context.is_none()
            && self.linked_web_sources_context.is_none()
        {
            if let Some(context_text) = self.followup_context.as_deref() {
                let template = resolved_evidence_template(
                    self.prompt_settings,
                    self.source,
                    self.strict_sources,
                    self.document_tools_available,
                );
                return render_prompt_template(template.as_ref(), context_text, self.question);
            }
        }

        let has_kb_context = self.kb_context.is_some();
        let has_web_context = self.web_context.is_some();
        // Weak passages are only worth flagging when some were actually
        // retrieved. With no knowledge-base section at all, the templates below
        // already say there was nothing to go on.
        let thin_kb_reason = self.thin_kb_reason.as_deref().filter(|_| has_kb_context);
        let mut context_sections: Vec<String> = vec![CONTINUITY_NOTE.to_string()];
        if let Some(context) = self.source_context.as_ref() {
            context_sections.push(context.clone());
        }
        if let Some(context) = self.prior_evidence_context.as_ref() {
            context_sections.push(context.clone());
        }
        if let Some(context_text) = self.attachment_context.as_ref() {
            context_sections.push(context_text.clone());
        }
        if let Some(context_text) = self.followup_context.as_ref() {
            context_sections.push(format!(
                "Document from earlier in this conversation:\n{}",
                context_text
            ));
        }
        if has_kb_context && has_web_context {
            // Preferring the knowledge base is the right default, but not when
            // the app has already judged that its passages do not answer the
            // question. Insisting on them then ranks known-weak evidence above
            // sources that may be better.
            context_sections.push(if thin_kb_reason.is_some() {
                "Priority rule: The Knowledge Base Results below were judged weak for this question. Weigh them against the Web Results on their merits rather than preferring them.".to_string()
            } else {
                "Priority rule: Prefer Knowledge Base Results first. Use Web Results only to supplement missing details.".to_string()
            });
        }
        if let Some(reason) = thin_kb_reason {
            context_sections.push(if self.strict_sources {
                format!(
                    "Evidence quality: {reason} Answer only what these passages support, \
say plainly which part of the question they do not cover, and do not fill the gap \
from general knowledge as though it came from the user's documents."
                )
            } else {
                format!(
                    "Evidence quality: {reason} Cite only what these passages genuinely support. \
Do not pretend the documents cover the rest; for general or conceptual parts, answer naturally \
from general knowledge without citations, and abstain only on private details specific to the \
user's own files."
                )
            });
        }
        if let Some(context_text) = self.kb_context.as_ref() {
            context_sections.push(EVIDENCE_SYNTHESIS_NOTE.to_string());
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
            if let Some(searches) = self.deep_research_searches.as_deref() {
                context_sections.push(deep_research_instruction(searches));
            }
        }
        if self.evidence_cut && context_sections.len() > 1 {
            context_sections.push(
                if self.document_tools_available {
                    EVIDENCE_CUT_NOTE
                } else {
                    EVIDENCE_CUT_NO_TOOLS_NOTE
                }
                .to_string(),
            );
        }

        if context_sections.len() > 1 {
            let template = resolved_evidence_template(
                self.prompt_settings,
                self.source,
                self.strict_sources,
                self.document_tools_available,
            );
            return render_prompt_template(
                template.as_ref(),
                &context_sections.join("\n\n"),
                self.question,
            );
        }

        if self.closed_book {
            return self.question.to_string();
        }

        if self.is_greeting {
            return render_prompt_template(
                &self.prompt_settings.greeting_prompt_template,
                "",
                self.question,
            );
        }

        // Evidence that was found and then cut for room is not a failed
        // search, so the two notes below do not describe it.
        if self.force_web_search
            && !self.evidence_cut
            && self.web_context.is_none()
            && self.kb_context.is_none()
        {
            let web_failure_note = self
                .web_search_error
                .as_deref()
                .unwrap_or("No web results were returned");
            if self.strict_sources {
                return format!(
                    "The user explicitly requested web search with Sources-only mode on, but live web retrieval failed ({web_failure_note}) and no supporting passages were available. \
Do not answer from general knowledge. Explain this clearly in one sentence and offer a concrete next step \
(for example: retry the web search, add documents, or turn off Sources only).\n\nQuestion: {question}",
                    question = self.question
                );
            }
            return format!(
                "The user explicitly requested web search, but live web retrieval is currently unavailable ({web_failure_note}). \
Respond in a natural, personable tone. Give your best-effort answer from your built-in knowledge, clearly stating that live web lookup failed for this turn and citations are unavailable. \
Do not mention the knowledge base unless asked.\n\nQuestion: {question}",
                question = self.question
            );
        }

        if self.strict_sources && self.evidence_cut {
            let tool_hint = if self.document_tools_available {
                " Use the document search and read tools if they are available before concluding evidence is missing."
            } else {
                ""
            };
            return format!(
                "Sources-only mode is on, and evidence was found for this turn, but the model's context window had no room to carry any of it.{tool_hint} \
Do not answer from general knowledge.\n\nQuestion: {question}",
                question = self.question
            );
        }

        if self.strict_sources && !self.evidence_cut && self.kb_context.is_none() {
            let kb_reason = self
                .kb_unavailable_reason
                .as_deref()
                .unwrap_or("no matching knowledge base documents were found");
            let scope_hint = if self.user_requested_kb {
                "The user explicitly requested a source-only answer from their documents"
            } else {
                "Sources-only mode is on, so this answer must come strictly from retrieved sources"
            };
            return format!(
                "{scope_hint}, but no supporting passages were available because {kb_reason}. \
Do not answer from general knowledge. Explain this clearly in one sentence and offer a concrete next step \
(for example: add documents, adjust the conversation's space or focus, or turn off Sources only).\n\nQuestion: {question}",
                scope_hint = scope_hint,
                kb_reason = kb_reason,
                question = self.question,
            );
        }

        render_prompt_template(
            &self.prompt_settings.no_context_prompt_template,
            &self.retrieval_note(),
            self.question,
        )
    }

    /// What happened to the user's documents this turn, in a sentence or two. A
    /// search that found nothing, a vault with nothing in it, an unreachable vault
    /// and evidence cut for room are different outcomes and must not be conflated.
    fn retrieval_note(&self) -> String {
        if self.evidence_cut {
            return if self.document_tools_available {
                "Evidence was found for this turn, but the model's context window had no room \
to carry any of it. Say so, and use the document search and read tools if they are available, \
rather than describing this as a search that found nothing."
            } else {
                "Evidence was found for this turn, but the model's context window had no room \
to carry any of it. Say so rather than describing this as a search that found nothing."
            }
            .to_string();
        }
        let outcome = if let Some(reason) = self.kb_unavailable_reason.as_deref() {
            format!(
                "Their documents were not searched for this turn, because {reason}. Do not describe this as a search that found nothing."
            )
        } else if self.kb_attempted {
            "Their documents were searched for this turn and no passage came back that bears on the question."
                .to_string()
        } else {
            "No passages from their documents were available for this turn.".to_string()
        };
        if self.user_requested_kb {
            format!("{outcome} {REQUESTED_DOCUMENTS_NOT_BACKING_NOTE}")
        } else {
            outcome
        }
    }
}

/// Closes the outcome of a turn whose user asked for the answer to come from
/// their documents when no passage backs it.
const REQUESTED_DOCUMENTS_NOT_BACKING_NOTE: &str =
    "The user asked for this answer to come from their documents, so say plainly that it is \
not backed by their documents, and offer a concrete next step.";

/// The standing instruction that opens every evidence-carrying message. It
/// names the latest message as the one to answer; "the ongoing question" makes
/// small models answer the earlier one again.
const CONTINUITY_NOTE: &str = "Conversation continuity: Answer the user's latest message, the question \
     this prompt asks. When it continues an earlier topic, use the user's established facts, \
     conversation memory, and relevant original evidence from earlier turns together with new sources; \
     when it asks about something else, leave the earlier topic and its evidence out. \
     New search results supplement that evidence; they do not replace it. Ignore unrelated results. \
     Before declaring information unavailable, consult retained evidence and available history/source \
     tools. Describe only the specific remaining gap, not a reset of knowledge for this search round. \
     Earlier assistant claims alone are not source evidence; use the numbered original passages \
     supplied now, or retrieve the original source.";

/// Closes the evidence of a message whose lowest-ranked evidence was cut to
/// fit the model's window.
const EVIDENCE_CUT_NOTE: &str =
    "Context window: Lower-ranked evidence found for this turn was left \
out because the model's context window could not hold it. If what is here does not settle the \
question, use the document search and read tools before concluding the evidence is missing.";

const EVIDENCE_CUT_NO_TOOLS_NOTE: &str =
    "Context window: Lower-ranked evidence found for this turn was left \
out because the model's context window could not hold it.";

/// Kept beside the passages rather than only in the system prompt so smaller
/// models apply it while reading the evidence. This is deliberately about the
/// shape of evidence, not any domain or fixture: a practical question often
/// names an entity in one document and finds the applicable rule in another.
const EVIDENCE_SYNTHESIS_NOTE: &str =
    "Evidence synthesis: Answer the user's intent, not only literal phrase matches. Join facts \
across passages when one passage identifies the relevant entity or document and another states \
the applicable rule; cite every link. If the user's wording is broader than the source text, give \
the closest supported mechanism and clearly state its narrower scope or condition. Different \
terminology alone is not missing evidence, and a narrower mechanism must not be relabeled as a \
broader one.";

/// The message an evidence-carrying turn sends with every passage taken out:
/// the template, its standing instructions, and the question.
///
/// The turn's budget charges this as the message before it sizes the
/// evidence, because it is what the evidence travels in.
#[cfg_attr(not(test), allow(dead_code))]
pub(super) fn evidence_frame(
    prompt_settings: &LLMPromptSettingsDto,
    question: &str,
    source: Option<TurnSourcePolicy>,
    strict_sources: bool,
) -> String {
    evidence_frame_with_tools(prompt_settings, question, source, strict_sources, true)
}

pub(super) fn evidence_frame_with_tools(
    prompt_settings: &LLMPromptSettingsDto,
    question: &str,
    source: Option<TurnSourcePolicy>,
    strict_sources: bool,
    document_tools_available: bool,
) -> String {
    let template = resolved_evidence_template(
        prompt_settings,
        source,
        strict_sources,
        document_tools_available,
    );
    render_prompt_template(template.as_ref(), CONTINUITY_NOTE, question)
}

const STRICT_TOOL_DIRECTIVE: &str = "When the supplied excerpts already answer the question, do not call tools. If the excerpts leave any part of the question unanswered — including when excerpts from a matching document omit the section or detail asked about — do not answer yet: call get_document (passing only the bracketed source label, such as [s1], never a filename), read_more (passing the bracketed continuation handle), or semantic_search first. ";
const HYBRID_TOOL_DIRECTIVE: &str = "When the supplied excerpts already answer the question, do not call tools. If the excerpts are incomplete for a question about the user's documents — including when excerpts from a matching document omit the section or detail asked about — do not answer yet: call get_document (passing only the bracketed source label, such as [s1], never a filename), read_more (passing the bracketed continuation handle), or semantic_search first. ";
const CLOSING_TOOL_DIRECTIVE: &str =
    ", or call a document tool first if a needed section or detail is missing from the excerpts.";

/// The template an evidence-carrying message renders in: the source's own,
/// such as an Explorer folder's; either built-in default template switched
/// between strict (`DEFAULT_RAG_PROMPT_TEMPLATE`) and hybrid
/// (`DEFAULT_HYBRID_RAG_PROMPT_TEMPLATE`) based on `strict_sources` (stripping
/// document-tool call orders when no document tools are offered); or else the
/// user's custom `rag_prompt_template` verbatim.
fn resolved_evidence_template<'a>(
    prompt_settings: &'a LLMPromptSettingsDto,
    source: Option<TurnSourcePolicy>,
    strict_sources: bool,
    document_tools_available: bool,
) -> Cow<'a, str> {
    if let Some(template) = source.and_then(|source| source.evidence_template) {
        return Cow::Borrowed(template);
    }
    if prompt_settings.rag_prompt_template == DEFAULT_HYBRID_RAG_PROMPT_TEMPLATE
        || prompt_settings.rag_prompt_template == DEFAULT_RAG_PROMPT_TEMPLATE
    {
        let base = if strict_sources {
            DEFAULT_RAG_PROMPT_TEMPLATE
        } else {
            DEFAULT_HYBRID_RAG_PROMPT_TEMPLATE
        };
        if document_tools_available {
            return Cow::Borrowed(base);
        }
        let adapted = base
            .replace(STRICT_TOOL_DIRECTIVE, "")
            .replace(HYBRID_TOOL_DIRECTIVE, "")
            .replace(CLOSING_TOOL_DIRECTIVE, ".");
        return Cow::Owned(adapted);
    }
    Cow::Borrowed(&prompt_settings.rag_prompt_template)
}

/// What a deep-research turn is asked to do with the rounds it has left: go
/// after what the first, wide round missed. Every search that round ran is
/// listed whole, so the model does not run one again.
fn deep_research_instruction(searches: &[String]) -> String {
    let mut seen = std::collections::HashSet::new();
    let listed: Vec<String> = searches
        .iter()
        .map(|search| search.trim())
        .filter(|search| !search.is_empty() && seen.insert(search.to_lowercase()))
        .map(|search| format!("\"{search}\""))
        .collect();
    let already = if listed.is_empty() {
        String::new()
    } else {
        format!(" Searches already run: {}.", listed.join("; "))
    };
    format!(
        "Deep research: The Web Results above are the first round of this research, not all of it.{already} \
Before you answer, work out which parts of the question they leave uncovered, thin or out of date. \
For each gap, call web_search with a short new query aimed at it, then fetch_url_content on the results \
that look most useful; when there are several gaps, make several calls in one reply. Do not repeat a \
search listed here or reopen a page that was already read. Once the question is covered, or new \
searches stop turning up anything new, stop and answer from everything you have gathered, saying what \
is still uncertain."
    )
}

/// Turns sufficiency reason codes into one sentence the model can act on. Only
/// the reasons that decide the verdict are described; a missing reranker or a
/// flat spread says nothing about whether the passages answer the question.
fn describe_thin_evidence(reasons: &[&'static str]) -> String {
    use super::retrieval::sufficiency_reason as reason;

    let mut parts: Vec<&str> = Vec::new();
    if reasons.contains(&reason::LOW_TOP_SCORE) {
        parts.push("nothing in the user's documents scored as clearly relevant");
    }
    if reasons.contains(&reason::LOW_TERM_COVERAGE) {
        parts.push("the passages below do not mention much of what was asked about");
    }
    if reasons.contains(&reason::TOO_FEW_RESULTS) {
        parts.push("only a couple of passages matched at all");
    }

    let Some(last) = parts.pop() else {
        return "the retrieved passages were judged too weak to answer from.".to_string();
    };
    if parts.is_empty() {
        format!("{last}.")
    } else {
        format!("{}, and {last}.", parts.join(", "))
    }
}

/// Render the retrieved knowledge-base chunks for the prompt.
///
/// Each number identifies one evidence passage, even when several passages
/// come from the same document. Never invent a number for missing evidence.
/// The turn renders them through [`kb_passages`], one item each, so it can
/// cut them one at a time; this is the section they make whole.
#[cfg(test)]
pub(super) fn build_kb_context(
    budgeted_results: &[&SearchResultDto],
    citation_ids: &std::collections::HashMap<String, u32>,
) -> Option<String> {
    if budgeted_results.is_empty() {
        return None;
    }

    Some(
        kb_passages(budgeted_results, citation_ids)
            .into_iter()
            .map(|(_, passage)| passage)
            .collect::<Vec<_>>()
            .join("\n\n"),
    )
}

/// Each retrieved passage as the prompt shows it, beside its citation number,
/// best first. A passage with no number is left out.
pub(super) fn kb_passages(
    budgeted_results: &[&SearchResultDto],
    citation_ids: &std::collections::HashMap<String, u32>,
) -> Vec<(u32, String)> {
    budgeted_results
        .iter()
        .filter_map(|result| {
            let citation = *citation_ids.get(&result.id)?;
            Some((citation, kb_passage(result, citation)))
        })
        .collect()
}

/// One retrieved passage under its citation number.
pub(super) fn kb_passage(result: &SearchResultDto, citation: u32) -> String {
    let doc_name = document_name(result);
    let page = result
        .metadata
        .get("pageNumber")
        .and_then(|v| v.as_u64())
        .map(|p| format!("\nPhysical PDF page: {p}"))
        .unwrap_or_default();
    let section = result
        .metadata
        .get("section")
        .and_then(|v| v.as_str())
        .map(|s| format!("\nSource heading: {s}"))
        .unwrap_or_default();
    let ocr = match result.metadata.get("ocr").and_then(|v| v.as_bool()) {
        Some(true) => "\nText: OCR of a scanned page; odd characters may be recognition errors",
        _ => "",
    };
    format!(
        "{} Document: {}{}{}{}\nContent: {}",
        marker(citation),
        doc_name,
        page,
        section,
        ocr,
        result.content
    )
}

/// The name every passage shows its document under: the title, or the file
/// name when there is none, never the path on disk.
fn document_name(result: &SearchResultDto) -> &str {
    if !result.title.trim().is_empty() {
        return &result.title;
    }
    result
        .path
        .as_deref()
        .and_then(|path| path.rsplit(['/', '\\']).find(|part| !part.is_empty()))
        .unwrap_or_default()
}

pub(super) fn render_prompt_template(template: &str, context: &str, question: &str) -> String {
    template
        .replace("{context}", context)
        .replace("{question}", question)
}

/// How to write a citation. Whether an unsourced claim belongs in the answer
/// at all is the turn's template to say: the document-answer template asks
/// for the context alone, while an Explorer turn may explain from general
/// knowledge without flagging each sentence.
pub(super) fn enforce_citation_format(template: &str) -> String {
    format!("{template}\n\nCitation rule: Use only the supplied passage labels, such as [s1], [s2]. The reader sees a label such as [s12] as 12. Each label identifies one passage, not an entire document. Place the citation immediately after the claim it supports. The cited passage must directly support that claim; a shared keyword is not evidence. If no passage supports a claim, give it no citation; never invent one. Do not output [#] or [^1].")
}

#[cfg(test)]
mod citation_numbering_tests;

#[cfg(test)]
mod thin_evidence_tests;

#[cfg(test)]
mod empty_vault_tests;

#[cfg(test)]
mod attachment_prompt_tests;

#[cfg(test)]
mod deep_research_prompt_tests;

#[cfg(test)]
mod source_prompt_tests;
