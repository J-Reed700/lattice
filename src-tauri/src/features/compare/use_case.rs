//! The compare pipeline: validate → resolve → retrieve → one grounded call per
//! document → tolerant parse → map quotes back to the chunks it carried →
//! assemble.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use futures::{stream, StreamExt};
use tracing::debug;

use crate::features::compare::di::CompareDeps;
use lattice_core::application::ports::llm_port::InferencePriority;
use lattice_core::application::ports::{LLMPort, RepositoryPort};
use lattice_core::application::services::grounded_generation::{
    self, CallOptions, EvidencePassage, EvidenceSelection, GroundedRequest,
};
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::persistence::timestamps::now_db_timestamp;

use super::dto::{
    CompareCellDto, CompareCitationDto, CompareDocumentsRequestDto, CompareRowDto, CompareTableDto,
};
use super::parser::parse_compare_response;
use super::prompt::{build_compare_task, PASSAGES_HEADING};
use super::retrieval::{retrieve_for_document, RetrievedChunk};

pub const DOCUMENT_CONCURRENCY: usize = 2;
/// How long one row call may take; a row of many columns is several calls.
pub const PER_CALL_TIMEOUT_SECS: u64 = 120;

/// Answer room one cell takes: a short value, the sentence quoted for it and
/// the JSON around them.
const CELL_OUTPUT_TOKENS: usize = 128;

/// Columns one row call fills: as many as a quarter of the model's window
/// holds as answer. A row with more columns than that is filled in several
/// calls, so no column is dropped and no answer is cut off.
fn columns_per_call(llm: &dyn LLMPort) -> usize {
    (llm.max_context_tokens() / 4 / CELL_OUTPUT_TOKENS).max(1)
}

/// Minimum share of a quote's words that must appear in a chunk before the
/// citation is trusted. Catches a model that dropped a comma.
const TOKEN_OVERLAP_THRESHOLD: f32 = 0.6;

#[derive(Debug, Clone)]
pub(super) struct ResolvedDocument {
    pub(super) id: String,
    pub(super) title: String,
    pub(super) file_path: String,
    pub(super) error: Option<String>,
}

/// Lowercase, collapse whitespace runs, strip leading/trailing punctuation.
fn normalize_for_match(text: &str) -> String {
    let collapsed = text
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase();
    collapsed
        .trim_matches(|c: char| !c.is_alphanumeric())
        .to_string()
}

fn significant_tokens(text: &str) -> HashSet<String> {
    normalize_for_match(text)
        .split(|c: char| !c.is_alphanumeric())
        .filter(|token| token.chars().count() >= 3)
        .map(|token| token.to_string())
        .collect()
}

/// Finds the chunk a model quote came from: verbatim containment first, then
/// token overlap. `None` when the quote cannot be placed.
pub(super) fn map_quote_to_chunk<'a>(
    quote: &str,
    chunks: &'a [RetrievedChunk],
) -> Option<&'a RetrievedChunk> {
    let needle = normalize_for_match(quote);
    if needle.is_empty() {
        return None;
    }

    for chunk in chunks {
        if normalize_for_match(&chunk.content).contains(&needle) {
            return Some(chunk);
        }
    }

    let quote_tokens = significant_tokens(quote);
    if quote_tokens.is_empty() {
        return None;
    }

    let mut best: Option<(&RetrievedChunk, f32)> = None;
    for chunk in chunks {
        let chunk_tokens = significant_tokens(&chunk.content);
        let shared = quote_tokens
            .iter()
            .filter(|token| chunk_tokens.contains(*token))
            .count();
        let score = shared as f32 / quote_tokens.len() as f32;
        if score >= TOKEN_OVERLAP_THRESHOLD && best.map(|(_, b)| score > b).unwrap_or(true) {
            best = Some((chunk, score));
        }
    }
    best.map(|(chunk, _)| chunk)
}

/// Trims, drops empties and dedupes case-insensitively, keeping the first
/// spelling. Every column the user named is kept, whole.
pub(super) fn sanitize_columns(columns: Vec<String>) -> Vec<String> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut out: Vec<String> = Vec::new();
    for column in columns {
        let trimmed = column.trim();
        if trimmed.is_empty() || !seen.insert(trimmed.to_lowercase()) {
            continue;
        }
        out.push(trimmed.to_string());
    }
    out
}

fn sanitize_document_ids(document_ids: Vec<String>) -> Vec<String> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut out: Vec<String> = Vec::new();
    for id in document_ids {
        let trimmed = id.trim();
        if trimmed.is_empty() {
            continue;
        }
        if !seen.insert(trimmed.to_string()) {
            continue;
        }
        out.push(trimmed.to_string());
    }
    out
}

