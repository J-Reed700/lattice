//! Parser and citation-mapping tests, pure, plus one retrieval test over an
//! in-memory container. No LLM.

#![cfg(test)]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]

use super::di::CompareDeps;
use super::parser::parse_compare_response;
use super::retrieval::RetrievedChunk;
use super::use_case::{assemble_cells, fill_row, map_quote_to_chunk};
use lattice_core::application::ports::llm_port::{
    CompletionInput, CompletionRequest, CompletionResponse, InferencePriority,
};
use lattice_core::application::ports::LLMPort;
use lattice_core::shared::error::Result;
use std::sync::Arc;

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
        chunk(
            "chunk_1",
            "This paper reviews prior literature on sleep.",
            0,
        ),
        chunk(
            "chunk_2",
            "We conducted a randomised controlled trial across four sites.",
            1,
        ),
        chunk(
            "chunk_3",
            "Funding was provided by the national council.",
            2,
        ),
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

async fn container_with_chunks(contents: &[String]) -> crate::interfaces::di::Container {
    let container = crate::tests::common::setup_test_container()
        .await
        .expect("container");
    let pool = container.db_pool().clone();
    sqlx::query(
        "INSERT INTO documents (id, file_path, file_name, size_bytes, modified_at, checksum)
         VALUES ('doc', '/lib/doc.md', 'doc.md', 1, '2026-01-01T00:00:00Z', 'sum')",
    )
    .execute(&pool)
    .await
    .expect("document");
    for (index, content) in contents.iter().enumerate() {
        sqlx::query(
            "INSERT INTO text_chunks (id, document_id, content, chunk_index) VALUES (?, 'doc', ?, ?)",
        )
        .bind(format!("c{index}"))
        .bind(content)
        .bind(index as i64)
        .execute(&pool)
        .await
        .expect("chunk");
    }
    container
}

/// Bind the retrieval deps from a test container. The loader is a placeholder:
/// the retrieval tests never load a model.
fn retrieval_deps(container: &crate::interfaces::di::Container) -> CompareDeps {
    CompareDeps {
        load_llm: super::di::LlmLoader::new(|| {
            Box::pin(async {
                Err(lattice_core::shared::error::AppError::InternalError(
                    "not used in retrieval tests".into(),
                ))
            })
        }),
        documents: container.document_repository(),
        search: Arc::new(super::scoped_search::HybridScopedSearch::new(
            container.hybrid_search_use_case(),
        )),
        chunks: container.chunk_repository(),
    }
}

/// Columns are searched with the library's hybrid search, as chat searches
/// it: with no embedding model the keyword branch still finds the passage a
/// column names, so the row is not reduced to the document's opening.
#[tokio::test]
async fn without_an_embedding_model_the_keyword_branch_still_finds_the_column() {
    let mut contents: Vec<String> = (0..12).map(|index| format!("passage {index}")).collect();
    contents.push("The method was a randomised controlled trial.".to_string());
    let container = container_with_chunks(&contents).await;

    let deps = retrieval_deps(&container);
    let retrieved = super::retrieval::retrieve_for_document(&deps, "doc", &columns(), 3)
        .await
        .expect("retrieve");

    assert!(retrieved.degraded.is_none());
    assert_eq!(retrieved.chunks[0].chunk_id, "c12");
}

/// A document the search finds nothing in still gets a row: its opening
/// chunks, as many as the searches would have read (the row call's room sets
/// that depth). The search worked, so the row is not marked degraded.
#[tokio::test]
async fn a_document_the_search_finds_nothing_in_falls_back_to_its_opening_chunks() {
    let contents: Vec<String> = (0..40).map(|index| format!("passage {index}")).collect();
    let container = container_with_chunks(&contents).await;

    let deps = retrieval_deps(&container);
    let per_column = 7;
    let retrieved = super::retrieval::retrieve_for_document(&deps, "doc", &columns(), per_column)
        .await
        .expect("retrieve");

    assert!(retrieved.degraded.is_none());
    assert_eq!(retrieved.chunks.len(), per_column * columns().len());
    assert_eq!(retrieved.chunks[0].chunk_id, "c0");
}

