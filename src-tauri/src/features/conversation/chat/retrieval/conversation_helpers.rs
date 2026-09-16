use std::collections::HashSet;
use std::sync::Arc;

use tracing::{debug, warn};

use crate::features::conversation::ConversationServiceTrait;
use crate::features::function_calling::domain::FunctionResult;
use crate::features::function_calling::dto::{GetDocumentOutput, SemanticSearchOutput};
use crate::features::search::dto::SearchResultDto;
use crate::interfaces::di::Container;
use crate::shared::error::Result;

pub(super) async fn load_space_document_scope(
    container: &Container,
    conversation_id: &str,
) -> Option<super::SpaceDocumentScope> {
    let repository = crate::features::conversation::repository::ConversationRepository::new(
        container.db_pool().clone(),
    );
    let (space_id, document_ids) = match repository.retrieval_document_scope(conversation_id).await
    {
        Ok(Some(scope)) => scope,
        Ok(None) => return None,
        Err(error) => {
            warn!(error = %error, conversation_id, "Failed to load conversation document scope");
            return None;
        }
    };

    debug!(
        conversation_id = conversation_id,
        space_id = space_id.as_str(),
        scoped_document_count = document_ids.len(),
        "Loaded hard space document scope"
    );

    Some(super::SpaceDocumentScope {
        space_id,
        document_ids,
    })
}

pub(super) async fn build_hyde_context_window_for_conversation(
    conv_service: &Arc<dyn ConversationServiceTrait>,
    conversation_id: &str,
) -> Option<String> {
    let aggregate = match conv_service.get_conversation(conversation_id).await {
        Ok(Some(aggregate)) => aggregate,
        Ok(None) => return None,
        Err(e) => {
            warn!(
                conversation_id = conversation_id,
                error = %e,
                "Failed loading conversation for HyDE context"
            );
            return None;
        }
    };

    let entries: Vec<String> = aggregate
        .messages()
        .iter()
        .filter(|message| message.is_completed() && !message.content.trim().is_empty())
        .map(|message| format!("{}: {}", message.role, message.content))
        .collect();

    if entries.is_empty() {
        None
    } else {
        Some(entries.join("\n"))
    }
}

pub(super) async fn persist_document_references(
    conv_service: &Arc<dyn ConversationServiceTrait>,
    conversation_id: &str,
    results: &[SearchResultDto],
) -> Result<()> {
    let mut seen: HashSet<(String, String)> = HashSet::new();

    for result in results {
        let doc_id = match result.document_id.as_deref() {
            Some(id) if !id.is_empty() => id,
            _ => continue,
        };

        let key = (doc_id.to_string(), result.id.clone());
        if !seen.insert(key) {
            continue;
        }

        if let Err(e) = conv_service
            .add_document_reference(
                conversation_id,
                doc_id.to_string(),
                Some(result.id.clone()),
                Some(result.score),
            )
            .await
        {
            warn!(
                error = %e,
                conversation_id = conversation_id,
                document_id = doc_id,
                "Failed to add conversation document reference"
            );
        }
    }

    Ok(())
}

pub(super) async fn record_tool_document_references(
    conv_service: &Arc<dyn ConversationServiceTrait>,
    conversation_id: &str,
    tool_name: &str,
    result: &FunctionResult,
) -> Result<()> {
    if !result.success {
        return Ok(());
    }

    let data = match result.data.clone() {
        Some(value) => value,
        None => return Ok(()),
    };

    match tool_name {
        "get_document" => {
            if let Ok(doc) = serde_json::from_value::<GetDocumentOutput>(data) {
                if let Err(e) = conv_service
                    .add_document_reference(conversation_id, doc.document_id.clone(), None, None)
                    .await
                {
                    warn!(
                        error = %e,
                        conversation_id = conversation_id,
                        document_id = doc.document_id.as_str(),
                        "Failed to add document reference from get_document tool"
                    );
                }
            }
        }
        "semantic_search" => {
            if let Ok(output) = serde_json::from_value::<SemanticSearchOutput>(data) {
                for result in output.results.iter() {
                    if let Err(e) = conv_service
                        .add_document_reference(
                            conversation_id,
                            result.document_id.clone(),
                            None,
                            Some(result.score),
                        )
                        .await
                    {
                        warn!(
                            error = %e,
                            conversation_id = conversation_id,
                            document_id = result.document_id.as_str(),
                            "Failed to add document reference from semantic_search tool"
                        );
                    }
                }
            }
        }
        _ => {}
    }

    Ok(())
}

pub(super) async fn load_recent_document_metadata(
    container: &Container,
    document_context: &[crate::domain::conversation::DocumentReference],
) -> Option<super::RecentDocumentMetadata> {
    let last_ref = document_context.last()?;
    let document_id = last_ref.document_id.clone();
    let doc = container
        .document_repository()
        .find_by_id(&document_id)
        .await
        .ok()??;
    Some(super::RecentDocumentMetadata {
        document_id,
        title: doc.file_name().to_string(),
    })
}