fn empty_cells(columns: &[String]) -> Vec<CompareCellDto> {
    columns
        .iter()
        .map(|_| CompareCellDto {
            value: None,
            citation: None,
        })
        .collect()
}

fn empty_row(document: &ResolvedDocument, columns: &[String], error: String) -> CompareRowDto {
    CompareRowDto {
        document_id: document.id.clone(),
        title: document.title.clone(),
        file_path: document.file_path.clone(),
        cells: empty_cells(columns),
        error: Some(error),
        degraded: None,
    }
}

/// Assembles cells in `columns` order, so `cells.len() == columns.len()`
/// always — the invariant the table renderer relies on.
pub(super) fn assemble_cells(
    parsed: &super::parser::ParsedRow,
    columns: &[String],
    chunks: &[RetrievedChunk],
) -> Vec<CompareCellDto> {
    columns
        .iter()
        .map(|column| {
            let Some((value, quote)) = parsed.get(column) else {
                return CompareCellDto {
                    value: None,
                    citation: None,
                };
            };
            let citation = match (value.as_ref(), quote.as_ref()) {
                // A quote we cannot place is weak evidence, not proof of a
                // hallucination: keep the value, drop only the link.
                (Some(_), Some(quote)) => {
                    map_quote_to_chunk(quote, chunks).map(|chunk| CompareCitationDto {
                        chunk_id: chunk.chunk_id.clone(),
                        excerpt: quote.clone(),
                    })
                }
                _ => None,
            };
            CompareCellDto {
                value: value.clone(),
                citation,
            }
        })
        .collect()
}

/// The model's answer for one row, and the chunks its prompt carried.
///
/// The prompt carries each passage once, the best ranked first when the
/// window cannot hold them all; someone is waiting on the table.
pub(super) async fn fill_row(
    llm: &dyn LLMPort,
    title: &str,
    columns: &[String],
    chunks: &[RetrievedChunk],
) -> Result<(String, Vec<RetrievedChunk>)> {
    let mut request = GroundedRequest::new("", build_compare_task(title, columns));
    request.output_tokens = columns.len().saturating_mul(CELL_OUTPUT_TOKENS);
    request.evidence_heading = PASSAGES_HEADING.to_string();
    request.evidence = chunks
        .iter()
        .enumerate()
        .map(|(index, chunk)| EvidencePassage {
            id: chunk.chunk_id.clone(),
            label: format!("[{}]", index + 1),
            text: chunk.content.clone(),
            // Retrieval order is relevance order.
            rank: 1.0 / (index + 1) as f32,
        })
        .collect();
    request.selection = EvidenceSelection::BestFirst;
    request.call = CallOptions {
        priority: InferencePriority::Interactive,
        ..Default::default()
    };
    let output = grounded_generation::generate(llm, request).await?;
    debug!(
        carried = output.used_evidence_ids.len(),
        left_out = output.accounting.evicted.len(),
        input_tokens = output.accounting.total_input,
        "compare: row request planned"
    );
    let carried = chunks
        .iter()
        .filter(|chunk| output.used_evidence_ids.contains(&chunk.chunk_id))
        .cloned()
        .collect();
    Ok((output.text, carried))
}

/// One row: its columns filled a call at a time, as many per call as the
/// model's answer room holds.
pub(super) async fn build_row(
    deps: &CompareDeps,
    llm: &Arc<dyn LLMPort>,
    document: &ResolvedDocument,
    columns: &[String],
) -> Result<CompareRowDto> {
    let mut cells = Vec::with_capacity(columns.len());
    let mut degraded = None;
    let mut error = None;
    for group in columns.chunks(columns_per_call(llm.as_ref())) {
        let filled = tokio::time::timeout(
            Duration::from_secs(PER_CALL_TIMEOUT_SECS),
            fill_columns(deps, llm, document, group),
        )
        .await;
        let failure = match filled {
            Ok(Ok(Some((group_cells, group_degraded)))) => {
                cells.extend(group_cells);
                degraded = degraded.or(group_degraded);
                continue;
            }
            Ok(Ok(None)) => {
                return Ok(empty_row(
                    document,
                    columns,
                    "No indexed text for this document.".to_string(),
                ))
            }
            Ok(Err(error)) if cells.is_empty() => return Err(error),
            Ok(Err(error)) => error.to_string(),
            Err(_) => "Timed out.".to_string(),
        };
        // The cells earlier calls filled are kept; the columns not reached
        // are left empty and named, rather than the whole row thrown away.
        let missing = columns.get(cells.len()..).unwrap_or_default();
        error = Some(if cells.is_empty() {
            failure
        } else {
            format!("{failure} Not filled: {}.", missing.join(", "))
        });
        cells.extend(empty_cells(missing));
        break;
    }
    Ok(CompareRowDto {
        document_id: document.id.clone(),
        title: document.title.clone(),
        file_path: document.file_path.clone(),
        cells,
        error,
        degraded,
    })
}

