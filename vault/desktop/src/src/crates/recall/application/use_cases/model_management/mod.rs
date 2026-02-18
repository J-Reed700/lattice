//! Model Management Use Cases
//!
//! Use cases for tracking and managing downloaded models.

mod check_is_downloaded_use_case;
mod clear_active_chat_model_use_case;
mod clear_active_embedding_model_use_case;
mod delete_downloaded_model_use_case;
pub mod download_default_model;
mod get_active_chat_model_use_case;
mod get_active_embedding_model_use_case;
mod get_models_with_metadata_use_case;
mod set_active_chat_model_use_case;
mod set_active_embedding_model_use_case;
mod track_download_use_case;

pub use check_is_downloaded_use_case::CheckIsDownloadedUseCase;
pub use clear_active_chat_model_use_case::ClearActiveChatModelUseCase;
pub use clear_active_embedding_model_use_case::ClearActiveEmbeddingModelUseCase;
pub use delete_downloaded_model_use_case::DeleteDownloadedModelUseCase;
pub use download_default_model::*;
pub use get_active_chat_model_use_case::GetActiveChatModelUseCase;
pub use get_active_embedding_model_use_case::GetActiveEmbeddingModelUseCase;
pub use get_models_with_metadata_use_case::GetDownloadedModelsWithMetadataUseCase;
pub use set_active_chat_model_use_case::SetActiveChatModelUseCase;
pub use set_active_embedding_model_use_case::SetActiveEmbeddingModelUseCase;
pub use track_download_use_case::TrackDownloadUseCase;
