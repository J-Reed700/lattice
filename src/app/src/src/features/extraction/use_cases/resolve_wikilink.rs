//! Resolve Wikilink Use Case
//!
//! Resolves wikilink targets to document IDs/paths with confidence scoring.
//!
//! # Resolution Strategy
//! 1. **Exact path match**: `[[path/to/file.md]]` → 1.0 confidence
//! 2. **Path without extension**: `[[path/to/file]]` → 1.0 confidence
//! 3. **Fuzzy filename match**: `[[file]]` → 0.8 confidence
//! 4. **Title match**: Search document titles → 0.6 confidence
//! 5. **No match**: None → 0.0 confidence
//!
//! # Example
//! ```rust,no_run
//! let use_case = ResolveWikilinkUseCase::new();
//! let request = ResolveWikilinkRequestDto {
//!     link_target: "todo".into(),
//!     source_document_id: Some("doc-1".into()),
//!     available_documents: vec![
//!         DocumentRefDto {
//!             document_id: "doc-2".into(),
//!             file_path: "notes/todo.md".into(),
//!             title: Some("Todo List".into()),
//!         },
//!     ],
//! };
//! let result = use_case.execute(request).await?;
//! assert_eq!(result.confidence, 0.8);
//! ```

use crate::features::extraction::dto::{
    DocumentRefDto, ResolveWikilinkRequestDto, ResolveWikilinkResponseDto,
};
use crate::infrastructure::extraction::{DocumentInfo, LinkParser};
use crate::shared::error::AppError;

pub struct ResolveWikilinkUseCase {
    link_parser: LinkParser,
}

impl ResolveWikilinkUseCase {
    pub fn new() -> Self {
        Self {
            link_parser: LinkParser::new(),
        }
    }

    pub async fn execute(
        &self,
        request: ResolveWikilinkRequestDto,
    ) -> Result<ResolveWikilinkResponseDto, AppError> {
        tracing::debug!(
            link_target = %request.link_target,
            available_documents = request.available_documents.len(),
            "Resolving wikilink"
        );

        let document_infos: Vec<DocumentInfo> = request
            .available_documents
            .iter()
            .map(|doc| DocumentInfo {
                file_path: doc.file_path.clone(),
                title: doc.title.clone(),
            })
            .collect();

        let source_path = request
            .source_document_id
            .as_ref()
            .and_then(|id| {
                request
                    .available_documents
                    .iter()
                    .find(|doc| &doc.document_id == id)
                    .map(|doc| doc.file_path.as_str())
            })
            .unwrap_or("");

        let resolved_path =
            self.link_parser
                .resolve_link(&request.link_target, source_path, &document_infos);

        let (resolved_document_id, confidence) = if let Some(path) = &resolved_path {
            let doc = request
                .available_documents
                .iter()
                .find(|doc| &doc.file_path == path);

            if let Some(doc) = doc {
                let confidence = self.calculate_confidence(&request.link_target, doc);
                (Some(doc.document_id.clone()), confidence)
            } else {
                (None, 0.0)
            }
        } else {
            (None, 0.0)
        };

        tracing::info!(
            resolved_document_id = ?resolved_document_id,
            resolved_path = ?resolved_path,
            confidence = confidence,
            "Successfully resolved wikilink"
        );

        Ok(ResolveWikilinkResponseDto {
            resolved_document_id,
            resolved_path,
            confidence,
        })
    }

    fn calculate_confidence(&self, target: &str, doc: &DocumentRefDto) -> f32 {
        let target_lower = target.to_lowercase();
        let path_lower = doc.file_path.to_lowercase();

        // Exact path match or path without extension
        if path_lower == target_lower || path_lower == format!("{}.md", target_lower) {
            return 1.0;
        }

        // Title match
        if let Some(title) = &doc.title {
            if title.to_lowercase() == target_lower {
                return 0.6;
            }
        }

        // Fuzzy filename match (everything else that resolved)
        0.8
    }
}

