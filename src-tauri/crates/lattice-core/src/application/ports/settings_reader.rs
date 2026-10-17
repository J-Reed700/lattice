//! Read access to the stored settings, for background workers that re-read
//! them on every pass.

use async_trait::async_trait;

use crate::application::contracts::settings::SettingsDto;
use crate::shared::error::Result;

/// Reads the settings as they are stored now.
#[async_trait]
pub trait SettingsReaderPort: Send + Sync {
    /// The current settings.
    async fn current_settings(&self) -> Result<SettingsDto>;
}
