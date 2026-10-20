use lattice_core::application::contracts::search::SearchResultDto;
use lattice_core::application::contracts::settings::LLMPromptSettingsDto;

use super::SearchFlags;

pub(super) struct PromptMessageBuilder<'a> {
    prompt_settings: &'a LLMPromptSettingsDto,
    question: &'a str,
    is_greeting: bool,
    force_kb_search: bool,
    force_web_search: bool,
    /// The turn belongs to an Explorer conversation, so its context travels
    /// in `EXPLORER_TEMPLATE` rather than the saved document-answer template.
    explorer: bool,
    /// The folder an Explorer conversation reads: its top level, the open
    /// file and selection, and the rule for pointing at lines.
    explorer_context: Option<String>,
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
            explorer: false,
            explorer_context: None,
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
        }
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

    /// The Explorer folder block. It leads the context: the folder is what the
    /// user is looking at, and the library and the web are outside knowledge.
    pub(super) fn with_explorer_context(mut self, explorer_context: Option<String>) -> Self {
        self.explorer_context = explorer_context;
        self
    }

    /// A turn in an Explorer conversation, whether or not its folder block
    /// survived the cut for room.
    pub(super) fn in_explorer(mut self, explorer: bool) -> Self {
        self.explorer = explorer;
        self
    }

    /// The files attached to this message.
    ///
    /// They outrank every other section: the reader put them there, and a turn
    /// that answers "no file came through" while one sits in the library is the
    /// worst failure this prompt can produce.
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

    /// Carry the sufficiency verdict into the prompt.
    ///
    /// The verdict already decides whether to search the web and what to write
    /// in the trace, but it never reached the model. So a turn whose passages
    /// the app had judged too weak to answer from was handed to the model under
    /// the same heading, and with the same instruction to prefer them, as a
    /// turn with strong evidence — which is how a confident answer gets written
    /// off text that does not support it.
    pub(super) fn with_kb_sufficiency(
        mut self,
        verdict: Option<&super::retrieval::SufficiencyVerdict>,
    ) -> Self {
        self.thin_kb_reason = verdict
            .filter(|verdict| !verdict.sufficient)
            .map(|verdict| describe_thin_evidence(&verdict.reasons));
        self
    }

    pub(super) fn build(self) -> String {
        // Reusing the last document is the whole prompt on a bare follow-up —
        // but not when this message brought files of its own. Then the
        // attachment leads and the earlier document is one section among
        // several, because "here it is" means the new file, not the old one.
        if self.attachment_context.is_none()
            && !self.explorer
            && self.explorer_context.is_none()
            && self.prior_evidence_context.is_none()
            && self.web_context.is_none()
            && self.linked_web_sources_context.is_none()
        {
            if let Some(context_text) = self.followup_context.as_deref() {
                return render_prompt_template(
                    &self.prompt_settings.rag_prompt_template,
                    context_text,
                    self.question,
                );
            }
        }

        let has_kb_context = self.kb_context.is_some();
        let has_web_context = self.web_context.is_some();
        // Weak passages are only worth flagging when some were actually
        // retrieved. With no knowledge-base section at all, the templates below
        // already say there was nothing to go on.
        let thin_kb_reason = self.thin_kb_reason.as_deref().filter(|_| has_kb_context);
        let mut context_sections: Vec<String> = vec![CONTINUITY_NOTE.to_string()];
        if let Some(context) = self.explorer_context.as_ref() {
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
            context_sections.push(format!(
                "Evidence quality: {reason} Answer only what these passages support, \
say plainly which part of the question they do not cover, and do not fill the gap \
from general knowledge as though it came from the user's documents."
            ));
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
            if let Some(searches) = self.deep_research_searches.as_deref() {
                context_sections.push(deep_research_instruction(searches));
            }
        }
        if self.evidence_cut && context_sections.len() > 1 {
            context_sections.push(EVIDENCE_CUT_NOTE.to_string());
        }

        if context_sections.len() > 1 {
            return render_prompt_template(
                evidence_template(self.prompt_settings, self.explorer),
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
            return format!(
                "The user explicitly requested web search, but live web retrieval is currently unavailable ({web_failure_note}). \
Respond in a natural, personable tone. Give your best-effort answer from your built-in knowledge, clearly stating that live web lookup failed for this turn and citations are unavailable. \
Do not mention the knowledge base unless asked.\n\nQuestion: {question}",
                question = self.question
            );
        }

        if self.force_kb_search && !self.evidence_cut && self.kb_context.is_none() {
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
            &self.retrieval_note(),
            self.question,
        )
    }

    /// One sentence saying what happened to the user's documents this turn.
    ///
    /// Three outcomes, and they are not interchangeable. "Nothing relevant was
    /// found" describes a search that ran and came back empty; saying it about
    /// a vault with nothing in it tells the reader their documents were read
    /// and rejected, which is a claim about their documents that nobody made.
    /// The reason for an unreachable vault is already computed during
    /// retrieval — it just never reached the prompt on this path. Evidence
    /// that was found and then cut for room is a fourth outcome, and the
    /// same care applies: it is not a search that found nothing.
    fn retrieval_note(&self) -> String {
        if self.evidence_cut {
            return "Evidence was found for this turn, but the model's context window had no room \
to carry any of it. Say so, and use the document search and read tools if they are available, \
rather than describing this as a search that found nothing."
                .to_string();
        }
        if let Some(reason) = self.kb_unavailable_reason.as_deref() {
            return format!(
                "Their documents were not searched for this turn, because {reason}. Do not describe this as a search that found nothing."
            );
        }
        if self.kb_attempted {
            return "Their documents were searched for this turn and no passage came back that bears on the question."
                .to_string();
        }
        "No passages from their documents were available for this turn.".to_string()
    }
}

