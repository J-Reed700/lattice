//! Parser and citation-mapping tests. All pure — no database, no LLM.

#![cfg(test)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

use super::parser::parse_compare_response;
use super::retrieval::RetrievedChunk;
use super::use_case::{assemble_cells, map_quote_to_chunk};

fn columns() -> Vec<String> {
    vec!["method".to_string(), "sample size".to_string()]
}

fn chunk(id: &str, content: &str, index: usize) -> RetrievedChunk {
    RetrievedChunk {
        chunk_id: id.to_string(),
        content: content.to_string(),
        section: None,
        index,
    }
}

fn sample_chunks() -> Vec<RetrievedChunk> {
    vec![
        chunk("chunk_1", "This paper reviews prior literature on sleep.", 0),
        chunk(
            "chunk_2",
            "We conducted a randomised controlled trial across four sites.",
            1,
        ),
        chunk("chunk_3", "Funding was provided by the national council.", 2),
    ]
}

#[test]
fn parses_strict_json() {
    let raw = r#"{"method": {"value": "randomised controlled trial", "quote": "We conducted a randomised controlled trial across four sites."}, "sample size": {"value": "412", "quote": "412 adults enrolled."}}"#;
    let parsed = parse_compare_response(raw, &columns());
    assert_eq!(
        parsed["method"].0.as_deref(),
        Some("randomised controlled trial")
    );
    assert_eq!(parsed["sample size"].0.as_deref(), Some("412"));
}

#[test]
fn parses_json_inside_markdown_fence() {
    let raw = "```json\n{\"method\": {\"value\": \"survey\", \"quote\": null}, \"sample size\": {\"value\": null, \"quote\": null}}\n```";
    let parsed = parse_compare_response(raw, &columns());
    assert_eq!(parsed["method"].0.as_deref(), Some("survey"));
    assert_eq!(parsed["sample size"].0, None);
}

#[test]
fn parses_json_after_preamble() {
    let raw = "Sure! Here you go: {\"method\": {\"value\": \"case study\", \"quote\": null}, \"sample size\": {\"value\": null, \"quote\": null}}";
    let parsed = parse_compare_response(raw, &columns());
    assert_eq!(parsed["method"].0.as_deref(), Some("case study"));
}

#[test]
fn parses_column_names_case_insensitively() {
    let raw = r#"{"Method": {"value": "cohort", "quote": null}, "Sample  Size": {"value": "80", "quote": null}}"#;
    let parsed = parse_compare_response(raw, &columns());
    assert_eq!(parsed["method"].0.as_deref(), Some("cohort"));
    assert_eq!(parsed["sample size"].0.as_deref(), Some("80"));
}

#[test]
fn treats_null_like_strings_as_null() {
    let raw = r#"{"method": {"value": "n/a", "quote": ""}, "sample size": {"value": "Not Stated", "quote": "unknown"}}"#;
    let parsed = parse_compare_response(raw, &columns());
    assert_eq!(parsed["method"].0, None);
    assert_eq!(parsed["method"].1, None);
    assert_eq!(parsed["sample size"].0, None);
    assert_eq!(parsed["sample size"].1, None);
}

#[test]
fn malformed_response_yields_empty_map() {
    let parsed = parse_compare_response("I'm sorry, I can't help with that.", &columns());
    assert!(parsed.is_empty());

    let cells = assemble_cells(&parsed, &columns(), &sample_chunks());
    assert_eq!(cells.len(), 2);
    assert!(cells.iter().all(|cell| cell.value.is_none()));
}

#[test]
fn truncated_json_salvages_what_it_can() {
    let raw = r#"{"method": {"value": "randomised controlled trial", "quote": "We conducted a randomised controlled trial across four sites."}, "sample siz"#;
    let parsed = parse_compare_response(raw, &columns());
    assert_eq!(
        parsed["method"].0.as_deref(),
        Some("randomised controlled trial")
    );
    assert!(!parsed.contains_key("sample size"));
}

#[test]
fn map_quote_to_chunk_exact_match() {
    let chunks = sample_chunks();
    let found = map_quote_to_chunk(
        "We conducted a randomised controlled trial across four sites.",
        &chunks,
    )
    .unwrap();
    assert_eq!(found.chunk_id, "chunk_2");
}

#[test]
fn map_quote_to_chunk_ignores_whitespace_and_case() {
    let chunks = sample_chunks();
    let found = map_quote_to_chunk(
        "  WE   CONDUCTED a randomised\ncontrolled trial across four sites  ",
        &chunks,
    )
    .unwrap();
    assert_eq!(found.chunk_id, "chunk_2");
}

#[test]
fn map_quote_to_chunk_token_overlap_above_threshold() {
    let chunks = sample_chunks();
    let found = map_quote_to_chunk(
        "We performed a randomised controlled trial across four sites.",
        &chunks,
    )
    .unwrap();
    assert_eq!(found.chunk_id, "chunk_2");
}

#[test]
fn map_quote_to_chunk_returns_none_for_invented_quote() {
    let chunks = sample_chunks();
    assert!(map_quote_to_chunk(
        "Participants received weekly vitamin supplements throughout winter.",
        &chunks
    )
    .is_none());
}

#[test]
fn cells_are_ordered_and_complete() {
    let raw = r#"{"sample size": {"value": "412", "quote": "Funding was provided by the national council."}}"#;
    let cols = columns();
    let parsed = parse_compare_response(raw, &cols);
    let cells = assemble_cells(&parsed, &cols, &sample_chunks());

    assert_eq!(cells.len(), cols.len());
    // Column order is the request's order, not the model's.
    assert_eq!(cells[0].value, None);
    assert_eq!(cells[1].value.as_deref(), Some("412"));
    assert_eq!(
        cells[1].citation.as_ref().unwrap().chunk_id.as_str(),
        "chunk_3"
    );
}

#[test]
fn salvage_does_not_panic_on_non_ascii_text() {
    // `İ` lowercases to two chars under Unicode rules, which used to shift
    // every byte offset and slice `raw` mid-character. The salvage pass must
    // stay on char boundaries whatever the model emits.
    let raw = "İ method—\"value\": \"survey\", \"quote\": \"Bir çalışma—yöntem.\"";
    let parsed = parse_compare_response(raw, &columns());
    assert_eq!(parsed["method"].0.as_deref(), Some("survey"));
}

#[test]
fn parses_nothing_from_prose_with_multibyte_characters() {
    let raw = "Üzgünüm — bu belgede yöntem belirtilmemiş. İşte bu kadar.";
    let parsed = parse_compare_response(raw, &columns());
    let cells = assemble_cells(&parsed, &columns(), &sample_chunks());
    assert_eq!(cells.len(), 2);
    assert!(cells.iter().all(|cell| cell.value.is_none()));
}
