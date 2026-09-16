//! The compare pipeline: validate → resolve → retrieve → one prompt per
//! document → tolerant parse → map quotes back to chunks → assemble.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use futures::{stream, StreamExt};
use tracing::debug;

use crate::application::ports::{LLMPort, RepositoryPort};
use crate::interfaces::di::Container;
use crate::shared::error::{AppError, Result};
use crate::shared::time::now_db_timestamp;

use super::dto::{
    CompareCellDto, CompareCitationDto, CompareDocumentsRequestDto, CompareRowDto, CompareTableDto,
};
use super::parser::parse_compare_response;
use super::prompt::build_compare_prompt;
use super::retrieval::{retrieve_for_document, truncate_chars, RetrievedChunk};

pub const MAX_DOCUMENTS: usize = 12;
pub const MAX_COLUMNS: usize = 6;
pub const MAX_COLUMN_CHARS: usize = 80;
pub const CHUNKS_PER_COLUMN: usize = 3;
pub const MAX_CHUNKS_PER_DOCUMENT: usize = 10;
pub const MAX_CHUNK_CHARS: usize = 1_200;
pub const MAX_CONTEXT_CHARS: usize = 9_000;
pub const MAX_VALUE_CHARS: usize = 400;
pub const MAX_EXCERPT_CHARS: usize = 320;
pub const DOCUMENT_CONCURRENCY: usize = 2;
pub const PER_DOCUMENT_TIMEOUT_SECS: u64 = 120;
pub const SEMANTIC_THRESHOLD: f32 = 0.25;

/// Minimum share of a quote's words that must appear in a chunk before the
/// citation is trusted. Catches a model that dropped a comma.
const TOKEN_OVERLAP_THRESHOLD: f32 = 0.6;

#[derive(Debug, Clone)]
struct ResolvedDocument {
    id: String,
    title: String,
    file_path: String,
    error: Option<String>,
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

/// Trims, drops empties, dedupes case-insensitively (keeping the first
/// spelling), truncates each entry and the list.
fn sanitize_columns(columns: Vec<String>) -> Vec<String> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut out: Vec<String> = Vec::new();
    for column in columns {
        let trimmed = column.trim();
        if trimmed.is_empty() {
            continue;
        }
        let truncated = truncate_chars(trimmed, MAX_COLUMN_CHARS);
        if !seen.insert(truncated.to_lowercase()) {
            continue;
        }
        out.push(truncated);
        if out.len() == MAX_COLUMNS {
            break;
        }
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
        if out.len() == MAX_DOCUMENTS {
            break;
        }
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
                        excerpt: truncate_chars(quote, MAX_EXCERPT_CHARS),
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

async fn build_row(
    container: &Container,
    llm: &Arc<dyn LLMPort>,
    document: &ResolvedDocument,
    columns: &[String],
) -> Result<CompareRowDto> {
    let chunks = retrieve_for_document(container, &document.id, columns).await?;
    if chunks.is_empty() {
        return Ok(empty_row(
            document,
            columns,
            "No indexed text for this document.".to_string(),
        ));
    }

    let prompt = build_compare_prompt(&document.title, columns, &chunks);
    let context: Vec<String> = chunks.iter().map(|chunk| chunk.content.clone()).collect();
    let raw = llm.generate(&prompt, &context, None).await?;

    let parsed = parse_compare_response(&raw, columns);
    if parsed.is_empty() {
        debug!(
            document_id = document.id.as_str(),
            "compare: no column could be read from the model response"
        );
    }

    Ok(CompareRowDto {
        document_id: document.id.clone(),
        title: document.title.clone(),
        file_path: document.file_path.clone(),
        cells: assemble_cells(&parsed, columns, &chunks),
        error: None,
    })
}

pub async fn compare_documents_impl(
    container: &Container,
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
    let repository = container.document_repository();
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
    let llm = container.get_or_load_llm().await.map_err(|error| {
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
                match tokio::time::timeout(
                    Duration::from_secs(PER_DOCUMENT_TIMEOUT_SECS),
                    build_row(container, &llm, &document, &columns),
                )
                .await
                {
                    Ok(Ok(row)) => row,
                    Ok(Err(error)) => empty_row(&document, &columns, error.to_string()),
                    Err(_) => empty_row(&document, &columns, "Timed out.".to_string()),
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