/// The standing instruction that opens every evidence-carrying message.
const CONTINUITY_NOTE: &str = "Conversation continuity: Answer the ongoing question using the user's established facts, \
     conversation memory, and relevant original evidence from earlier turns together with new sources. \
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

/// The message an evidence-carrying turn sends with every passage taken out:
/// the template, its standing instructions and the question.
///
/// The turn's budget charges this as the message before it sizes the
/// evidence, because it is what the evidence travels in.
pub(super) fn evidence_frame(
    prompt_settings: &LLMPromptSettingsDto,
    question: &str,
    explorer: bool,
) -> String {
    render_prompt_template(
        evidence_template(prompt_settings, explorer),
        CONTINUITY_NOTE,
        question,
    )
}

/// The template an evidence-carrying message renders in: the saved
/// document-answer template, or in an Explorer conversation the folder's own.
fn evidence_template(prompt_settings: &LLMPromptSettingsDto, explorer: bool) -> &str {
    if explorer {
        EXPLORER_TEMPLATE
    } else {
        &prompt_settings.rag_prompt_template
    }
}

/// The template an Explorer turn's context travels in.
///
/// The document-answer template asks for the answer from the context alone
/// and a citation on every statement. An Explorer turn is about the folder on
/// screen, and explaining that code draws on what no passage holds; under that
/// template the model put bracket numbers on the folder, on standing notes
/// and on its own explanations. The folder block's rules say how to point at
/// lines.
const EXPLORER_TEMPLATE: &str = "Answer the user's question about the folder open beside this chat: \
     what its code does, how it fits together, and the ideas behind it. Explain from the folder and \
     from what you know; teaching how the code works, or a concept it uses, needs no source and no \
     citation. Point at the folder's code with line references, as the folder's rules below describe. \
     Bracket numbers like [1] belong only to the numbered library or web passages in this message: put \
     one after a claim such a passage supports, and never on the folder, on these instructions or on \
     your own explanation.\n\nContext:\n{context}\n\nQuestion: {question}\n\nAnswer:";

/// What a deep-research turn is asked to do with the rounds it has left. The
/// first round searched wide before the model ran; this sends the model after
/// what that round missed, instead of answering from it or reopening one of
/// its links.
///
/// Every search the round ran is listed whole: one left off the list is one
/// the model may run again. They are short queries the round itself chose,
/// so the list is as long as that round was.
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

/// Turn sufficiency reason codes into one sentence the model can act on.
///
/// Only the reasons that actually decide the verdict are described. A missing
/// reranker or a flat spread says something about how the judgement was made,
/// not about whether the passages answer the question, and telling the model
/// its evidence is weak on that basis would be wrong.
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
    let doc_name = result.path.as_ref().unwrap_or(&result.title);
    let doc_id_line = result
        .document_id
        .as_deref()
        .map(|id| format!("\nDocument ID: {}", id))
        .unwrap_or_default();
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
        "[{}] Document: {}{}{}{}{}\nContent: {}",
        citation, doc_name, doc_id_line, page, section, ocr, result.content
    )
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
pub(super) fn enforce_numeric_citation_format(template: &str) -> String {
    format!("{template}\n\nCitation rule: Use only the supplied numeric passage labels, such as [1], [2]. Each number identifies one passage, not an entire document. Place the citation immediately after the claim it supports. The cited passage must directly support that claim; a shared keyword is not evidence. If no passage supports a claim, give it no citation; never invent one. Do not output [#] or [^1].")
}

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]
mod citation_numbering_tests;

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]
mod thin_evidence_tests;

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]
mod empty_vault_tests;

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]
mod attachment_prompt_tests;

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]
mod deep_research_prompt_tests;

#[cfg(test)]
#[cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]
mod explorer_prompt_tests;
