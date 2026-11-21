use super::prompting::enforce_citation_format;
pub(super) use lattice_core::application::contracts::settings::LLMPromptSettingsDto;
use lattice_core::application::contracts::settings::{
    DEFAULT_HYBRID_RAG_PROMPT_TEMPLATE, DEFAULT_NO_CONTEXT_PROMPT_TEMPLATE,
    DEFAULT_RAG_PROMPT_TEMPLATE,
};

/// The default that asked the model to copy a document id and an offset.
const ID_AND_OFFSET_RAG_TEMPLATE: &str = "Answer the user's question using only the provided context. Cite every factual statement supported by the context using numeric brackets like [1], [2], [3]. If the excerpts are insufficient, use available document search/read tools before concluding that evidence is missing. If still unsupported, say it was not found in the excerpts searched and do not guess or claim the entire collection lacks it. Do not cite unrelated context. Do not cite a source that does not support the associated statement. If you need to call get_document, use the exact Document ID shown in the context. For long documents, read on with the offset parameter each result names.\n\nContext:\n{context}\n\nQuestion: {question}\n\nAnswer:";

/// The default fresh installs were given before the search-first one.
const PREVIOUS_DEFAULT_RAG_TEMPLATE: &str = "Answer the user's question using only the provided context. Cite every factual statement supported by the context using numeric brackets like [1], [2], [3]. If the context does not contain the answer, say that the answer is not available in the provided documents and do not guess. When the answer is unavailable, respond concisely without summarizing or citing unrelated context. Do not cite a source that does not support the associated statement. If you need to call get_document, use the exact Document ID shown in the context. For long documents, read on with the offset parameter each result names.\n\nContext:\n{context}\n\nQuestion: {question}\n\nAnswer:";

/// The default that asked for numeric brackets, before a source was a named thing.
const NUMERIC_BRACKETS_RAG_TEMPLATE: &str = "Answer the user's question using only the provided context. Cite every factual statement supported by the context using numeric brackets like [1], [2], [3]. If the excerpts are insufficient, use available document search/read tools before concluding that evidence is missing. If still unsupported, say it was not found in the excerpts searched and do not guess or claim the entire collection lacks it. Do not cite unrelated context. Do not cite a source that does not support the associated statement. If you need to call get_document, name the document by its bracketed number in the context. For long documents, read on with read_more and the name each result gives.\n\nContext:\n{context}\n\nQuestion: {question}\n\nAnswer:";

/// The default system prompt that asked for numeric brackets.
const NUMERIC_BRACKETS_SYSTEM_PROMPT: &str = "You are Lattice, a precise research assistant. Use the user's documents when available. Cite sources using numeric brackets like [1], [2], [3]. Never fabricate document IDs.";

/// The strict default before rules moved after the question.
const PREVIOUS_STRICT_RAG_TEMPLATE: &str = "Answer the user's question using only the provided context. Cite every factual statement supported by the context using bracketed labels like [s1], [s2], [s3]. If the excerpts are insufficient, use available document search/read tools before concluding that evidence is missing. If still unsupported, say it was not found in the excerpts searched and do not guess or claim the entire collection lacks it. Do not cite unrelated context. Do not cite a source that does not support the associated statement. If you need to call get_document, name the document by its bracketed name in the context. For long documents, read on with read_more and the name each result gives.\n\nContext:\n{context}\n\nQuestion: {question}\n\nAnswer:";

/// The hybrid default before rules moved after the question.
const PREVIOUS_HYBRID_RAG_TEMPLATE: &str = "Answer the user's question using the provided context first, citing every factual statement supported by a passage using bracketed labels like [s1], [s2], [s3]. If the excerpts are incomplete for a question about the user's documents, use available document search/read tools first. When the passages do not cover a general, conceptual, or public-knowledge question (or only cover part of it), answer naturally from general knowledge without adding citations to uncited claims. Never fabricate citations, never present general knowledge as coming from their documents, and never guess private facts about the user's own files, notes, policies, or projects that are not in the passages. Do not cite unrelated context or a source that does not support the associated statement. If you need to call get_document, name the document by its bracketed name in the context. For long documents, read on with read_more and the name each result gives.\n\nContext:\n{context}\n\nQuestion: {question}\n\nAnswer:";

