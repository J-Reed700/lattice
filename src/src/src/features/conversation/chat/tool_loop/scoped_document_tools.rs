//! Follow-up document tools obey the same scope as initial chat retrieval.
use crate::features::conversation::repository::ConversationRepository;
use crate::features::function_calling::domain::{FunctionCall, FunctionResult};
use crate::features::function_calling::dto::{
    DocumentResult, SearchMode, SemanticSearchInput, SemanticSearchOutput,
};
use crate::features::search::dto::{SearchModeDto, SearchRequestDto};
use crate::interfaces::di::Container;
use crate::shared::error::{AppError, Result};

pub(super) async fn execute(
    container: &Container,
    conversation_id: &str,
    call: FunctionCall,
) -> Result<FunctionResult> {
    if !matches!(call.name.as_str(), "semantic_search" | "get_document") {
        return container.function_executor().execute(call).await;
    }
    let repository = ConversationRepository::new(container.db_pool().clone());
    let (space, allowed) = repository
        .retrieval_document_scope(conversation_id)
        .await?
        .ok_or_else(|| {
            AppError::InvalidState("Could not resolve this conversation's document scope".into())
        })?;
    if call.name == "get_document" {
        let id = call
            .arguments
            .get("document_id")
            .and_then(|id| id.as_str())
            .unwrap_or("");
        ensure_allowed_document(id, &allowed)?;
        return container.function_executor().execute(call).await;
    }
    let input: SemanticSearchInput = serde_json::from_value(call.arguments.clone())?;
    let request = SearchRequestDto {
        query: input.query.clone(),
        limit: Some(input.limit.clamp(1, 50)),
        threshold: Some(input.threshold.clamp(0.0, 1.0)),
        mode: match input.search_mode {
            SearchMode::Semantic => SearchModeDto::Vector,
            SearchMode::Keyword => SearchModeDto::BM25,
            SearchMode::Hybrid => SearchModeDto::Hybrid {
                vector_weight: 0.5,
                bm25_weight: 0.5,
            },
        },
    };
    let response = container
        .hybrid_search_use_case()
        .execute_scoped(
            request,
            (space != "space_general").then_some(space.as_str()),
            Some(&allowed),
        )
        .await?;
    let documents = container.document_repository().list_metadata().await?;
    let mut results = Vec::new();
    for result in response.results {
        let Some(doc) = documents
            .iter()
            .find(|doc| Some(doc.id().as_str()) == result.document_id.as_deref())
        else {
            continue;
        };
        if !allowed.contains(doc.id().as_str()) {
            continue;
        }
        if input.file_types.as_ref().is_some_and(|types| {
            !types.iter().any(|ext| {
                doc.file_extension()
                    .is_some_and(|actual| actual.eq_ignore_ascii_case(ext.trim_start_matches('.')))
            })
        }) {
            continue;
        }
        if input
            .date_from
            .as_ref()
            .is_some_and(|date| doc.modified_at() < date)
            || input
                .date_to
                .as_ref()
                .is_some_and(|date| doc.modified_at() > date)
        {
            continue;
        }
        results.push(DocumentResult {
            document_id: doc.id().as_str().into(),
            filename: doc.file_name().into(),
            file_path: doc.file_path().display().to_string(),
            mime_type: doc.mime_type().into(),
            score: result.score,
            snippet: result.content,
            chunk_index: result.position,
            modified_at: *doc.modified_at(),
            size_bytes: doc.size_bytes(),
        });
    }
    let output = SemanticSearchOutput {
        total_found: results.len(),
        results,
        documents: vec![],
        search_time_ms: response.query_time_ms as f64,
        query: input.query,
    };
    Ok(FunctionResult::success(serde_json::to_value(output)?))
}

fn ensure_allowed_document(id: &str, allowed: &std::collections::HashSet<String>) -> Result<()> {
    if allowed.contains(id) {
        Ok(())
    } else {
        Err(AppError::InvalidInput(
            "That document is outside this conversation's space".into(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn document_read_cannot_widen_conversation_scope() {
        let allowed = std::collections::HashSet::from(["a".into()]);
        assert!(ensure_allowed_document("a", &allowed).is_ok());
        assert!(ensure_allowed_document("b", &allowed).is_err());
        assert!(ensure_allowed_document("", &allowed).is_err());
        assert!(ensure_allowed_document("a", &Default::default()).is_err());
    }
}
