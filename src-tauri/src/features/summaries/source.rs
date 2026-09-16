//! What the summarizer reads.
//!
//! A document's text is reassembled from its stored chunks — `documents` has no
//! content column — and grouped by the *top-level* segment of each chunk's
//! section breadcrumb (`prepare_structured_with_spans` writes "Parent > Child",
//! so "Parent" is the section a reader would call a section).
//!
//! Reads are bounded here rather than in the use case: a 900-page manual must
//! not be pulled into memory just to write three sentences about it.

use async_trait::async_trait;
use sqlx::SqlitePool;

use crate::shared::error::{AppError, Result};

/// Beginning-of-document text read for the document prompt, before the real
/// tokenizer trims it to [`crate::features::summaries::prompt::DOCUMENT_BODY_TOKENS`].
/// Four characters per token with headroom.
const DOCUMENT_BODY_CHARS: usize = 24_000;

/// Per-section text read, before the tokenizer trims it to
/// [`crate::features::summaries::prompt::SECTION_BODY_TOKENS`].
const SECTION_BODY_CHARS: usize = 12_000;

/// Chunks read per document. Far more than any prompt can use; the cap only
/// exists so a pathological document cannot stall the background task.
const MAX_CHUNKS: i64 = 4_000;

/// One top-level section's text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SummarySection {
    pub heading: String,
    pub text: String,
}

/// Everything the summarizer needs about one document.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SummarySource {
    pub document_id: String,
    pub title: String,
    /// Beginning of the document, bounded by [`DOCUMENT_BODY_CHARS`].
    pub body: String,
    /// The document's `corpus_shape` cluster label from the most recent run.
    pub cluster_label: Option<String>,
    pub sections: Vec<SummarySection>,
}

/// Port for loading summarizer input. Swapped for a fixture in tests.
#[async_trait]
pub trait SummarySourcePort: Send + Sync {
    async fn load(&self, document_id: &str) -> Result<Option<SummarySource>>;
}

/// Group `(section, content)` chunk rows, in `chunk_index` order, into the
/// document body and its top-level sections.
///
/// Pure so the grouping rules are testable without a database.
pub fn assemble(
    document_id: &str,
    title: &str,
    cluster_label: Option<String>,
    chunks: impl IntoIterator<Item = (Option<String>, String)>,
) -> SummarySource {
    let mut body = String::new();
    let mut sections: Vec<SummarySection> = Vec::new();
    for (section, content) in chunks {
        // Byte length bounds char count from above, so this guard is cheap
        // and conservative; the exact character cap is applied once at the end.
        if body.len() < DOCUMENT_BODY_CHARS {
            body.push_str(&content);
            body.push('\n');
        }
        let Some(heading) = section
            .as_deref()
            .and_then(|s| s.split(" > ").next())
            .map(str::trim)
            .filter(|s| !s.is_empty())
        else {
            continue;
        };
        match sections.iter_mut().find(|s| s.heading == heading) {
            Some(existing) => {
                if existing.text.len() < SECTION_BODY_CHARS {
                    existing.text.push_str(&content);
                    existing.text.push('\n');
                }
            }
            None => sections.push(SummarySection {
                heading: heading.to_owned(),
                text: format!("{content}\n"),
            }),
        }
    }
    body.truncate(
        body.char_indices()
            .nth(DOCUMENT_BODY_CHARS)
            .map_or(body.len(), |(index, _)| index),
    );
    SummarySource {
        document_id: document_id.to_owned(),
        title: title.to_owned(),
        body: body.trim().to_owned(),
        cluster_label,
        sections,
    }
}

pub struct SqliteSummarySource {
    pool: SqlitePool,
}

impl SqliteSummarySource {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl SummarySourcePort for SqliteSummarySource {
    async fn load(&self, document_id: &str) -> Result<Option<SummarySource>> {
        let Some(title): Option<String> =
            sqlx::query_scalar("SELECT file_name FROM documents WHERE id = ?")
                .bind(document_id)
                .fetch_optional(&self.pool)
                .await
                .map_err(|e| AppError::Database(format!("summary source title: {e}")))?
        else {
            return Ok(None);
        };
        // The newest cluster run wins; older runs describe a corpus shape that
        // no longer exists. Absent clustering simply contributes no label.
        let cluster_label: Option<String> = sqlx::query_scalar(
            "SELECT c.label FROM cluster_members cm JOIN clusters c ON c.id = cm.cluster_id JOIN cluster_runs r ON r.id = c.run_id WHERE cm.document_id = ? ORDER BY r.ran_at DESC LIMIT 1",
        )
        .bind(document_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AppError::Database(format!("summary source cluster label: {e}")))?;
        let chunks: Vec<(Option<String>, String)> = sqlx::query_as(
            "SELECT section, content FROM text_chunks WHERE document_id = ? ORDER BY chunk_index LIMIT ?",
        )
        .bind(document_id)
        .bind(MAX_CHUNKS)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AppError::Database(format!("summary source chunks: {e}")))?;
        Ok(Some(assemble(document_id, &title, cluster_label, chunks)))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    #[test]
    fn breadcrumbs_group_under_their_top_level_section() {
        let source = assemble(
            "doc-1",
            "Manual",
            Some("Aviation".into()),
            [
                (Some("Intro".into()), "opening".into()),
                (Some("Safety > Fire".into()), "fire".into()),
                (Some("Safety > Water".into()), "water".into()),
                (None, "loose".into()),
            ],
        );
        assert_eq!(source.title, "Manual");
        assert_eq!(source.cluster_label.as_deref(), Some("Aviation"));
        assert_eq!(
            source
                .sections
                .iter()
                .map(|s| s.heading.as_str())
                .collect::<Vec<_>>(),
            vec!["Intro", "Safety"]
        );
        assert!(source.sections[1].text.contains("fire"));
        assert!(source.sections[1].text.contains("water"));
        assert!(source.body.contains("loose"));
    }

    #[test]
    fn body_is_bounded_and_never_splits_a_character() {
        let chunks: Vec<(Option<String>, String)> =
            (0..40).map(|_| (None, "é".repeat(1_000))).collect();
        let source = assemble("doc-1", "Big", None, chunks);
        assert!(source.body.chars().count() <= DOCUMENT_BODY_CHARS);
        assert!(source.body.chars().all(|c| c == 'é' || c == '\n'));
    }

    #[test]
    fn a_document_with_no_chunks_still_loads() {
        let source = assemble("doc-1", "Empty", None, []);
        assert!(source.body.is_empty());
        assert!(source.sections.is_empty());
    }
}
