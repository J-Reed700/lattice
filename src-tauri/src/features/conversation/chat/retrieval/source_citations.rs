use std::collections::{HashMap, HashSet};

use tracing::{debug, warn};

use crate::features::function_calling::dto::WebSearchResult;
use crate::features::qa::dto::SourceDto;
use crate::features::search::dto::SearchResultDto;
use crate::interfaces::di::Container;
use crate::shared::text_utils::build_excerpt;

/// Marks a `SourceDto` that came from the web rather than the user's vault.
///
/// A web result is shaped like a document source so citations can treat both
/// alike, which means the id prefix is the only thing telling them apart.
/// Anything that counts, scopes or filters vault documents must check it.
pub const WEB_SOURCE_PREFIX: &str = "web:";

/// Number the sources the user will see, so the prompt can cite the same
/// numbers.
///
/// This must be the *only* place citation numbers are assigned, and it must
/// run after every step that reorders or filters the list (dedup, score sort,
/// appending tool/web sources). Numbering in two places is what made
/// footnotes open the wrong document.
pub(super) fn assign_citation_ids(sources: &mut [SourceDto]) {
    let mut next = sources
        .iter()
        .filter_map(|s| s.citation_id)
        .max()
        .unwrap_or(0)
        + 1;
    for source in sources {
        if source.citation_id.is_none() {
            source.citation_id = Some(next);
            next += 1;
        }
    }
}

/// Map each chunk id to the citation number the model was given.
pub(super) fn citation_ids_by_chunk(sources: &[SourceDto]) -> HashMap<String, u32> {
    sources
        .iter()
        .filter_map(|source| source.citation_id.map(|id| (source.chunk_id.clone(), id)))
        .collect()
}

/// Fold sources produced by a tool call into the turn's source list, returning
/// the chunk ids the caller should quote back to the model as citable passages.
///
/// A web page is one source, no matter how often the turn reaches for it. The
/// search results give a URL as a snippet; `fetch_url_content` then gives the
/// same URL as full text. Appending both leaves two entries that the UI groups
/// back into one card by URL — the list says ten sources while the citation
/// numbers run to eleven, so the last footnote points at nothing the reader can
/// open. Upgrading the entry in place keeps the number the model was already
/// given and swaps a snippet for the article.
///
/// Only web sources fold this way. Two passages of the same vault document are
/// genuinely two sources, and collapsing those would throw evidence away.
pub(super) fn merge_tool_sources(
    sources: &mut Vec<SourceDto>,
    incoming: Vec<SourceDto>,
) -> HashSet<String> {
    let mut citable_chunk_ids = HashSet::new();

    for source in incoming {
        let existing = if is_web_document_id(&source.document_id) {
            sources
                .iter_mut()
                .find(|candidate| candidate.document_id == source.document_id)
        } else {
            None
        };

        match existing {
            Some(existing) => {
                citable_chunk_ids.insert(existing.chunk_id.clone());
                // The rank, the number and the identity stay; only the evidence
                // improves, and only when the newcomer actually carries more.
                if source.content.len() > existing.content.len() {
                    existing.content = source.content;
                    existing.excerpt = source.excerpt;
                    existing.file_size_bytes = source.file_size_bytes;
                    existing.mime_type = source.mime_type;
                    if existing.file_name == existing.file_path && !source.file_name.is_empty() {
                        existing.file_name = source.file_name;
                    }
                }
            }
            None => {
                citable_chunk_ids.insert(source.chunk_id.clone());
                sources.push(source);
            }
        }
    }

    citable_chunk_ids
}

/// Web sources are keyed by their URL, so one document id means one page.
fn is_web_document_id(document_id: &str) -> bool {
    document_id.starts_with("web:")
}

pub(super) fn deduplicate_sources(sources: Vec<SourceDto>) -> Vec<SourceDto> {
    // Keep separate passages from the same file and preserve numbers already
    // exposed to the model when tool calls append evidence.
    let mut seen = HashSet::new();
    sources
        .into_iter()
        .filter(|source| seen.insert((source.document_id.clone(), source.chunk_id.clone())))
        .collect()
}

