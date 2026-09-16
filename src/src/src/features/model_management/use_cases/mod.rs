//! Model Management Use Cases
//!
//! Use cases for tracking and managing downloaded models.

mod check_is_downloaded;
mod clear_active_chat_model;
mod clear_active_embedding_model;
mod delete_downloaded_model;
pub mod download_default_model;
mod get_active_chat_model;
mod get_active_embedding_model;
mod get_models_with_metadata;
mod set_active_chat_model;
mod set_active_embedding_model;
mod set_active_utility_model;
mod track_download;

pub use check_is_downloaded::CheckIsDownloadedUseCase;
pub use clear_active_chat_model::ClearActiveChatModelUseCase;
pub use clear_active_embedding_model::ClearActiveEmbeddingModelUseCase;
pub use delete_downloaded_model::DeleteDownloadedModelUseCase;
pub use download_default_model::*;
pub use get_active_chat_model::GetActiveChatModelUseCase;
pub use get_active_embedding_model::GetActiveEmbeddingModelUseCase;
pub use get_models_with_metadata::GetDownloadedModelsWithMetadataUseCase;
pub use set_active_chat_model::SetActiveChatModelUseCase;
pub use set_active_embedding_model::SetActiveEmbeddingModelUseCase;
pub use set_active_utility_model::SetActiveUtilityModelUseCase;
pub use track_download::TrackDownloadUseCase;