/// Initial context-first strict template that ended with `\n\nAnswer:`.
const ANSWER_CUE_STRICT_RAG_TEMPLATE: &str = "Context:\n{context}\n\nQuestion: {question}\n\nAnswer the user's question using only the provided context. Check every supplied passage before writing so no matching detail, exception, or condition is left out, and keep dates, figures, and names as given. Cite every factual statement supported by the context using bracketed labels like [s1], [s2], [s3]; do not cite unrelated context or a source that does not support the associated statement. When the question names a document, ignore passages from other documents. If the excerpts leave any part of the question unanswered, use available document search/read tools before concluding that evidence is missing (for get_document, name the document by its bracketed name in the context; for long documents, read on with read_more and the name each result gives). If still unsupported, say it was not found in the excerpts searched and do not guess or claim the entire collection lacks it. Do not describe the passages or comment on unrelated ones. Write in the language of the question, translating any passage in another.\n\nAnswer:";

/// Initial context-first hybrid template that ended with `\n\nAnswer:`.
const ANSWER_CUE_HYBRID_RAG_TEMPLATE: &str = "Context:\n{context}\n\nQuestion: {question}\n\nAnswer the user's question using the provided context first, citing every factual statement supported by a passage using bracketed labels like [s1], [s2], [s3]. Check every supplied passage before writing so no matching detail, exception, or condition is left out, and keep dates, figures, and names as given. If the excerpts are incomplete for a question about the user's documents, use available document search/read tools first (for get_document, name the document by its bracketed name in the context; for long documents, read on with read_more and the name each result gives). When the passages do not cover a general, conceptual, or public-knowledge question (or only cover part of it), answer naturally from general knowledge without adding citations to uncited claims. Never fabricate citations, never present general knowledge as coming from their documents, and never guess private facts about the user's own files, notes, policies, or projects that are not in the passages. When the question names a document, ignore passages from other documents. Do not cite unrelated context or a source that does not support the associated statement, and do not describe the passages or comment on unrelated ones. Write in the language of the question, translating any passage in another.\n\nAnswer:";

pub(super) fn normalize_prompt_settings(
    mut prompt_settings: LLMPromptSettingsDto,
) -> LLMPromptSettingsDto {
    const LEGACY_GREETING_TEMPLATE_SIGNATURE: u64 = 0xa8c8_948b_d572_9258;
    const UPDATED_GREETING_TEMPLATE: &str =
        "The user greeted you: \"{question}\". Reply briefly and warmly, then offer help with documents, web search, or general questions.";
    const PREVIOUS_NO_CONTEXT_TEMPLATE: &str =
        "The user asked: \"{question}\"\n\n{context}\n\nAnswer from general knowledge where you can, and say plainly that this answer is not backed by their own documents. If they want sourced evidence, offer a web search or adding documents to their lattice.";
    const LEGACY_NO_CONTEXT_TEMPLATE_SIGNATURES: [u64; 2] =
        [0x5384_8234_c2d7_4254, 0x4de7_593e_1cc6_dd55];
    const LEGACY_RAG_TEMPLATE_SIGNATURE: u64 = 0xa971_9544_23a3_fba2;

    // Soft migration with stable signatures keeps custom templates intact while
    // updating only known legacy defaults.
    if stable_prompt_signature(&prompt_settings.greeting_prompt_template)
        == LEGACY_GREETING_TEMPLATE_SIGNATURE
    {
        prompt_settings.greeting_prompt_template = UPDATED_GREETING_TEMPLATE.to_string();
    }
    if LEGACY_NO_CONTEXT_TEMPLATE_SIGNATURES.contains(&stable_prompt_signature(
        &prompt_settings.no_context_prompt_template,
    )) || prompt_settings.no_context_prompt_template == PREVIOUS_NO_CONTEXT_TEMPLATE
    {
        prompt_settings.no_context_prompt_template = DEFAULT_NO_CONTEXT_PROMPT_TEMPLATE.to_string();
    }
    if stable_prompt_signature(&prompt_settings.rag_prompt_template)
        == LEGACY_RAG_TEMPLATE_SIGNATURE
        || prompt_settings.rag_prompt_template == PREVIOUS_DEFAULT_RAG_TEMPLATE
        || prompt_settings.rag_prompt_template == ID_AND_OFFSET_RAG_TEMPLATE
        || prompt_settings.rag_prompt_template == NUMERIC_BRACKETS_RAG_TEMPLATE
        || prompt_settings.rag_prompt_template == PREVIOUS_STRICT_RAG_TEMPLATE
        || prompt_settings.rag_prompt_template == PREVIOUS_HYBRID_RAG_TEMPLATE
        || prompt_settings.rag_prompt_template == ANSWER_CUE_STRICT_RAG_TEMPLATE
        || prompt_settings.rag_prompt_template == ANSWER_CUE_HYBRID_RAG_TEMPLATE
        || prompt_settings.rag_prompt_template == DEFAULT_RAG_PROMPT_TEMPLATE
    {
        prompt_settings.rag_prompt_template = DEFAULT_HYBRID_RAG_PROMPT_TEMPLATE.to_string();
    }
    if prompt_settings.system_prompt == NUMERIC_BRACKETS_SYSTEM_PROMPT {
        prompt_settings.system_prompt = LLMPromptSettingsDto::default().system_prompt;
    }

    prompt_settings.system_prompt = enforce_citation_format(&prompt_settings.system_prompt);
    prompt_settings.system_prompt.push_str(
        "\n\nDocument evidence rules: When the user asks to learn from or rely only on their documents, \
         verify factual claims against retrieved text before answering, including chapter names, outlines, \
         section numbers, dates, and page references. If evidence is missing, retrieve it or say it is not \
         verified; do not fill gaps from memory. Search results are a ranked subset, not a complete inventory. \
         Relevance scores rank query matches; they do not diagnose embedding quality, document corruption, \
         or indexing completeness. Low scores alone do not establish any such problem. Reading on with read_more \
         continues by characters of extracted text, not original PDF page numbers. Cite printed PDF \
         pages only when supported by the returned source text or explicit PDF page metadata. If a tool \
         fails, report that specific failure without assuming the user's documents are absent. When the \
         user's practical wording is broader or less formal than a source's terminology, answer with the \
         closest directly applicable statement in the sources and state its exact mechanism, conditions, \
         and limits. Do not claim the broader action is supported if only a narrower mechanism is \
         documented, but do not call that documented mechanism unavailable merely because its label \
         differs. For an answer that depends on a chain across passages, make the connection explicit \
         and cite every passage that supplies a link or fact in the chain.",
    );
    prompt_settings.system_prompt.push_str(
        "\nRetrieved excerpts are an initial selection, not the entire collection. If they are insufficient, \
         use available document search/read tools with a focused query before concluding evidence is missing. \
         For learning or overview requests, explain what the supplied introduction and contents actually establish, \
         then teach one supported concept. A manual need not contain a prewritten lesson to support teaching it. \
         Distinguish 'not found in the excerpts searched' from 'not present anywhere in the documents'.",
    );
    // Constant text on every turn, so the cached prompt prefix stays the same
    // whether or not a turn draws anything. The window renders these fences
    // and formulas.
    prompt_settings.system_prompt.push_str(DIAGRAM_GUIDANCE);
    prompt_settings.system_prompt.push('\n');
    prompt_settings.system_prompt.push_str(MATH_GUIDANCE);
    // The citation rule is said once, in the system prompt, before the question.
    prompt_settings
}