#[derive(Default)]
struct Recorder(std::sync::Mutex<Vec<CompletionRequest>>);

#[async_trait::async_trait]
impl LLMPort for Recorder {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        self.0.lock().unwrap().push(request.clone());
        Ok(CompletionResponse::from_text("{}"))
    }
    fn model_name(&self) -> &str {
        "recorder"
    }
    fn max_context_tokens(&self) -> usize {
        8192
    }
    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
}

/// The passages are in the prompt; sending them again as context paid for
/// every one twice.
#[tokio::test]
async fn a_row_sends_its_passages_once_at_interactive_priority() {
    let llm = Recorder::default();
    let (_, carried) = fill_row(&llm, "Sleep study", &columns(), &sample_chunks())
        .await
        .unwrap();
    assert_eq!(carried.len(), 3, "an 8k window carries every passage");
    let sent = llm.0.lock().unwrap();
    assert_eq!(sent[0].priority, InferencePriority::Interactive);
    let [CompletionInput::Message { role, content }] = sent[0].input.as_slice() else {
        panic!("one user message, got {:?}", sent[0].input);
    };
    assert_eq!(role, "user");
    assert_eq!(
        content
            .matches("This paper reviews prior literature on sleep.")
            .count(),
        1
    );
}

/// A model with a small window that records what it was sent.
struct SmallWindow(std::sync::Mutex<Vec<CompletionRequest>>);

#[async_trait::async_trait]
impl LLMPort for SmallWindow {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        self.0.lock().unwrap().push(request.clone());
        Ok(CompletionResponse::from_text("{}"))
    }
    fn model_name(&self) -> &str {
        "small-window"
    }
    fn max_context_tokens(&self) -> usize {
        1_500
    }
    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
}

/// The window, not a character cap, decides how much of a document a row
/// reads: the best-ranked passages that fit go in, and a citation can only
/// point at a passage the model was shown.
#[tokio::test]
async fn a_small_window_carries_the_best_passages_that_fit_and_reports_them() {
    let llm = SmallWindow(std::sync::Mutex::new(Vec::new()));
    let mut chunks = sample_chunks();
    chunks.insert(1, chunk("chunk_long", &"filler sentence. ".repeat(400), 3));

    let (_, carried) = fill_row(&llm, "Sleep study", &columns(), &chunks)
        .await
        .unwrap();

    let carried_ids: Vec<&str> = carried.iter().map(|c| c.chunk_id.as_str()).collect();
    assert_eq!(carried_ids, vec!["chunk_1", "chunk_2", "chunk_3"]);
    let sent = llm.0.lock().unwrap();
    let prompt = sent[0].user_text();
    assert!(!prompt.contains("filler sentence."));
    assert!(prompt.contains("Passages:\n[1] This paper reviews prior literature on sleep."));
}

/// The table kept six columns and cut each name at 80 characters. Every
/// column named is kept, whole.
#[test]
fn every_named_column_is_kept_whole() {
    let long = format!("the primary outcome measure {}", "and its unit ".repeat(10));
    let mut named: Vec<String> = (0..9).map(|n| format!("field {n}")).collect();
    named.push(long.clone());
    named.push("FIELD 3".to_string());
    let columns = super::use_case::sanitize_columns(named);
    assert_eq!(columns.len(), 10, "{columns:?}");
    assert!(columns.contains(&long.trim().to_string()));
}

/// Says which columns each call asked for, on a window whose answer room
/// holds four cells.
#[derive(Default)]
struct FourCellsPerCall(std::sync::Mutex<Vec<String>>);

