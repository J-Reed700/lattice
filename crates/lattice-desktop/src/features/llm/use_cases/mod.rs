//! # LLM Use Cases
//!
//! Use cases for LLM model management and system capabilities.
//!
//! ## Use Cases
//!
//! - `GetSystemCapabilitiesUseCase` - Detect hardware capabilities
//! - `GetAvailableModelsUseCase` - List models in catalog
//! - `ListDownloadedModelsUseCase` - List locally downloaded models
//! - `DownloadModelUseCase` - Download a model from catalog
//! - `DeleteModelUseCase` - Delete a downloaded model
//! - `GetRecommendedModelsUseCase` - Recommend models based on system
//! - `GetBestModelUseCase` - Get single best model for system
//! - `CheckModelDownloadedUseCase` - Check if model exists locally
//! - `GetModelPathUseCase` - Get filesystem path for model

pub mod check_model_downloaded;
pub mod delete_model;
pub mod download_model;
pub mod get_available_models;
pub mod get_best_model;
pub mod get_model_path;
pub mod get_recommended_models;
pub mod get_system_capabilities;
pub mod list_models;

pub use check_model_downloaded::CheckModelDownloadedUseCase;
pub use delete_model::DeleteModelUseCase;
pub use download_model::DownloadModelUseCase;
pub use get_available_models::GetAvailableModelsUseCase;
pub use get_best_model::GetBestModelUseCase;
pub use get_model_path::GetModelPathUseCase;
pub use get_recommended_models::GetRecommendedModelsUseCase;
pub use get_system_capabilities::GetSystemCapabilitiesUseCase;
pub use list_models::ListDownloadedModelsUseCase;
