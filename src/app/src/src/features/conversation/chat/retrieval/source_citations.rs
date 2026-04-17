use std::collections::{HashMap, HashSet};

use tracing::{debug, warn};

use crate::features::function_calling::dto::WebSearchResult;
use crate::features::qa::dto::{SourceChunkExcerptDto, SourceDto};
use crate::application::dtos::search_dto::SearchResultDto;
use crate::interfaces::di::Container;
use crate::shared::text_utils::build_excerpt;

pub(super) fn deduplicate_sources(sources: Vec<SourceDto>) -> Vec<SourceDto> {
    let mut grouped_by_doc: HashMap<String, Vec<SourceDto>> = HashMap::new();

    for source in sources {
        grouped_by_doc
            .entry(source.document_id.clone())
            .or_default()
            .push(source);
    }

    let mut deduped: Vec<SourceDto> = Vec::with_capacity(grouped_by_doc.len());

    for mut doc_sources in grouped_by_doc.into_values() {
        doc_sources.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let mut representative = doc_sources.remove(0);

        let mut seen_chunks: HashSet<String> = HashSet::new();
        let mut chunk_excerpts: Vec<SourceChunkExcerptDto> = Vec::new();

        for source in std::iter::once(&representative).chain(doc_sources.iter()) {
            let Some(excerpt) = super::source_excerpt_text(source) else {
                continue;
            };

            let dedupe_key = format!("{}:{}", source.chunk_id, excerpt);
            if !seen_chunks.insert(dedupe_key) {
                continue;
            }

            chunk_excerpts.push(SourceChunkExcerptDto {
                chunk_id: source.chunk_id.clone(),
                excerpt,
                section: source.section.clone(),
                chunk_index: source.chunk_index,
                score: source.score,
                highlights: source.highlights.clone(),
            });
        }

        chunk_excerpts.sort_by(|a, b| match (a.chunk_index, b.chunk_index) {
            (Some(left), Some(right)) => left.cmp(&right),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => b
                .score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal),
        });

        representative.chunk_excerpts = if chunk_excerpts.is_empty() {
            None
        } else {
            Some(chunk_excerpts)
        };

        deduped.push(representative);
    }

    deduped.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    deduped
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
    for (id, result) in doc_ids.iter().zip(doc_results.into_iter()) {
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
            document_id: format!("web:{}", url),
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
