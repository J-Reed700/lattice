//! What the summarizer reads.
//!
//! A document's text is reassembled from its stored chunks — `documents` has no
//! content column — and grouped by the *top-level* segment of each chunk's
//! section breadcrumb (`prepare_structured_with_spans` writes "Parent > Child",
//! so "Parent" is the section a reader would call a section).
//!
//! The whole text is read: the summarizer describes a long document from all
//! of it, part by part, rather than from its opening.

use async_trait::async_trait;
use sqlx::SqlitePool;

use lattice_core::shared::error::{AppError, Result};

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
    /// The document's whole text, in chunk order.
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
        body.push_str(&content);
        body.push('\n');
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
                existing.text.push_str(&content);
                existing.text.push('\n');
            }
            None => sections.push(SummarySection {
                heading: heading.to_owned(),
                text: format!("{content}\n"),
            }),
        }
    }
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
            "SELECT section, content FROM text_chunks WHERE document_id = ? ORDER BY chunk_index",
        )
        .bind(document_id)
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

    /// The body used to stop at 24,000 characters and each section at
    /// 12,000. Both are read whole.
    #[test]
    fn body_and_sections_are_read_whole() {
        let chunks: Vec<(Option<String>, String)> = (0..40)
            .map(|_| (Some("Only".to_string()), "é".repeat(1_000)))
            .collect();
        let source = assemble("doc-1", "Big", None, chunks);
        assert_eq!(source.body.chars().filter(|c| *c == 'é').count(), 40_000);
        assert_eq!(
            source.sections[0]
                .text
                .chars()
                .filter(|c| *c == 'é')
                .count(),
            40_000
        );
    }

    #[test]
    fn a_document_with_no_chunks_still_loads() {
        let source = assemble("doc-1", "Empty", None, []);
        assert!(source.body.is_empty());
        assert!(source.sections.is_empty());
    }
}
