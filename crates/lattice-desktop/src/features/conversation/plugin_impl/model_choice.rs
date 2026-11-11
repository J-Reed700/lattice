//! A chat's own model choice. Setting it touches this chat only: never
//! Settings, the `models` rows or a loaded model, and never a turn already
//! running, which keeps the model it pinned.

use crate::features::conversation::dto::ConversationDto;
use crate::features::conversation::model_availability::check_available;
use crate::features::conversation::repository::ConversationRepository;
use crate::features::credentials::keys::cloud_api_key;
use crate::interfaces::di::Container;
use lattice_core::application::services::chat_model_resolution::cloud_provider_id;
use lattice_core::domain::conversation::ChatModelChoice;
use lattice_core::domain::models::model_ref::ModelRef;
use lattice_core::shared::error::AppError;
use lattice_core::shared::ipc::ApiError;

pub async fn set_conversation_model_choice_impl(
    conversation_id: String,
    choice: ChatModelChoice,
    container: &Container,
) -> Result<ConversationDto, ApiError> {
    if let ChatModelChoice::Explicit { model } = &choice {
        let settings = container.get_settings_use_case().execute().await?;
        let cloud_key_saved = match model {
            ModelRef::Cloud { provider, .. } => cloud_api_key(
                container.credentials().as_ref(),
                cloud_provider_id(*provider),
            )
            .await
            .map(|key| key.is_some()),
            _ => Ok(false),
        };
        check_available(
            model,
            &settings.llm,
            container.downloaded_models().as_ref(),
            |_| cloud_key_saved,
        )
        .await?;
    }

    let repository = ConversationRepository::new(container.database().clone());
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
