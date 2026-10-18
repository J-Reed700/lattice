//! Conversation branching and regeneration workflows.
use super::branching_dto::*;
use super::chat::{
    chat_with_conversation_impl as run_chat_with_conversation_impl, ChatResponse,
    MessageAttachments, ToolPreferences,
};
use crate::features::conversation::chat::ports::{ChatRuntime, ConversationBranches};
use crate::features::conversation::tangent_dto::{ConversationTangentDto, CreateTangentRequestDto};
use lattice_core::application::contracts::images::ChatImageDto;
use lattice_core::domain::models::model_ref::ModelRef;
use lattice_core::shared::{error::AppError, ipc::ApiError};

pub async fn create_conversation_tangent_impl(
    request: CreateTangentRequestDto,
    runtime: &dyn ChatRuntime,
) -> Result<ConversationTangentDto, ApiError> {
    let tangent = runtime
        .conversation_branches()
        .create_tangent(request)
        .await
        .map_err(ApiError::from)?;
    copy_branch_pictures(runtime, &tangent.conversation_id).await;
    Ok(tangent)
}

/// Give a branch its own copies of the pictures its copied messages show,
/// and point their `metadata.images` at them, so deleting the conversation
/// it came from leaves them intact. A failure is logged: the branch exists,
/// and a picture not copied shows from the original while that exists.
async fn copy_branch_pictures(runtime: &dyn ChatRuntime, conversation_id: &str) {
    use lattice_core::application::ports::CopiedMessage;

    let messages = match runtime
        .conversation_branches()
        .get_messages(conversation_id)
        .await
    {
        Ok(messages) => messages,
        Err(error) => {
            tracing::warn!(%error, "Could not read a branch to copy its pictures");
            return;
        }
    };
    let shown: Vec<(String, Vec<ChatImageDto>)> = messages
        .iter()
        .filter_map(|message| {
            let metadata: serde_json::Value =
                serde_json::from_str(message.metadata.as_deref()?).ok()?;
            let images: Vec<ChatImageDto> =
                serde_json::from_value(metadata.get("images")?.clone()).ok()?;
            (!images.is_empty()).then(|| (message.id.clone(), images))
        })
        .collect();
    if shown.is_empty() {
        return;
    }
    let copies: Vec<CopiedMessage> = shown
        .iter()
        .map(|(message_id, images)| CopiedMessage {
            message_id: message_id.clone(),
            image_ids: images.iter().map(|image| image.id.clone()).collect(),
        })
        .collect();
    let copied: std::collections::HashMap<String, Vec<ChatImageDto>> = match runtime
        .chat_images()
        .copy_for_messages(conversation_id, &copies)
        .await
    {
        Ok(copied) => copied.into_iter().collect(),
        Err(error) => {
            tracing::warn!(%error, "Could not copy a branch's pictures");
            return;
        }
    };
    let conversations = runtime.conversation_service();
    for (message_id, originals) in shown {
        let Some(copies) = copied.get(&message_id) else {
            continue;
        };
        let images: Vec<ChatImageDto> = originals
            .into_iter()
            .map(|original| {
                copies
                    .iter()
                    .find(|copy| copy.r#ref == original.r#ref)
                    .cloned()
                    .unwrap_or(original)
            })
            .collect();
        if let Err(error) = conversations
            .set_message_metadata_fields(
                &message_id,
                vec![("images".to_string(), serde_json::json!(images))],
            )
            .await
        {
            tracing::warn!(%error, "Could not point a branch message at its pictures");
        }
    }
}

pub async fn list_conversation_tangents_impl(
    conversation_id: String,
    runtime: &dyn ChatRuntime,
) -> Result<Vec<ConversationTangentDto>, ApiError> {
    runtime
        .conversation_branches()
        .list_tangents(&conversation_id)
        .await
        .map_err(ApiError::from)
}

pub async fn promote_conversation_tangent_impl(
    conversation_id: String,
    runtime: &dyn ChatRuntime,
) -> Result<crate::features::conversation::dto::ConversationDto, ApiError> {
    let branches = runtime.conversation_branches();
    branches
        .promote_tangent(&conversation_id)
        .await
        .map_err(ApiError::from)?;
    let conversation = branches
        .find_by_id(&conversation_id)
        .await
        .map_err(ApiError::from)?
        .ok_or_else(|| ApiError::from(AppError::NotFound("Tangent no longer exists.".into())))?;
    branches
        .project_conversation(&conversation)
        .await
        .map_err(ApiError::from)
}

fn to_message_dto(
    message: &lattice_core::domain::conversation::ConversationMessage,
) -> crate::features::conversation::dto::MessageDto {
    crate::features::conversation::dto::MessageDto {
        id: message.id.to_string(),
        conversation_id: message.conversation_id.to_string(),
        role: message.role.to_string(),
        content: message.content.clone(),
        tokens: message.tokens,
        created_at: message.created_at.to_rfc3339(),
        metadata: message.metadata.clone(),
        status: message.status.clone(),
    }
}

/// Free the picture files of messages a truncate or regenerate deleted.
/// Their rows went with the messages by cascade, so nothing names the files
/// any more; they go now rather than at the next start's sweep. A failure is
/// logged: the sweep collects what is left.
fn remove_unclaimed_pictures(runtime: &dyn ChatRuntime) {
    let images = runtime.chat_images();
    runtime.background().spawn(async move {
        if let Err(error) = images.remove_unclaimed_files().await {
            tracing::warn!(%error, "Could not remove the pictures of deleted messages");
        }
    });
}

pub async fn truncate_conversation_after_impl(
    request: TruncateConversationAfterRequestDto,
    runtime: &dyn ChatRuntime,
) -> Result<TruncateConversationAfterResponseDto, ApiError> {
    let branches = runtime.conversation_branches();

    let deleted = branches
        .truncate_after(
            &request.conversation_id,
            &request.message_id,
            request.inclusive,
        )
        .await
        .map_err(ApiError::from)?;
    if deleted > 0 {
        remove_unclaimed_pictures(runtime);
    }

    let messages = branches
        .get_messages(&request.conversation_id)
        .await
        .map_err(ApiError::from)?;

    Ok(TruncateConversationAfterResponseDto {
        conversation_id: request.conversation_id,
        deleted_count: deleted as u32,
        messages: messages.iter().map(to_message_dto).collect(),
    })
}

pub async fn fork_conversation_impl(
    request: ForkConversationRequestDto,
    runtime: &dyn ChatRuntime,
) -> Result<ForkConversationResponseDto, ApiError> {
    let branches = runtime.conversation_branches();

    let source = branches
        .find_by_id(&request.conversation_id)
        .await
        .map_err(ApiError::from)?
        .ok_or_else(|| {
            ApiError::from(AppError::NotFound(format!(
                "Conversation {} not found",
                request.conversation_id
            )))
        })?;

    let mut new_title = format!("{} · branch", source.title);
    if new_title.chars().count() > 200 {
        new_title = new_title.chars().take(200).collect();
    }
    let new_id = uuid::Uuid::new_v4().to_string();

    let (new_id, copied) = branches
        .fork(
            &request.conversation_id,
            request.up_to_message_id.as_deref(),
            &new_id,
            &new_title,
        )
        .await
        .map_err(ApiError::from)?;
    copy_branch_pictures(runtime, &new_id).await;

    let created = branches
        .find_by_id(&new_id)
        .await
        .map_err(ApiError::from)?
        .ok_or_else(|| {
            ApiError::from(AppError::InvalidState(
                "The branch was created but could not be read back.".to_string(),
            ))
        })?;

    Ok(ForkConversationResponseDto {
        conversation: branches
            .project_conversation(&created)
            .await
            .map_err(ApiError::from)?,
        copied_message_count: copied,
    })
}

/// `model` answers this regeneration only; the chat's choice is untouched.
pub async fn regenerate_response_impl(
    runtime: &dyn ChatRuntime,
    conversation_id: String,
    tool_preferences: Option<ToolPreferences>,
    request_id: Option<String>,
    model: Option<ModelRef>,
    window: tauri::Window,
) -> Result<ChatResponse, ApiError> {
    let branches = runtime.conversation_branches();

    // The question's pictures go with its message when it is lifted, so
    // they are staged again first, as copies the new turn claims.
    let pictures = restage_question_pictures(runtime, &conversation_id).await;
    let lifted = branches.take_last_user_turn(&conversation_id).await;
    if !matches!(lifted, Ok(Some(_))) {
        release_pictures(runtime, &conversation_id, &pictures).await;
    }
    let (content, _tokens, metadata) = lifted.map_err(ApiError::from)?.ok_or_else(|| {
        ApiError::from(AppError::InvalidInput(
            "This conversation has nothing to regenerate.".to_string(),
        ))
    })?;
    // The answer being replaced is gone, and its pictures with it.
    remove_unclaimed_pictures(runtime);

    // The lifted message's attachment record travels with it, so a regenerate
    // shows the same files the original turn brought in — and, through the
    // ids, reads them again rather than answering the same question with the
    // files missing.
    let attachment_metadata = metadata
        .as_deref()
        .and_then(|raw| serde_json::from_str::<serde_json::Value>(raw).ok());
    let string_list = |value: Option<&serde_json::Value>, key: &str| {
        value.and_then(|value| {
            value.get(key).and_then(|items| {
                items.as_array().map(|entries| {
                    entries
                        .iter()
                        .filter_map(|entry| entry.as_str().map(str::to_string))
                        .collect::<Vec<String>>()
                })
            })
        })
    };
    let attachment_names = string_list(attachment_metadata.as_ref(), "attachments");
    let attachment_document_ids =
        string_list(attachment_metadata.as_ref(), "attachmentDocumentIds");
    let image_ids: Vec<String> = pictures.iter().map(|picture| picture.id.clone()).collect();
    // A question put back after a failure shows the restaged copies.
    let metadata = with_pictures(metadata, &pictures);

    // How long the thread is without the question, so a failure can tell
    // whether the turn got as far as saving it again.
    // Unreadable means unknown, and an unknown thread gets the question back.
    let baseline = branches
        .get_messages(&conversation_id)
        .await
        .map(|messages| messages.len())
        .ok();

    let fut = run_chat_with_conversation_impl(
        runtime,
        Some(conversation_id.clone()),
        content.clone(),
        tool_preferences,
        None,
        request_id,
        MessageAttachments {
            names: attachment_names.unwrap_or_default(),
            document_ids: attachment_document_ids.unwrap_or_default(),
            image_ids: image_ids.clone(),
        },
        model,
        window,
    );

    match Box::pin(fut).await {
        Ok(response) => Ok(response),
        Err(error) => {
            let restored = restore_lifted_question(
                branches.as_ref(),
                &conversation_id,
                baseline,
                &content,
                metadata.as_deref(),
            )
            .await;
            if let Some(message_id) = restored.filter(|_| !image_ids.is_empty()) {
                let claimed = runtime
                    .chat_images()
                    .attach_ids_to_message(
                        &conversation_id,
                        &image_ids,
                        &message_id,
                        &format!("restore:{message_id}"),
                    )
                    .await;
                if let Err(error) = claimed {
                    tracing::warn!(%error, "Could not give the restored question its pictures");
                }
            }
            Err(ApiError::from(error))
        }
    }
}

/// Fresh draft copies of the pictures the last question carries, so the
/// regenerated turn sees them again. A picture that cannot be copied is
/// logged and left out.
async fn restage_question_pictures(
    runtime: &dyn ChatRuntime,
    conversation_id: &str,
) -> Vec<ChatImageDto> {
    let Ok(messages) = runtime
        .conversation_branches()
        .get_messages(conversation_id)
        .await
    else {
        return Vec::new();
    };
    let Some(question) = messages
        .iter()
        .rev()
        .find(|message| message.role == lattice_core::domain::conversation::MessageRole::User)
    else {
        return Vec::new();
    };
    let ids: Vec<String> = question
        .metadata
        .as_deref()
        .and_then(|raw| serde_json::from_str::<serde_json::Value>(raw).ok())
        .and_then(|metadata| metadata.get("images").cloned())
        .and_then(|images| serde_json::from_value::<Vec<ChatImageDto>>(images).ok())
        .unwrap_or_default()
        .into_iter()
        .map(|image| image.id)
        .collect();
    let images = runtime.chat_images();
    let mut restaged = Vec::with_capacity(ids.len());
    for id in ids {
        match images.restage(conversation_id, &id).await {
            Ok(copy) => restaged.push(copy),
            Err(error) => {
                tracing::warn!(%error, image_id = id.as_str(), "Could not keep a picture for the regenerate");
            }
        }
    }
    restaged
}

async fn release_pictures(
    runtime: &dyn ChatRuntime,
    conversation_id: &str,
    pictures: &[ChatImageDto],
) {
    if pictures.is_empty() {
        return;
    }
    let ids: Vec<String> = pictures.iter().map(|picture| picture.id.clone()).collect();
    if let Err(error) = runtime
        .chat_images()
        .release_images(conversation_id, &ids)
        .await
    {
        tracing::warn!(%error, "Could not release a regenerate's staged pictures");
    }
}

/// `metadata` with `images` replaced by `pictures`, when there are any.
fn with_pictures(metadata: Option<String>, pictures: &[ChatImageDto]) -> Option<String> {
    if pictures.is_empty() {
        return metadata;
    }
    let mut map = metadata
        .as_deref()
        .and_then(|raw| serde_json::from_str::<serde_json::Value>(raw).ok())
        .and_then(|value| match value {
            serde_json::Value::Object(map) => Some(map),
            _ => None,
        })
        .unwrap_or_default();
    map.insert("images".to_string(), serde_json::json!(pictures));
    Some(serde_json::Value::Object(map).to_string())
}

/// Put a lifted question back on the thread after a failed regenerate.
///
/// The chat turn persists the question itself once retrieval is done and marks
/// it failed when generation fails, so restoring it unconditionally left two
/// failed copies. Only a turn that failed before that point — the thread no
/// longer than it was when the question was lifted — needs it put back.
/// Returns the id of the question it put back, if it put one back.
async fn restore_lifted_question(
    branches: &dyn ConversationBranches,
    conversation_id: &str,
    baseline: Option<usize>,
    content: &str,
    metadata: Option<&str>,
) -> Option<String> {
    let Some(baseline) = baseline else {
        return insert_failed_question(branches, conversation_id, content, metadata).await;
    };
    let saved = match branches.get_messages(conversation_id).await {
        Ok(messages) => messages.len() > baseline,
        Err(error) => {
            tracing::warn!(
                error = %error,
                conversation_id,
                "Could not read the thread after a failed regenerate; restoring the question"
            );
            false
        }
    };
    if saved {
        return None;
    }
    insert_failed_question(branches, conversation_id, content, metadata).await
}

async fn insert_failed_question(
    branches: &dyn ConversationBranches,
    conversation_id: &str,
    content: &str,
    metadata: Option<&str>,
) -> Option<String> {
    match branches
        .add_message_with_status(
            conversation_id,
            lattice_core::domain::conversation::MessageRole::User,
            content,
            0,
            metadata,
            "failed",
        )
        .await
    {
        Ok(message) => Some(message.id),
        Err(persist_error) => {
            tracing::error!(
                error = %persist_error,
                conversation_id,
                "Failed to restore the user message after a failed regenerate"
            );
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::conversation::repository::ConversationRepository;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn thread() -> (ConversationRepository, String) {
        let pool = SqlitePoolOptions::new().connect(":memory:").await.unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        let repo = ConversationRepository::new(pool);
        let id = repo.create("Thread", None).await.unwrap().id.to_string();
        (repo, id)
    }

    #[tokio::test]
    async fn a_question_the_failed_turn_already_saved_is_not_restored_twice() {
        let (repo, id) = thread().await;
        // What the chat turn leaves behind when generation fails.
        repo.add_message_with_status(
            &id,
            lattice_core::domain::conversation::MessageRole::User,
            "why?",
            0,
            None,
            "failed",
        )
        .await
        .unwrap();

        restore_lifted_question(&repo, &id, Some(0), "why?", None).await;

        assert_eq!(repo.get_messages(&id).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn a_question_lost_before_the_turn_saved_it_is_put_back_failed() {
        let (repo, id) = thread().await;

        restore_lifted_question(&repo, &id, Some(0), "why?", None).await;

        let messages = repo.get_messages(&id).await.unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].content, "why?");
        assert_eq!(messages[0].status, "failed");
    }

    /// A question put back after a failed regenerate shows the restaged
    /// copies, which it then claims, and keeps the rest of its record.
    #[test]
    fn a_restored_question_names_the_restaged_pictures() {
        let copy = crate::features::conversation::chat::test_runtime::test_attachment(2, 64, 64);
        let metadata = with_pictures(
            Some(r#"{"requestId":"r-1","images":[{"id":"gone"}]}"#.into()),
            std::slice::from_ref(&copy),
        )
        .unwrap();
        let metadata: serde_json::Value = serde_json::from_str(&metadata).unwrap();
        assert_eq!(metadata["requestId"], "r-1");
        assert_eq!(metadata["images"][0]["id"], copy.id.as_str());
        assert_eq!(metadata["images"].as_array().unwrap().len(), 1);
        assert_eq!(with_pictures(Some("{}".into()), &[]), Some("{}".into()));
    }
}
