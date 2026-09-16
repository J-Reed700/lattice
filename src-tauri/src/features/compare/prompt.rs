//! The single per-document prompt. One call fills every column.
//!
//! Structured output is prompt-enforced (there is no schema parameter on
//! `LLMPort`), so the shape is stated exactly, an example is shown, and the
//! parser is tolerant.

use super::retrieval::RetrievedChunk;

pub fn build_compare_prompt(title: &str, columns: &[String], chunks: &[RetrievedChunk]) -> String {
    let field_list = columns
        .iter()
        .map(|column| format!("- {column}"))
        .collect::<Vec<_>>()
        .join("\n");

    let passages = chunks
        .iter()
        .enumerate()
        .map(|(index, chunk)| format!("[{}] {}", index + 1, chunk.content))
        .collect::<Vec<_>>()
        .join("\n\n");

    [
        "You are filling one row of a comparison table about a single document.".to_string(),
        String::new(),
        format!("Document title: {title}"),
        String::new(),
        "Fill exactly these fields, using only the passages below:".to_string(),
        field_list,
        String::new(),
        "Respond with a single JSON object and nothing else. No prose before or after, no"
            .to_string(),
        "markdown fences. The object has one key per field, spelled exactly as listed above."
            .to_string(),
        "Each value is an object with two keys:".to_string(),
        "  \"value\": a short factual answer (at most 25 words), or null".to_string(),
        "  \"quote\": the exact sentence from the passages that supports it, copied verbatim, or null"
            .to_string(),
        String::new(),
        "Rules:".to_string(),
        "- If the passages do not state the answer, use null for both \"value\" and \"quote\". Do"
            .to_string(),
        "  not guess, do not infer, do not answer from general knowledge.".to_string(),
        "- \"quote\" must be copied character-for-character from a passage. Never paraphrase it."
            .to_string(),
        "- Keep \"value\" plain text: no markdown, no citations, no brackets.".to_string(),
        String::new(),
        "Example of the required shape:".to_string(),
        r#"{"method": {"value": "randomised controlled trial", "quote": "We conducted a randomised controlled trial across four sites."}, "sample size": {"value": null, "quote": null}}"#
            .to_string(),
        String::new(),
        "Passages:".to_string(),
        passages,
    ]
    .join("\n")
}