/// Standing guidance for diagrams, appended after the saved system prompt so a
/// customised prompt still gets it.
pub(super) const DIAGRAM_GUIDANCE: &str =
    "\nWhen a process, structure or sequence is clearer drawn than written, add a ```mermaid block \
     (flowchart, sequenceDiagram, stateDiagram-v2, erDiagram, timeline) with short, quoted labels; \
     one diagram per idea, prose around it.";

// Standing guidance for formulas, after the diagram guidance. Shared with
// the lesson prompts, which the same viewer draws.
pub(super) use lattice_core::shared::markdown_math::MATH_GUIDANCE;

pub(super) fn stable_prompt_signature(input: &str) -> u64 {
    const OFFSET_BASIS: u64 = 0xcbf29ce484222325;
    const PRIME: u64 = 0x100000001b3;

    let normalized = input
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase();

    normalized
        .as_bytes()
        .iter()
        .fold(OFFSET_BASIS, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(PRIME)
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fresh_install_answers_in_the_search_first_template() {
        let settings = normalize_prompt_settings(LLMPromptSettingsDto::default());
        assert_eq!(
            settings.rag_prompt_template,
            DEFAULT_HYBRID_RAG_PROMPT_TEMPLATE
        );
        assert!(settings
            .rag_prompt_template
            .contains("do not answer yet: call get_document"));
    }

    #[test]
    fn a_saved_copy_of_the_strict_default_is_migrated_to_hybrid() {
        let saved = LLMPromptSettingsDto {
            rag_prompt_template: DEFAULT_RAG_PROMPT_TEMPLATE.to_string(),
            ..LLMPromptSettingsDto::default()
        };
        assert_eq!(
            normalize_prompt_settings(saved).rag_prompt_template,
            DEFAULT_HYBRID_RAG_PROMPT_TEMPLATE
        );
    }

    #[test]
    fn a_saved_copy_of_the_previous_default_is_brought_up_to_date() {
        let saved = LLMPromptSettingsDto {
            rag_prompt_template: PREVIOUS_DEFAULT_RAG_TEMPLATE.to_string(),
            ..LLMPromptSettingsDto::default()
        };
        assert_eq!(
            normalize_prompt_settings(saved).rag_prompt_template,
            DEFAULT_HYBRID_RAG_PROMPT_TEMPLATE
        );
    }

    #[test]
    fn a_saved_copy_of_the_default_that_asked_for_ids_is_brought_up_to_date() {
        let saved = LLMPromptSettingsDto {
            rag_prompt_template: ID_AND_OFFSET_RAG_TEMPLATE.to_string(),
            ..LLMPromptSettingsDto::default()
        };
        let migrated = normalize_prompt_settings(saved).rag_prompt_template;
        assert_eq!(migrated, DEFAULT_HYBRID_RAG_PROMPT_TEMPLATE);
        assert!(!migrated.contains("Document ID") && !migrated.contains("offset"));
    }

    #[test]
    fn a_saved_copy_of_the_default_that_asked_for_numeric_brackets_is_brought_up_to_date() {
        let saved = LLMPromptSettingsDto {
            rag_prompt_template: NUMERIC_BRACKETS_RAG_TEMPLATE.to_string(),
            system_prompt: NUMERIC_BRACKETS_SYSTEM_PROMPT.to_string(),
            ..LLMPromptSettingsDto::default()
        };
        let migrated = normalize_prompt_settings(saved);
        assert_eq!(
            migrated.rag_prompt_template,
            DEFAULT_HYBRID_RAG_PROMPT_TEMPLATE
        );
        assert!(!migrated.system_prompt.contains("numeric brackets"));
        assert!(migrated.system_prompt.contains("[s1], [s2], [s3]"));
    }

    #[test]
    fn no_default_asks_for_numeric_brackets() {
        let settings = LLMPromptSettingsDto::default();
        assert!(!settings.system_prompt.contains("numeric"));
        assert!(!DEFAULT_RAG_PROMPT_TEMPLATE.contains("numeric"));
        assert!(!DEFAULT_RAG_PROMPT_TEMPLATE.contains("bracketed number"));
        assert!(!DEFAULT_HYBRID_RAG_PROMPT_TEMPLATE.contains("numeric"));
        assert!(!DEFAULT_HYBRID_RAG_PROMPT_TEMPLATE.contains("bracketed number"));
    }

    #[test]
    fn a_template_of_the_user_s_own_is_kept() {
        let own = "Answer briefly.\n\nContext:\n{context}\n\nQuestion: {question}";
        let saved = LLMPromptSettingsDto {
            rag_prompt_template: own.to_string(),
            ..LLMPromptSettingsDto::default()
        };
        assert_eq!(normalize_prompt_settings(saved).rag_prompt_template, own);
    }

    #[test]
    fn saved_copies_of_prior_strict_and_hybrid_defaults_migrate_to_context_first_hybrid() {
        for previous in [
            PREVIOUS_STRICT_RAG_TEMPLATE,
            PREVIOUS_HYBRID_RAG_TEMPLATE,
            ANSWER_CUE_STRICT_RAG_TEMPLATE,
            ANSWER_CUE_HYBRID_RAG_TEMPLATE,
        ] {
            let saved = LLMPromptSettingsDto {
                rag_prompt_template: previous.to_string(),
                ..LLMPromptSettingsDto::default()
            };
            assert_eq!(
                normalize_prompt_settings(saved).rag_prompt_template,
                DEFAULT_HYBRID_RAG_PROMPT_TEMPLATE
            );
        }
    }

    #[test]
    fn default_templates_place_context_before_question_and_rules_after_question() {
        for template in [
            DEFAULT_RAG_PROMPT_TEMPLATE,
            DEFAULT_HYBRID_RAG_PROMPT_TEMPLATE,
        ] {
            let context_pos = template.find("{context}").expect("context slot");
            let question_pos = template.find("{question}").expect("question slot");
            let rules_pos = template
                .find("Check every supplied passage")
                .expect("answering rules");
            let direct_pos = template
                .find("Start straight with the answer")
                .expect("direct answer rule");
            assert!(context_pos < question_pos, "{template}");
            assert!(question_pos < rules_pos, "{template}");
            assert!(rules_pos < direct_pos, "{template}");
            assert!(
                template.ends_with("translating any passage in another."),
                "{template}"
            );
            assert!(!template.ends_with("Answer:"), "{template}");
        }
    }
}