pub(super) async fn build_source_citations(
    results: &[SearchResultDto],
    container: &Container,
    highlight_terms: &[String],
) -> Vec<SourceDto> {
    if results.is_empty() {
        return Vec::new();
    }

    let doc_repo = container.document_repository();
    let chunk_repo = container.chunk_repository();

    let mut doc_ids = HashSet::new();
    for doc_id in results.iter().filter_map(|r| r.document_id.as_deref()) {
        if !doc_id.is_empty() {
            doc_ids.insert(doc_id);
        }
    }
    let doc_ids: Vec<&str> = doc_ids.into_iter().collect();

    let chunk_ids: Vec<String> = results.iter().map(|r| r.id.clone()).collect();

    let doc_futures: Vec<_> = doc_ids.iter().map(|id| doc_repo.find_by_id(id)).collect();
    let doc_results = futures::future::join_all(doc_futures).await;

    let mut doc_map = HashMap::new();
    for (id, result) in doc_ids.iter().zip(doc_results) {
        match result {
            Ok(Some(doc)) => {
                doc_map.insert(*id, doc);
            }
            Ok(None) => {
                debug!(doc_id = %id, "Document not found for source citation");
            }
            Err(e) => {
                warn!(error = %e, doc_id = %id, "Failed to fetch document metadata for citation");
            }
        }
    }

    let mut chunk_map = HashMap::new();
    match chunk_repo.find_by_ids(&chunk_ids).await {
        Ok(chunks) => {
            for chunk in chunks {
                chunk_map.insert(chunk.id().as_str().to_string(), chunk);
            }
        }
        Err(e) => {
            warn!(error = %e, "Failed to fetch chunk metadata for citation batch");
        }
    }

    let mut sources = Vec::with_capacity(results.len());
    for result in results {
        let doc_id = result.document_id.as_deref().unwrap_or(&result.id);
        let document = doc_map.get(doc_id);

        let (file_path, file_name, mime_type, file_size_bytes, modified_at) =
            if let Some(doc) = document {
                (
                    doc.file_path().display().to_string(),
                    doc.file_name().to_string(),
                    doc.mime_type().to_string(),
                    doc.size_bytes(),
                    doc.modified_at().to_rfc3339(),
                )
            } else {
                let fallback_path = result.path.as_deref().unwrap_or("");
                let fallback_name = std::path::Path::new(fallback_path)
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or(&result.title)
                    .to_string();
                (
                    fallback_path.to_string(),
                    fallback_name,
                    String::new(),
                    0,
                    String::new(),
                )
            };

        let category = infer_category(&file_path);

        let chunk_meta = chunk_map.get(&result.id);
        let section = chunk_meta.and_then(|chunk| chunk.section().map(|s| s.to_string()));
        let chunk_index = chunk_meta.map(|chunk| chunk.index());

        let excerpt = if result.content.trim().is_empty() {
            None
        } else {
            Some(build_excerpt(&result.content, highlight_terms, 480))
        };

        let highlights = if highlight_terms.is_empty() {
            None
        } else {
            Some(highlight_terms.to_vec())
        };

        sources.push(SourceDto {
            page_number: chunk_meta.and_then(|c| c.page_number()),
            document_id: doc_id.to_string(),
            chunk_id: result.id.clone(),
            content: result.content.clone(),
            score: result.score,
            path: if file_path.is_empty() {
                None
            } else {
                Some(file_path.clone())
            },
            position: result.position,
            file_name,
            file_path,
            mime_type,
            category,
            file_size_bytes,
            modified_at,
            excerpt,
            highlights,
            section,
            chunk_index,
            chunk_excerpts: None,
            citation_id: None,
        });
    }
    sources
}

pub(super) fn build_web_source_citations(
    results: &[WebSearchResult],
    highlight_terms: &[String],
    excerpt_chars: usize,
) -> Vec<SourceDto> {
    if results.is_empty() {
        return Vec::new();
    }

    let mut seen_urls: HashSet<&str> = HashSet::new();
    let total = results.len() as f32;
    let mut sources = Vec::with_capacity(results.len());

    for (idx, result) in results.iter().enumerate() {
        let url = result.url.trim();
        if url.is_empty() || !seen_urls.insert(url) {
            continue;
        }

        let content = result.snippet.trim().to_string();
        let excerpt = if content.is_empty() {
            None
        } else {
            Some(build_excerpt(&content, highlight_terms, excerpt_chars))
        };

        let highlights = if highlight_terms.is_empty() {
            None
        } else {
            Some(highlight_terms.to_vec())
        };

        let published_at = result
            .published_date
            .map(|d| d.to_rfc3339())
            .unwrap_or_else(|| chrono::Utc::now().to_rfc3339());

        let score = ((results.len() - idx) as f32) / total.max(1.0);
        let title = result.title.trim();
        let file_name = if title.is_empty() {
            url.to_string()
        } else {
            title.to_string()
        };

        sources.push(SourceDto {
            page_number: None,
            document_id: format!("{WEB_SOURCE_PREFIX}{url}"),
            chunk_id: format!("web-result-{}", idx + 1),
            content,
            score,
            path: Some(url.to_string()),
            position: Some(idx + 1),
            file_name,
            file_path: url.to_string(),
            mime_type: "text/html".to_string(),
            category: "Web Article".to_string(),
            file_size_bytes: 0,
            modified_at: published_at,
            excerpt,
            highlights,
            section: None,
            chunk_index: Some(idx + 1),
            chunk_excerpts: None,
            citation_id: None,
        });
    }

    sources
}

