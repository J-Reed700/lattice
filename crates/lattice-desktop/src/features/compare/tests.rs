//! Retrieval tests over an in-memory container, and the row contract driven by
//! a scripted model.

#![cfg(test)]

use super::di::CompareDeps;
use super::dto::CompareRowDto;
use super::prompt::{Column, CompareRow, Input, Passage, Sentence};
use super::use_case::{build_row, ResolvedDocument};
use futures::future::BoxFuture;
use lattice_core::application::ports::llm_port::{CompletionInput, CompletionResponse};
use lattice_core::application::ports::LLMPort;
use lattice_core::application::services::model_contract::packed::Packed;
use lattice_core::application::services::model_contract::test_support::taint::{holds, Marks};
use lattice_core::application::services::model_contract::test_support::{ScriptedModel, Sent};
use lattice_core::shared::error::{AppError, Result};
use std::sync::Arc;

fn columns() -> Vec<String> {
    vec!["method".to_string(), "sample size".to_string()]
}

async fn container_with_chunks(contents: &[String]) -> crate::interfaces::di::Container {
    let container = crate::tests::common::setup_test_container()
        .await
        .expect("container");
    let mut tx = container
        .database()
        .begin_write()
        .await
        .expect("write unit");
    sqlx::query(
        "INSERT INTO documents (id, file_path, file_name, size_bytes, modified_at, checksum)
         VALUES ('doc', '/lib/doc.md', 'doc.md', 1, '2026-01-01T00:00:00Z', 'sum')",
    )
    .execute(&mut *tx)
    .await
    .expect("document");
    for (index, content) in contents.iter().enumerate() {
        sqlx::query(
            "INSERT INTO text_chunks (id, document_id, content, chunk_index) VALUES (?, 'doc', ?, ?)",
        )
        .bind(format!("c{index}"))
        .bind(content)
        .bind(index as i64)
        .execute(&mut *tx)
        .await
        .expect("chunk");
    }
    tx.commit().await.expect("commit");
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

const KILN: &str = "The kiln is fired at dawn.";
const CRANES: &str = "The quay holds two cranes.";

fn document() -> ResolvedDocument {
    ResolvedDocument {
        id: "doc".to_string(),
        title: "doc.md".to_string(),
        file_path: "/lib/doc.md".to_string(),
        error: None,
    }
}

fn row_columns() -> Vec<String> {
    vec!["kiln firing".to_string(), "crane count".to_string()]
}

/// Everything a request shows the model, as one text.
fn shown(sent: &Sent) -> String {
    sent.request
        .input()
        .iter()
        .filter_map(|item| match item {
            CompletionInput::Message { content, .. } => Some(content.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The handle written in `[…]` at the start of the line that holds `text`.
fn handle_of(prompt: &str, text: &str) -> String {
    let line = prompt
        .lines()
        .find(|line| line.starts_with('[') && line.contains(text))
        .unwrap_or_else(|| panic!("no handle for {text} in:\n{prompt}"));
    let end = line.find(']').expect("a closing bracket");
    line[1..end].to_string()
}

fn column_handles(prompt: &str) -> Vec<String> {
    prompt
        .lines()
        .filter(|line| line.starts_with("[f"))
        .filter_map(|line| Some(line.get(1..line.find(']')?)?.to_string()))
        .collect()
}

fn stated(of: &str, value: &str, support: &[&str]) -> serde_json::Value {
    serde_json::json!({
        "of": of,
        "answer": {"kind": "stated", "value": value, "support": support}
    })
}

fn not_stated(of: &str) -> serde_json::Value {
    serde_json::json!({"of": of, "answer": {"kind": "not-stated"}})
}

fn entries(list: Vec<serde_json::Value>) -> String {
    serde_json::json!({ "entries": list }).to_string()
}

fn reply(text: String) -> BoxFuture<'static, Result<CompletionResponse>> {
    Box::pin(std::future::ready(Ok(CompletionResponse::from_text(text))))
}

async fn row_of(model: &Arc<ScriptedModel>, columns: &[String]) -> Result<CompareRowDto> {
    let container = container_with_chunks(&[format!("{KILN} {CRANES}")]).await;
    let llm: Arc<dyn LLMPort> = model.clone();
    build_row(&retrieval_deps(&container), &llm, &document(), columns).await
}

#[test]
fn the_request_tells_what_it_must_and_hides_the_identities() {
    let mut marks = Marks::new();
    let input = Input {
        title: marks.told("Harbour kiln log".to_string()),
        columns: vec![
            Column {
                index: marks.reference(100_001_usize),
                name: marks.told("firing temperature".to_string()),
            },
            Column {
                index: marks.reference(100_002_usize),
                name: marks.told("number of cranes".to_string()),
            },
        ],
        passages: Packed::new(
            vec![Passage {
                sentences: vec![
                    Sentence {
                        chunk_id: marks.reference("chunk-id-7a31c9".to_string()),
                        at: marks.reference(100_003_usize),
                        text: marks.told(KILN.to_string()),
                    },
                    Sentence {
                        chunk_id: marks.reference("chunk-id-7a31c9".to_string()),
                        at: marks.reference(100_004_usize),
                        text: marks.told(CRANES.to_string()),
                    },
                ],
            }],
            |_| 10,
        ),
    };
    holds::<CompareRow>(&input, &marks);
}

/// A cell cites by pointing at a sentence the passages showed; the table's
/// citation holds the app's own copy of it and the chunk it is of.
#[tokio::test]
async fn a_cell_cites_the_sentence_the_model_pointed_at() {
    let model = Arc::new(ScriptedModel::new().on_request::<CompareRow>(|sent, _| {
        let prompt = shown(sent);
        reply(entries(vec![
            stated(
                &handle_of(&prompt, "kiln firing"),
                "at dawn",
                &[&handle_of(&prompt, KILN)],
            ),
            not_stated(&handle_of(&prompt, "crane count")),
        ]))
    }));
    let row = row_of(&model, &row_columns()).await.unwrap();

    assert_eq!(row.cells.len(), 2);
    assert_eq!(row.cells[0].value.as_deref(), Some("at dawn"));
    let citation = row.cells[0].citation.as_ref().expect("a citation");
    assert_eq!(citation.chunk_id, "c0");
    assert_eq!(citation.excerpt, KILN);
    assert_eq!(row.cells[1].value, None);
    assert!(row.cells[1].citation.is_none());
    assert!(row.error.is_none(), "{:?}", row.error);
    let sent = model.sent();
    assert_eq!(sent.len(), 1);
    let prompt = shown(&sent[0]);
    assert!(prompt.contains("] kiln firing\n"), "{prompt}");
    assert!(!prompt.contains("spelled exactly"), "{prompt}");
}

/// A pointer at a sentence that is not shown costs only the citation.
#[tokio::test]
async fn a_pointer_at_nothing_shown_costs_only_the_citation() {
    let model = Arc::new(ScriptedModel::new().on_request::<CompareRow>(|sent, _| {
        let prompt = shown(sent);
        reply(entries(vec![
            stated(&handle_of(&prompt, "kiln firing"), "at dawn", &["s99"]),
            not_stated(&handle_of(&prompt, "crane count")),
        ]))
    }));
    let row = row_of(&model, &row_columns()).await.unwrap();

    assert_eq!(row.cells[0].value.as_deref(), Some("at dawn"));
    assert!(row.cells[0].citation.is_none());
    assert_eq!(model.sent().len(), 1);
}

/// A column the answer left out is asked for again alone; the other stands.
#[tokio::test]
async fn a_column_left_out_is_asked_for_again_alone() {
    let model = Arc::new(ScriptedModel::new().on_request::<CompareRow>(|sent, nth| {
        let prompt = shown(sent);
        if nth == 0 {
            reply(entries(vec![stated(
                &handle_of(&prompt, "kiln firing"),
                "at dawn",
                &[],
            )]))
        } else {
            reply(entries(vec![stated(
                &handle_of(&prompt, "crane count"),
                "two",
                &[],
            )]))
        }
    }));
    let row = row_of(&model, &row_columns()).await.unwrap();

    assert_eq!(row.cells[0].value.as_deref(), Some("at dawn"));
    assert_eq!(row.cells[1].value.as_deref(), Some("two"));
    assert!(row.error.is_none(), "{:?}", row.error);
    let sent = model.sent();
    assert_eq!(sent.len(), 2);
    let second = shown(&sent[1]);
    assert!(second.contains("] crane count"), "{second}");
    assert!(!second.contains("] kiln firing"), "{second}");
}

/// A stated value with nothing in it is refused and corrected for that
/// column alone.
#[tokio::test]
async fn a_blank_value_is_corrected_alone() {
    let model = Arc::new(ScriptedModel::new().on_request::<CompareRow>(|sent, nth| {
        let prompt = shown(sent);
        let value = if nth == 0 { "  " } else { "at dawn" };
        reply(entries(vec![
            stated(&handle_of(&prompt, "kiln firing"), value, &[]),
            not_stated(&handle_of(&prompt, "crane count")),
        ]))
    }));
    let row = row_of(&model, &row_columns()).await.unwrap();

    assert_eq!(row.cells[0].value.as_deref(), Some("at dawn"));
    let sent = model.sent();
    assert_eq!(sent.len(), 2);
    assert!(shown(&sent[1]).contains("The value is empty."));
}

/// A column the model never answers is an empty cell the row says it could
/// not fill; the row is not an error.
#[tokio::test]
async fn a_column_that_stays_unanswered_is_named_on_the_row() {
    let model =
        Arc::new(ScriptedModel::new().on_request::<CompareRow>(|_, _| reply(entries(Vec::new()))));
    let row = row_of(&model, &row_columns()).await.unwrap();

    assert_eq!(row.cells.len(), 2);
    assert!(row.cells.iter().all(|cell| cell.value.is_none()));
    let error = row.error.expect("the row says what is missing");
    assert!(
        error.contains("no usable answer for: kiln firing, crane count"),
        "{error}"
    );
}

/// The driver, not a count of columns, decides how a row is split: columns
/// that fit are asked in one request.
#[tokio::test]
async fn a_row_of_many_columns_is_asked_in_one_request_when_it_fits() {
    let model = Arc::new(ScriptedModel::new().on_request::<CompareRow>(|sent, _| {
        let prompt = shown(sent);
        reply(entries(
            column_handles(&prompt)
                .iter()
                .map(|handle| not_stated(handle))
                .collect(),
        ))
    }));
    let columns: Vec<String> = (0..9).map(|n| format!("column {n}")).collect();
    let row = row_of(&model, &columns).await.unwrap();

    assert_eq!(row.cells.len(), 9);
    assert!(row.error.is_none(), "{:?}", row.error);
    assert_eq!(model.sent().len(), 1);
}

/// A fault of sending is the row's error, not an empty table.
#[tokio::test]
async fn a_send_fault_is_an_error() {
    let model = Arc::new(
        ScriptedModel::new()
            .on_request::<CompareRow>(|_, _| reply(String::new()))
            .failing_every_request(|_| AppError::Network("server went away".into())),
    );
    assert!(row_of(&model, &row_columns()).await.is_err());
}
