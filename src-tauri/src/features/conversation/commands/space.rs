//! Creating a conversation space.

use crate::features::conversation::space_dto::{
    ConversationSpaceDto, CreateConversationSpaceRequestDto,
};
use crate::interfaces::di::container::Container;
use crate::shared::error::{AppError, Result};
use serde_json::Value;

pub async fn create_conversation_space_impl(
    container: &Container,
    request: CreateConversationSpaceRequestDto,
) -> Result<ConversationSpaceDto> {
    if request.name.trim().is_empty() {
        return Err(AppError::InvalidInput(
            "Space name cannot be empty".to_string(),
        ));
    }
    validate_space_preferences_not_journal(
        request.tool_preferences_json.as_deref(),
        "Conversation space",
    )?;

    let trimmed_name = request.name.trim().to_string();
    let request = CreateConversationSpaceRequestDto {
        name: trimmed_name,
        ..request
    };

    crate::features::conversation::space_repository::ConversationSpaceRepository::new(
        container.db_pool().clone(),
    )
    .create(request)
    .await
}

fn validate_space_preferences_not_journal(
    raw: Option<&str>,
    context: &str,
) -> Result<(), AppError> {
    if preferences_declares_journal(raw) {
        return Err(AppError::InvalidInput(format!(
            "{} cannot declare `spaceType=journal`; journals are a separate entity",
            context
        )));
    }
    Ok(())
}

fn preferences_declares_journal(raw: Option<&str>) -> bool {
    let Some(raw_json) = raw else {
        return false;
    };
    if raw_json.trim().is_empty() {
        return false;
    }
    let parsed: Result<Value, _> = serde_json::from_str(raw_json);
    let Ok(value) = parsed else {
        return false;
    };
    let kind = value
        .get("spaceType")
        .and_then(Value::as_str)
        .or_else(|| value.get("space_type").and_then(Value::as_str))
        .unwrap_or("standard");
    kind.eq_ignore_ascii_case("journal")
}
