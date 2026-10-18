//! A chat's own model choice. Setting it touches this chat only: never
//! Settings, the `models` rows or a loaded model, and never a turn already
//! running, which keeps the model it pinned.

use crate::features::conversation::dto::ConversationDto;
use crate::features::conversation::model_availability::check_available;
use crate::features::conversation::repository::ConversationRepository;
use crate::infrastructure::security::keyring_storage::SecureStorage;
use crate::interfaces::di::Container;
use lattice_core::domain::conversation::ChatModelChoice;
use lattice_core::shared::error::AppError;
use lattice_core::shared::ipc::ApiError;

pub async fn set_conversation_model_choice_impl(
    conversation_id: String,
    choice: ChatModelChoice,
    container: &Container,
) -> Result<ConversationDto, ApiError> {
    if let ChatModelChoice::Explicit { model } = &choice {
        let settings = container.get_settings_use_case().execute().await?;
        let keys = SecureStorage::new();
        check_available(
            model,
            &settings.llm,
            container.downloaded_models().as_ref(),
            |provider| Ok(keys.cloud_api_key(provider)?.is_some()),
        )
        .await?;
    }

    let repository = ConversationRepository::new(container.db_pool().clone());
    let from = repository.model_choice(&conversation_id).await?;
    repository
        .set_model_choice(&conversation_id, &choice)
        .await?;
    tracing::info!(
        conversation_id = conversation_id.as_str(),
        from = %described(&from),
        to = %described(&choice),
        "chat.model_choice_set"
    );

    let conversation = repository
        .find_by_id(&conversation_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Conversation not found: {conversation_id}")))?;
    Ok(repository.project_conversation(&conversation).await?)
}

fn described(choice: &ChatModelChoice) -> String {
    match choice {
        ChatModelChoice::Default => "default".to_string(),
        ChatModelChoice::Explicit { model } => model.to_string(),
    }
}