impl Default for ResolveWikilinkUseCase {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_documents() -> Vec<DocumentRefDto> {
        vec![
            DocumentRefDto {
                document_id: "doc-1".to_string(),
                file_path: "notes/todo.md".to_string(),
                title: Some("Todo List".to_string()),
            },
            DocumentRefDto {
                document_id: "doc-2".to_string(),
                file_path: "projects/ml.md".to_string(),
                title: Some("Machine Learning".to_string()),
            },
            DocumentRefDto {
                document_id: "doc-3".to_string(),
                file_path: "ideas/startup.md".to_string(),
                title: None,
            },
        ]
    }

    #[tokio::test]
    async fn test_resolve_exact_path_match() {
        let use_case = ResolveWikilinkUseCase::new();
        let request = ResolveWikilinkRequestDto {
            link_target: "notes/todo.md".to_string(),
            source_document_id: None,
            available_documents: create_test_documents(),
        };

        let result = use_case.execute(request).await.unwrap();

        assert_eq!(result.resolved_document_id, Some("doc-1".to_string()));
        assert_eq!(result.resolved_path, Some("notes/todo.md".to_string()));
        assert_eq!(result.confidence, 1.0);
    }

    #[tokio::test]
    async fn test_resolve_path_without_extension() {
        let use_case = ResolveWikilinkUseCase::new();
        let request = ResolveWikilinkRequestDto {
            link_target: "notes/todo".to_string(),
            source_document_id: None,
            available_documents: create_test_documents(),
        };

        let result = use_case.execute(request).await.unwrap();

        assert_eq!(result.resolved_document_id, Some("doc-1".to_string()));
        assert_eq!(result.resolved_path, Some("notes/todo.md".to_string()));
        assert_eq!(result.confidence, 1.0);
    }

    #[tokio::test]
    async fn test_resolve_fuzzy_match() {
        let use_case = ResolveWikilinkUseCase::new();
        let request = ResolveWikilinkRequestDto {
            link_target: "todo".to_string(),
            source_document_id: None,
            available_documents: create_test_documents(),
        };

        let result = use_case.execute(request).await.unwrap();

        assert_eq!(result.resolved_document_id, Some("doc-1".to_string()));
        assert_eq!(result.resolved_path, Some("notes/todo.md".to_string()));
        assert_eq!(result.confidence, 0.8);
    }

    #[tokio::test]
    async fn test_resolve_title_match() {
        let use_case = ResolveWikilinkUseCase::new();
        let request = ResolveWikilinkRequestDto {
            link_target: "Todo List".to_string(),
            source_document_id: None,
            available_documents: create_test_documents(),
        };

        let result = use_case.execute(request).await.unwrap();

        assert_eq!(result.resolved_document_id, Some("doc-1".to_string()));
        assert_eq!(result.resolved_path, Some("notes/todo.md".to_string()));
        assert_eq!(result.confidence, 0.6);
    }

    #[tokio::test]
    async fn test_resolve_no_match() {
        let use_case = ResolveWikilinkUseCase::new();
        let request = ResolveWikilinkRequestDto {
            link_target: "nonexistent".to_string(),
            source_document_id: None,
            available_documents: create_test_documents(),
        };

        let result = use_case.execute(request).await.unwrap();

        assert_eq!(result.resolved_document_id, None);
        assert_eq!(result.resolved_path, None);
        assert_eq!(result.confidence, 0.0);
    }

    #[tokio::test]
    async fn test_resolve_empty_documents() {
        let use_case = ResolveWikilinkUseCase::new();
        let request = ResolveWikilinkRequestDto {
            link_target: "todo".to_string(),
            source_document_id: None,
            available_documents: vec![],
        };

        let result = use_case.execute(request).await.unwrap();

        assert_eq!(result.resolved_document_id, None);
        assert_eq!(result.resolved_path, None);
        assert_eq!(result.confidence, 0.0);
    }

    #[tokio::test]
    async fn test_resolve_with_source_document() {
        let use_case = ResolveWikilinkUseCase::new();
        let request = ResolveWikilinkRequestDto {
            link_target: "ml".to_string(),
            source_document_id: Some("doc-1".to_string()),
            available_documents: create_test_documents(),
        };

        let result = use_case.execute(request).await.unwrap();

        assert_eq!(result.resolved_document_id, Some("doc-2".to_string()));
        assert_eq!(result.resolved_path, Some("projects/ml.md".to_string()));
        assert!(result.confidence > 0.0);
    }
}