/// The cells of `columns` for one document from one call, or `None` when the
/// document has no indexed text.
async fn fill_columns(
    deps: &CompareDeps,
    llm: &Arc<dyn LLMPort>,
    document: &ResolvedDocument,
    columns: &[String],
) -> Result<Option<(Vec<CompareCellDto>, Option<String>)>> {
    // Each column reads as deep as its share of the call's evidence room
    // holds, at the chunker's passage size.
    let mut task = GroundedRequest::new("", build_compare_task(&document.title, columns));
    task.output_tokens = columns.len().saturating_mul(CELL_OUTPUT_TOKENS);
    let room = grounded_generation::evidence_room(llm.as_ref(), &task).map_err(AppError::from)?;
    let per_column = grounded_generation::retrieval_depth(room, None, llm.chars_per_token())
        .div_ceil(columns.len().max(1));
    let super::retrieval::DocumentChunks { chunks, degraded } =
        retrieve_for_document(deps, &document.id, columns, per_column).await?;
    if chunks.is_empty() {
        return Ok(None);
    }

    // A quote is only mapped to a chunk the model was actually shown.
    let (raw, chunks) = fill_row(llm.as_ref(), &document.title, columns, &chunks).await?;

    let parsed = parse_compare_response(&raw, columns);
    if parsed.is_empty() {
        debug!(
            document_id = document.id.as_str(),
            "compare: no column could be read from the model response"
        );
    }
    Ok(Some((assemble_cells(&parsed, columns, &chunks), degraded)))
}

pub async fn compare_documents_impl(
    deps: &CompareDeps,
    request: CompareDocumentsRequestDto,
) -> Result<CompareTableDto> {
    let columns = sanitize_columns(request.columns);
    if columns.is_empty() {
        return Err(AppError::InvalidInput(
            "Name at least one column.".to_string(),
        ));
    }

    let document_ids = sanitize_document_ids(request.document_ids);
    if document_ids.len() < 2 {
        return Err(AppError::InvalidInput(
            "Select at least two documents to compare.".to_string(),
        ));
    }

    // Resolve titles once. A missing document is a row, never a hard failure.
    let repository = &deps.documents;
    let mut documents: Vec<ResolvedDocument> = Vec::with_capacity(document_ids.len());
    for id in &document_ids {
        match RepositoryPort::find_by_id(repository.as_ref(), id).await {
            Ok(Some(document)) => documents.push(ResolvedDocument {
                id: id.clone(),
                title: document.file_name().to_string(),
                file_path: document.file_path().to_string_lossy().to_string(),
                error: None,
            }),
            Ok(None) | Err(_) => documents.push(ResolvedDocument {
                id: id.clone(),
                title: id.clone(),
                file_path: String::new(),
                error: Some("This document is no longer indexed.".to_string()),
            }),
        }
    }

    // Without a model there is no table: this is the one whole-request failure.
    let llm = deps.load_llm.load().await.map_err(|error| {
        debug!(%error, "compare: no language model available");
        AppError::ServiceNotAvailable(
            "No language model is loaded. Pick one in Settings → AI.".to_string(),
        )
    })?;
    let model_name = llm.model_name().to_string();

    let rows: Vec<CompareRowDto> = stream::iter(documents)
        .map(|document| {
            let llm = Arc::clone(&llm);
            let columns = columns.clone();
            async move {
                if let Some(error) = document.error.clone() {
                    return empty_row(&document, &columns, error);
                }
                match build_row(deps, &llm, &document, &columns).await {
                    Ok(row) => row,
                    Err(error) => empty_row(&document, &columns, error.to_string()),
                }
            }
        })
        .buffer_unordered(DOCUMENT_CONCURRENCY)
        .collect()
        .await;

    // `buffer_unordered` reorders. The user picked the order; honour it.
    let mut by_id: std::collections::HashMap<String, CompareRowDto> = rows
        .into_iter()
        .map(|row| (row.document_id.clone(), row))
        .collect();
    let ordered_rows: Vec<CompareRowDto> = document_ids
        .iter()
        .filter_map(|id| by_id.remove(id))
        .collect();

    Ok(CompareTableDto {
        columns,
        rows: ordered_rows,
        model_name,
        generated_at: now_db_timestamp(),
    })
}