pub(super) fn infer_category(path: &str) -> String {
    let ext = std::path::Path::new(path)
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_lowercase();

    match ext.as_str() {
        "pdf" => "PDF Document".to_string(),
        "md" | "markdown" => "Markdown".to_string(),
        "txt" | "text" => "Text File".to_string(),
        "rs" => "Rust Source".to_string(),
        "py" => "Python Source".to_string(),
        "js" | "jsx" => "JavaScript".to_string(),
        "ts" | "tsx" => "TypeScript".to_string(),
        "html" | "htm" => "HTML Document".to_string(),
        "json" => "JSON".to_string(),
        "yaml" | "yml" => "YAML".to_string(),
        "toml" => "TOML".to_string(),
        "csv" => "CSV".to_string(),
        "doc" | "docx" => "Word Document".to_string(),
        "xls" | "xlsx" => "Spreadsheet".to_string(),
        "ppt" | "pptx" => "Presentation".to_string(),
        "png" | "jpg" | "jpeg" | "gif" | "svg" | "webp" => "Image".to_string(),
        _ if ext.is_empty() => "Unknown".to_string(),
        other => format!("{} File", other.to_uppercase()),
    }
}

#[cfg(test)]
mod merge_tool_sources_tests {
    use super::*;

    fn source(document_id: &str, chunk_id: &str, content: &str) -> SourceDto {
        SourceDto {
            page_number: None,
            document_id: document_id.to_string(),
            chunk_id: chunk_id.to_string(),
            content: content.to_string(),
            score: 1.0,
            path: Some(document_id.to_string()),
            position: Some(1),
            file_name: document_id.to_string(),
            file_path: document_id.to_string(),
            mime_type: "text/html".to_string(),
            category: "Web Article".to_string(),
            file_size_bytes: 0,
            modified_at: String::new(),
            excerpt: None,
            highlights: None,
            section: None,
            chunk_index: Some(1),
            chunk_excerpts: None,
            citation_id: None,
        }
    }

    /// The reported bug: the search snippet and the fetched article are the same
    /// page, so they must stay one entry with one number. Two entries render as
    /// one card, and the eleventh footnote then opens nothing.
    #[test]
    fn fetching_a_page_already_in_the_list_upgrades_it_instead_of_adding_a_source() {
        let mut sources = vec![source(
            "web:https://example.com/a",
            "web-result-1",
            "snippet",
        )];
        sources[0].citation_id = Some(1);

        let citable = merge_tool_sources(
            &mut sources,
            vec![source(
                "web:https://example.com/a",
                "web-content-1",
                "the full article text",
            )],
        );

        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].citation_id, Some(1));
        assert_eq!(sources[0].chunk_id, "web-result-1");
        assert_eq!(sources[0].content, "the full article text");
        assert!(citable.contains("web-result-1"));
    }

    #[test]
    fn a_shorter_fetch_does_not_replace_the_evidence_already_held() {
        let mut sources = vec![source(
            "web:https://example.com/a",
            "web-result-1",
            "a long and detailed snippet",
        )];

        merge_tool_sources(
            &mut sources,
            vec![source(
                "web:https://example.com/a",
                "web-content-1",
                "short",
            )],
        );

        assert_eq!(sources[0].content, "a long and detailed snippet");
    }

    #[test]
    fn a_new_page_is_still_appended() {
        let mut sources = vec![source(
            "web:https://example.com/a",
            "web-result-1",
            "snippet",
        )];

        let citable = merge_tool_sources(
            &mut sources,
            vec![source(
                "web:https://example.com/b",
                "web-content-1",
                "other",
            )],
        );

        assert_eq!(sources.len(), 2);
        assert!(citable.contains("web-content-1"));
    }

    /// Two passages of one vault document are two pieces of evidence; folding
    /// them together would silently drop one.
    #[test]
    fn vault_chunks_of_the_same_document_are_kept_apart() {
        let mut sources = vec![source("doc-1", "chunk-1", "first passage")];

        merge_tool_sources(
            &mut sources,
            vec![source("doc-1", "chunk-2", "a much longer second passage")],
        );

        assert_eq!(sources.len(), 2);
    }
}
