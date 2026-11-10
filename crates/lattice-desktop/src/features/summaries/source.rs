//! What the summarizer reads.
//!
//! A document's text is reassembled from its stored chunks — `documents` has no
//! content column — and grouped by the *top-level* segment of each chunk's
//! section breadcrumb (`prepare_structured` writes "Parent > Child",
//! so "Parent" is the section a reader would call a section).
//!
//! The whole text is read: the summarizer describes a long document from all
//! of it, part by part, rather than from its opening.

use crate::features::summaries::repository;
use async_trait::async_trait;
use lattice_core::shared::error::Result;
use lattice_core::shared::persistence::Database;

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
    /// The text of chunks under no section heading, in chunk order: what a
    /// document summary built from the section summaries still has to read.
    pub unsectioned: String,
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
    let mut unsectioned = String::new();
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
            unsectioned.push_str(&content);
            unsectioned.push('\n');
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
        unsectioned: unsectioned.trim().to_owned(),
    }
}

pub struct SqliteSummarySource {
    database: Database,
}

impl SqliteSummarySource {
    pub fn new(database: Database) -> Self {
        Self { database }
    }
}

#[async_trait]
impl SummarySourcePort for SqliteSummarySource {
    async fn load(&self, document_id: &str) -> Result<Option<SummarySource>> {
        // The newest cluster run wins; older runs describe a corpus shape that
        // no longer exists. Absent clustering simply contributes no label.
        let Some(parts) = repository::source_parts(&self.database, document_id).await? else {
            return Ok(None);
        };
        Ok(Some(assemble(
            document_id,
            &parts.title,
            parts.cluster_label,
            parts.chunks,
        )))
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

    /// The body and every section are read whole.
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