#[async_trait::async_trait]
impl LLMPort for FourCellsPerCall {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        self.0.lock().unwrap().push(request.user_text().to_string());
        Ok(CompletionResponse::from_text("{}"))
    }
    fn model_name(&self) -> &str {
        "four-cells"
    }
    fn max_context_tokens(&self) -> usize {
        2_048
    }
    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
}

/// A row with more columns than one answer holds is filled in several calls,
/// every column in exactly one of them, and the row has a cell for each.
#[tokio::test]
async fn a_row_of_many_columns_is_filled_across_calls() {
    let contents: Vec<String> = (0..6).map(|index| format!("passage {index}")).collect();
    let container = container_with_chunks(&contents).await;
    let llm = Arc::new(FourCellsPerCall::default());
    let model: Arc<dyn LLMPort> = llm.clone();
    let deps = retrieval_deps(&container);
    let columns: Vec<String> = (0..9).map(|n| format!("column {n}")).collect();
    let document = super::use_case::ResolvedDocument {
        id: "doc".to_string(),
        title: "doc.md".to_string(),
        file_path: "/lib/doc.md".to_string(),
        error: None,
    };
    let row = super::use_case::build_row(&deps, &model, &document, &columns)
        .await
        .unwrap();

    assert_eq!(row.cells.len(), 9);
    assert!(row.error.is_none(), "{:?}", row.error);
    let prompts = llm.0.lock().unwrap();
    assert_eq!(prompts.len(), 3, "nine columns at four a call");
    for column in &columns {
        let asked = prompts
            .iter()
            .filter(|prompt| prompt.contains(&format!("- {column}\n")))
            .count();
        assert_eq!(asked, 1, "{column} asked {asked} times");
    }
}

/// Answers the first call with a value for every column it asks about and
/// fails every call after it.
#[derive(Default)]
struct FailsAfterFirstCall(std::sync::Mutex<usize>);

#[async_trait::async_trait]
impl LLMPort for FailsAfterFirstCall {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        let mut calls = self.0.lock().unwrap();
        *calls += 1;
        if *calls > 1 {
            return Err(lattice_core::shared::error::AppError::Network(
                "server went away".into(),
            ));
        }
        let answer: serde_json::Map<String, serde_json::Value> = (0..9)
            .map(|n| format!("column {n}"))
            .filter(|column| request.user_text().contains(&format!("- {column}\n")))
            .map(|column| (column, serde_json::json!({"value": "found", "quote": null})))
            .collect();
        Ok(CompletionResponse::from_text(
            serde_json::Value::Object(answer).to_string(),
        ))
    }
    fn model_name(&self) -> &str {
        "fails-after-first"
    }
    fn max_context_tokens(&self) -> usize {
        2_048
    }
    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
}

/// A row split across calls used to be all or nothing: the second call
/// failing threw away the cells the first had filled. They are kept, and the
/// columns not reached are left empty and named.
#[tokio::test]
async fn a_failed_later_call_keeps_the_cells_earlier_calls_filled() {
    let contents: Vec<String> = (0..6).map(|index| format!("passage {index}")).collect();
    let container = container_with_chunks(&contents).await;
    let model: Arc<dyn LLMPort> = Arc::new(FailsAfterFirstCall::default());
    let deps = retrieval_deps(&container);
    let columns: Vec<String> = (0..9).map(|n| format!("column {n}")).collect();
    let document = super::use_case::ResolvedDocument {
        id: "doc".to_string(),
        title: "doc.md".to_string(),
        file_path: "/lib/doc.md".to_string(),
        error: None,
    };
    let row = super::use_case::build_row(&deps, &model, &document, &columns)
        .await
        .unwrap();

    assert_eq!(row.cells.len(), 9);
    let filled = row.cells.iter().filter(|cell| cell.value.is_some()).count();
    assert_eq!(filled, 4, "the first call's four cells are kept");
    let error = row.error.expect("the row says what is missing");
    assert!(error.contains("Not filled: column 4"), "{error}");
    assert!(error.contains("column 8"), "{error}");
}
