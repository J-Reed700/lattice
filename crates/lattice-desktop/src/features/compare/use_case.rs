//! The compare pipeline: validate → resolve → retrieve → one grounded call per
//! document → map the cited sentences back to their chunks → assemble.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use futures::{stream, StreamExt};
use tracing::debug;

use crate::features::compare::di::CompareDeps;
use lattice_core::application::ports::llm_port::InferencePriority;
use lattice_core::application::ports::LLMPort;
use lattice_core::application::services::grounded_generation::{self, CallOptions};
use lattice_core::application::services::model_contract::ask::{ask_each, room, Plain, Settled};
use lattice_core::application::services::model_contract::packed::Packed;
use lattice_core::application::services::model_contract::shown::{Kind, Scope, Told};
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::persistence::timestamps::now_db_timestamp;

use super::dto::{
    CompareCellDto, CompareCitationDto, CompareDocumentsRequestDto, CompareRowDto, CompareTableDto,
};
use super::prompt::{Answer, Cell, Column, CompareRow, Input, Sentence};
use super::retrieval::retrieve_for_document;

pub const DOCUMENT_CONCURRENCY: usize = 2;
/// How long one row call may take.
pub const PER_CALL_TIMEOUT_SECS: u64 = 120;

#[derive(Debug, Clone)]
pub(super) struct ResolvedDocument {
    pub(super) id: String,
    pub(super) title: String,
    pub(super) file_path: String,
    pub(super) error: Option<String>,
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

/// The cell the model's answer for one column makes: its value, and the
/// first sentence it cites, with the chunk that sentence is of.
fn cell_of(cell: &Cell, held: &[Sentence]) -> CompareCellDto {
    match &cell.answer {
        Answer::Stated { value, support } => CompareCellDto {
            value: Some(value.trim().to_string()),
            citation: support.iter().find_map(|picked| {
                held.iter()
                    .find(|sentence| sentence.reference() == picked.reference)
                    .map(|sentence| CompareCitationDto {
                        chunk_id: sentence.chunk_id.clone(),
                        excerpt: sentence.text.0.clone(),
                    })
            }),
        },
        Answer::NotStated => CompareCellDto {
            value: None,
            citation: None,
        },
    }
}

/// One row: its columns filled by one asking, which the driver halves as the
/// model's window needs.
pub(super) async fn build_row(
    deps: &CompareDeps,
    llm: &Arc<dyn LLMPort>,
    document: &ResolvedDocument,
    columns: &[String],
) -> Result<CompareRowDto> {
    let filled = tokio::time::timeout(
        Duration::from_secs(PER_CALL_TIMEOUT_SECS),
        fill_columns(deps, llm, document, columns),
    )
    .await;
    let filled = match filled {
        Ok(Ok(Some(filled))) => filled,
        Ok(Ok(None)) => {
            return Ok(empty_row(
                document,
                columns,
                "No indexed text for this document.".to_string(),
            ))
        }
        Ok(Err(error)) => return Err(error),
        Err(_) => return Ok(empty_row(document, columns, "Timed out.".to_string())),
    };
    let error = (!filled.unanswered.is_empty()).then(|| {
        format!(
            "The model gave no usable answer for: {}.",
            filled.unanswered.join(", ")
        )
    });
    Ok(CompareRowDto {
        document_id: document.id.clone(),
        title: document.title.clone(),
        file_path: document.file_path.clone(),
        cells: filled.cells,
        error,
        degraded: filled.degraded,
    })
}

/// The cells of one document's row, the columns the model gave no usable
/// answer for, and the note of a degraded search.
struct Filled {
    cells: Vec<CompareCellDto>,
    unanswered: Vec<String>,
    degraded: Option<String>,
}

/// The cells of `columns` for one document, or `None` when the document has
/// no indexed text.
async fn fill_columns(
    deps: &CompareDeps,
    llm: &Arc<dyn LLMPort>,
    document: &ResolvedDocument,
    columns: &[String],
) -> Result<Option<Filled>> {
    let call = CallOptions {
        priority: InferencePriority::Interactive,
        ..Default::default()
    };
    let title = Told(document.title.clone());
    let items: Vec<Column> = columns
        .iter()
        .enumerate()
        .map(|(index, name)| Column {
            index,
            name: Told(name.clone()),
        })
        .collect();

    // Each column reads as deep as its share of the call's evidence room
    // holds, at the chunker's passage size.
    let bare = Input {
        title: title.clone(),
        columns: items.clone(),
        passages: Packed::default(),
    };
    let evidence_room = room::<CompareRow>(llm.as_ref(), &bare, &call)?;
    let per_column =
        grounded_generation::retrieval_depth(evidence_room, None, llm.chars_per_token())
            .div_ceil(columns.len().max(1));
    let super::retrieval::DocumentChunks { chunks, degraded } =
        retrieve_for_document(deps, &document.id, columns, per_column).await?;
    if chunks.is_empty() {
        return Ok(None);
    }

    let passages = Input::passages_of(&chunks, llm.chars_per_token());
    let held: Vec<Sentence> = passages
        .iter()
        .flat_map(|passage| passage.sentences.iter().cloned())
        .collect();
    let mut scope = Scope::for_question();
    let settled = ask_each::<CompareRow>(
        llm.as_ref(),
        &Plain,
        &mut scope,
        &items,
        |open, _| Input {
            title: title.clone(),
            columns: open.iter().map(|column| (*column).clone()).collect(),
            passages: passages.clone(),
        },
        &call,
    )
    .await?;

    let mut cells = Vec::with_capacity(columns.len());
    let mut unanswered = Vec::new();
    for (column, settled) in columns.iter().zip(settled) {
        match settled {
            Settled::Answered(cell) => cells.push(cell_of(&cell, &held)),
            Settled::Unanswered(_) => {
                unanswered.push(column.clone());
                cells.push(CompareCellDto {
                    value: None,
                    citation: None,
                });
            }
        }
    }
    Ok(Some(Filled {
        cells,
        unanswered,
        degraded,
    }))
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
    let mut metadata: HashMap<_, _> = repository
        .find_metadata_by_ids(&document_ids)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|document| (document.id().as_str().to_owned(), document))
        .collect();
    let mut documents: Vec<ResolvedDocument> = Vec::with_capacity(document_ids.len());
    for id in &document_ids {
        match metadata.remove(id) {
            Some(document) => documents.push(ResolvedDocument {
                id: id.clone(),
                title: document.file_name().to_string(),
                file_path: document.file_path().to_string_lossy().to_string(),
                error: None,
            }),
            None => documents.push(ResolvedDocument {
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
