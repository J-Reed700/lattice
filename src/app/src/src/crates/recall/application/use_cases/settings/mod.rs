//! # Settings Use Cases
//!
//! Use cases for application settings management.
//!
//! This module provides business logic for:
//! - Retrieving settings (all or by category)
//! - Updating settings with validation
//! - Resetting settings to defaults
//! - Exporting settings to file
//! - Importing settings from file
//! - Validating settings structure and values

pub mod export_settings;
pub mod get_settings;
pub mod import_settings;
pub mod reset_settings;
pub mod update_settings;
pub mod validate_settings;

// Re-export use cases
pub use export_settings::ExportSettingsUseCase;
pub use get_settings::GetSettingsUseCase;
pub use import_settings::ImportSettingsUseCase;
pub use reset_settings::ResetSettingsUseCase;
pub use update_settings::UpdateSettingsUseCase;
pub use validate_settings::ValidateSettingsUseCase;
