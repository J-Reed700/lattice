//! # Privacy gate
//!
//! Helpers any code path that wants to emit telemetry or crash reports
//! upstream MUST consult before doing so. The flags live in the
//! `Privacy` settings category (the SSOT) and default to OFF — users
//! must opt in.
//!
//! Today neither flag has a real upstream consumer (metrics are local
//! read-only snapshots; crash reports are written to local disk only).
//! When you add an upstream pipeline, gate it with these helpers so
//! the user's stated preference is honored.
//!
//! ## Usage
//!
//! ```rust,ignore
//! use crate::features::settings::privacy_gate;
//!
//! if privacy_gate::should_send_telemetry(&settings_repo).await {
//!     send_telemetry_payload(&payload).await?;
//! }
//! ```

use crate::application::ports::SettingsRepositoryPort;
use std::sync::Arc;

/// Returns true if the user has opted in to upstream telemetry.
///
/// On any error reading settings, returns `false`. Failing closed is
/// the only safe default for privacy gates.
pub async fn should_send_telemetry(repo: &Arc<dyn SettingsRepositoryPort>) -> bool {
    match repo.get_all().await {
        Ok(settings) => settings.privacy.telemetry_enabled,
        Err(err) => {
            tracing::warn!(
                "Privacy gate: failed to read settings for telemetry decision; defaulting OFF: {err}"
            );
            false
        }
    }
}

/// Returns true if the user has opted in to upstream crash reporting.
///
/// On any error reading settings, returns `false`. Failing closed is
/// the only safe default for privacy gates.
pub async fn should_send_crash_reports(repo: &Arc<dyn SettingsRepositoryPort>) -> bool {
    match repo.get_all().await {
        Ok(settings) => settings.privacy.crash_reporting,
        Err(err) => {
            tracing::warn!(
                "Privacy gate: failed to read settings for crash-reporting decision; defaulting OFF: {err}"
            );
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::ports::settings_port::MockSettingsRepository;
    use crate::features::settings::dto::SettingsDto;

    #[tokio::test]
    async fn telemetry_off_by_default() {
        let repo: Arc<dyn SettingsRepositoryPort> = Arc::new(MockSettingsRepository::new());
        assert!(!should_send_telemetry(&repo).await);
    }

    #[tokio::test]
    async fn crash_reports_off_by_default() {
        let repo: Arc<dyn SettingsRepositoryPort> = Arc::new(MockSettingsRepository::new());
        assert!(!should_send_crash_reports(&repo).await);
    }

    #[tokio::test]
    async fn telemetry_respects_opt_in() {
        let mut settings = SettingsDto::default();
        settings.privacy.telemetry_enabled = true;
        let repo: Arc<dyn SettingsRepositoryPort> =
            Arc::new(MockSettingsRepository::with_settings(settings));
        assert!(should_send_telemetry(&repo).await);
    }

    #[tokio::test]
    async fn crash_reports_respect_opt_in() {
        let mut settings = SettingsDto::default();
        settings.privacy.crash_reporting = true;
        let repo: Arc<dyn SettingsRepositoryPort> =
            Arc::new(MockSettingsRepository::with_settings(settings));
        assert!(should_send_crash_reports(&repo).await);
    }
}
