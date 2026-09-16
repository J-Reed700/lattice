use std::collections::HashSet;
use std::sync::Arc;

use tracing::{debug, info, warn};

use crate::features::conversation::ConversationServiceTrait;
use crate::features::function_calling::domain::FunctionResult;
use crate::features::function_calling::dto::{GetDocumentOutput, SemanticSearchOutput};
use crate::features::search::dto::{SearchResponseDto, SearchResultDto};
use crate::interfaces::di::Container;
use crate::shared::error::Result;

pub(super) async fn load_space_document_scope(
    container: &Container,
    conversation_id: &str,
) -> Option<super::SpaceDocumentScope> {
    let pool = container.db_pool();
    let space_id = match sqlx::query_scalar::<_, String>(
        "SELECT space_id FROM conversations WHERE id = ? LIMIT 1",
    )
    .bind(conversation_id)
    .fetch_optional(pool)
    .await
    {
        Ok(Some(value)) if !value.trim().is_empty() => value,
        Ok(_) => return None,
        Err(error) => {
            warn!(
                error = %error,
                conversation_id = conversation_id,
                "Failed to load conversation space for hard scope"
            );
            return None;
        }
    };

    let document_ids = match sqlx::query_scalar::<_, String>(
        r#"
        SELECT DISTINCT dsm.document_id
        FROM document_space_memberships dsm
        WHERE dsm.space_id = ?
          AND dsm.document_id IS NOT NULL
          AND TRIM(dsm.document_id) <> ''
        "#,
    )
    .bind(&space_id)
    .fetch_all(pool)
    .await
    {
        Ok(rows) => rows.into_iter().collect::<HashSet<_>>(),
        Err(error) => {
            warn!(
                error = %error,
                space_id = space_id.as_str(),
                "Failed to load space document scope"
            );
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

pub(super) fn apply_hard_space_scope_filter(
    mut response: SearchResponseDto,
    space_id: &str,
    scoped_document_ids: &HashSet<String>,
) -> SearchResponseDto {
    let before = response.results.len();
    response.results.retain(|result| {
        result
            .document_id
            .as_ref()
            .map(|document_id| scoped_document_ids.contains(document_id))
            .unwrap_or(false)
    });
    response.total = response.results.len();

    info!(
        space_id = space_id,
        before = before,
        after = response.results.len(),
        scoped_document_count = scoped_document_ids.len(),
        "Applied hard space scope filter to KB results"
    );

    if before > 0 && response.results.is_empty() {
        warn!(
            space_id = space_id,
            before = before,
            scoped_document_count = scoped_document_ids.len(),
            "Hard space scope filter removed all KB results"
        );
    }

    response
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
