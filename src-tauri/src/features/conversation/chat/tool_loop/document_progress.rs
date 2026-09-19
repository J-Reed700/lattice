use std::collections::{HashMap, HashSet};

use crate::features::function_calling::{
    domain::FunctionResult,
    dto::{GetDocumentOutput, SemanticSearchOutput},
};
use crate::features::qa::dto::SourceDto;

#[derive(Default)]
pub(super) struct DocumentProgress(HashMap<String, HashSet<String>>);

impl DocumentProgress {
    pub(super) fn new(sources: &[SourceDto]) -> Self {
        let mut progress = Self::default();
        for source in sources {
            if source
                .document_id
                .starts_with(super::super::retrieval::WEB_SOURCE_PREFIX)
            {
                continue;
            }
            progress.add(&source.document_id, &source.content);
            if let Some(chunks) = &source.chunk_excerpts {
                for chunk in chunks {
                    progress.add(&source.document_id, &chunk.excerpt);
                }
            }
        }
        progress
    }

    fn add(&mut self, document: &str, text: &str) {
        if !document.trim().is_empty() && !text.trim().is_empty() {
            self.0
                .entry(document.to_string())
                .or_default()
                .insert(text.to_string());
        }
    }

    pub(super) fn record(&mut self, tool: &str, result: &FunctionResult) -> bool {
        if !result.success {
            return false;
        }
        let Some(data) = result.data.clone() else {
            return false;
        };
        match tool {
            "semantic_search" => {
                let Ok(output) = serde_json::from_value::<SemanticSearchOutput>(data) else {
                    return false;
                };
                let mut found = false;
                for item in output.results {
                    found |= !item.document_id.trim().is_empty() && !item.snippet.trim().is_empty();
                    self.add(&item.document_id, &item.snippet);
                }
                found
            }
            "get_document" => {
                let Ok(output) = serde_json::from_value::<GetDocumentOutput>(data) else {
                    return false;
                };
                let found =
                    !output.document_id.trim().is_empty() && !output.content.trim().is_empty();
                self.add(&output.document_id, &output.content);
                found
            }
            _ => false,
        }
    }

    pub(super) fn update_trace(&self, trace: &mut super::super::RetrievalTraceDto) {
        trace.files = self.0.len();
        trace.passages = self.0.values().map(HashSet::len).sum();
        trace.unavailable_reason = None;
        // Tool outputs report returned results, not the searched corpus size.
        // Preserve the original scope count instead of inventing a new one.
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn search(ids: &[&str]) -> FunctionResult {
        FunctionResult::success(json!({
            "results": ids.iter().map(|id| json!({
                "document_id": id, "filename": "manual.pdf", "file_path": "/manual.pdf",
                "mime_type": "application/pdf", "score": 0.8, "snippet": "Verified passage",
                "modified_at": "2026-09-15T00:00:00Z", "size_bytes": 100
            })).collect::<Vec<_>>(),
            "total_found": ids.len(), "search_time_ms": 1, "query": "MPEP"
        }))
    }

    #[test]
    fn tool_document_progress_recovers_initial_failure_and_deduplicates_repeated_searches() {
        let mut progress = DocumentProgress::default();
        let mut trace = super::super::super::RetrievalTraceDto {
            unavailable_reason: Some("empty scope".into()),
            scope: "vault".into(),
            ..Default::default()
        };
        for ids in [&["1", "2", "3", "4", "5", "6", "7"][..], &["1", "2"][..]] {
            assert!(progress.record("semantic_search", &search(ids)));
            progress.update_trace(&mut trace);
        }
        assert_eq!(trace.files, 7);
        assert_eq!(trace.passages, 7);
        assert_eq!(trace.searched_documents, 0);
        assert!(trace.unavailable_reason.is_none());
        let persisted = serde_json::to_value(trace).unwrap();
        assert!(persisted.get("unavailableReason").is_none());
    }

    #[test]
    fn tool_document_progress_does_not_clear_failure_for_empty_failed_or_web_results() {
        let mut progress = DocumentProgress::default();
        assert!(!progress.record("semantic_search", &search(&[])));
        assert!(!progress.record(
            "semantic_search",
            &FunctionResult::error("offline", "unavailable")
        ));
        assert!(!progress.record("web_search", &search(&["1"])));
        assert!(!progress.record("semantic_search", &FunctionResult::success(json!({}))));
    }

    #[test]
    fn tool_document_progress_recovers_after_reading_a_document() {
        let mut progress = DocumentProgress::default();
        assert!(progress.record(
            "get_document",
            &FunctionResult::success(json!({
                "document_id": "doc", "content": "Verified content", "content_truncated": false
            }))
        ));
        let mut trace = super::super::super::RetrievalTraceDto::default();
        progress.update_trace(&mut trace);
        assert_eq!((trace.files, trace.passages), (1, 1));
    }
}
