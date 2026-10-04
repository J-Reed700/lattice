use super::prompting::enforce_numeric_citation_format;
use crate::features::settings::dto::LLMPromptSettingsDto;

pub(super) fn normalize_prompt_settings(
    mut prompt_settings: LLMPromptSettingsDto,
) -> LLMPromptSettingsDto {
    const LEGACY_GREETING_TEMPLATE_SIGNATURE: u64 = 0xa8c8_948b_d572_9258;
    const UPDATED_GREETING_TEMPLATE: &str =
        "The user greeted you: \"{question}\". Reply briefly and warmly, then offer help with documents, web search, or general questions.";
    const LEGACY_NO_CONTEXT_TEMPLATE_SIGNATURES: [u64; 2] =
        [0x5384_8234_c2d7_4254, 0x4de7_593e_1cc6_dd55];
    const UPDATED_NO_CONTEXT_TEMPLATE: &str =
        "The user asked: \"{question}\"\n\n{context}\n\nAnswer from general knowledge where you can, and say plainly that this answer is not backed by their own documents. If they want sourced evidence, offer a web search or adding documents to their lattice.";
    const LEGACY_RAG_TEMPLATE_SIGNATURE: u64 = 0xa971_9544_23a3_fba2;
    const UPDATED_RAG_TEMPLATE: &str =
        "Answer the user's question using only the provided context. Cite every factual statement supported by the context using numeric brackets like [1], [2], [3]. If the excerpts are insufficient, use available document search/read tools before concluding that evidence is missing. If still unsupported, say it was not found in the excerpts searched and do not guess or claim the entire collection lacks it. Do not cite unrelated context. Do not cite a source that does not support the associated statement. If you need to call get_document, use the exact Document ID shown in the context. For long documents, request additional pages with the page parameter.\n\nContext:\n{context}\n\nQuestion: {question}\n\nAnswer:";

    // Soft migration with stable signatures keeps custom templates intact while
    // updating only known legacy defaults.
    if stable_prompt_signature(&prompt_settings.greeting_prompt_template)
        == LEGACY_GREETING_TEMPLATE_SIGNATURE
    {
        prompt_settings.greeting_prompt_template = UPDATED_GREETING_TEMPLATE.to_string();
    }
    if LEGACY_NO_CONTEXT_TEMPLATE_SIGNATURES.contains(&stable_prompt_signature(
        &prompt_settings.no_context_prompt_template,
    )) {
        prompt_settings.no_context_prompt_template = UPDATED_NO_CONTEXT_TEMPLATE.to_string();
    }
    if stable_prompt_signature(&prompt_settings.rag_prompt_template)
        == LEGACY_RAG_TEMPLATE_SIGNATURE
    {
        prompt_settings.rag_prompt_template = UPDATED_RAG_TEMPLATE.to_string();
    }

    prompt_settings.system_prompt = enforce_numeric_citation_format(&prompt_settings.system_prompt);
    prompt_settings.system_prompt.push_str(
        "\n\nDocument evidence rules: When the user asks to learn from or rely only on their documents, \
         verify factual claims against retrieved text before answering, including chapter names, outlines, \
         section numbers, dates, and page references. If evidence is missing, retrieve it or say it is not \
         verified; do not fill gaps from memory. Search results are a ranked subset, not a complete inventory. \
         Relevance scores rank query matches; they do not diagnose embedding quality, document corruption, \
         or indexing completeness. Low scores alone do not establish any such problem. get_document page \
         and total_pages describe internal text pagination, not original PDF page numbers. Cite printed PDF \
         pages only when supported by the returned source text or explicit PDF page metadata. If a tool \
         fails, report that specific failure without assuming the user's documents are absent.",
    );
    prompt_settings.system_prompt.push_str(
        "\nRetrieved excerpts are an initial selection, not the entire collection. If they are insufficient, \
         use available document search/read tools with a focused query before concluding evidence is missing. \
         For learning or overview requests, explain what the supplied introduction and contents actually establish, \
         then teach one supported concept. A manual need not contain a prewritten lesson to support teaching it. \
         Distinguish 'not found in the excerpts searched' from 'not present anywhere in the documents'.",
    );
    prompt_settings.rag_prompt_template =
        enforce_numeric_citation_format(&prompt_settings.rag_prompt_template);
    prompt_settings.tool_followup_prompt_template =
        enforce_numeric_citation_format(&prompt_settings.tool_followup_prompt_template);
    prompt_settings
}

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
