use super::*;
use crate::features::learning::tutor::{bind_conversation, SetConversationCourseRequestDto};

/// Makes a conversation a tutor chat of a course, on one of its lessons when it
/// was opened from one.
#[tauri::command]
#[specta::specta]
pub async fn set_conversation_course(
    request: SetConversationCourseRequestDto,
    container: State<'_, Container>,
) -> Result<(), ApiError> {
    bind_conversation(container.database(), &request)
        .await
        .map_err(ApiError::from)
}
