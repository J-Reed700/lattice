//! Settings → Sources on the keyring side (design §3.14): which
//! source-finding keys are stored, saving and removing one, and
//! re-registering the finders whenever the sourcing settings or the keys
//! change, and once at startup.
//!
//! A key goes from the command to the keyring, and from the keyring to its
//! finder. No DTO, log line or error carries it: the panel learns only
//! whether one is stored.

use std::collections::BTreeMap;

use lattice_core::application::contracts::settings::{KeyServiceDto, SourcesStatusDto};
use lattice_core::application::ports::{CredentialsPort, SettingsRepositoryPort};
use lattice_core::domain::sourcing::KeyService;
use lattice_core::shared::error::{AppError, Result};

use crate::features::web::services::web::WebService;

/// The source-finding keys in the keyring. A key that cannot be read is left
/// out, so its finders stay off, and the service id and the reason are
/// logged.
pub(crate) async fn stored_keys(credentials: &dyn CredentialsPort) -> BTreeMap<KeyService, String> {
    let mut keys = BTreeMap::new();
    for service in KeyService::ALL {
        match credentials.get_api_key(service.keyring_id()).await {
            Ok(Some(key)) if !key.trim().is_empty() => {
                keys.insert(service, key);
            }
            Ok(_) => {}
            Err(error) => tracing::warn!(
                service = service.keyring_id(),
                %error,
                "Could not read a source-finding key; its finders stay off"
            ),
        }
    }
    keys
}

/// Re-registers the finders from the saved sourcing settings and the keys in
/// the keyring. Reloads run one at a time, so the last change read is the
/// last one applied. When the settings cannot be read, the finders in force
/// stay.
pub async fn reload(
    web: &WebService,
    settings: &dyn SettingsRepositoryPort,
    credentials: &dyn CredentialsPort,
) {
    let _one_at_a_time = web.sourcing_reload().lock().await;
    let sourcing = match settings.get_all().await {
        Ok(settings) => settings.sourcing,
        Err(error) => {
            tracing::warn!(
                %error,
                "Could not read the sourcing settings; the finders in force stay"
            );
            return;
        }
    };
    let keys = stored_keys(credentials).await;
    let with_keys: Vec<&str> = keys.keys().map(|service| service.keyring_id()).collect();
    tracing::info!(keys = ?with_keys, "Source finders registered");
    web.apply_sourcing(sourcing, keys);
}

/// Stores `key` for `service` in the keyring and re-registers the finders.
pub async fn save_key(
    web: &WebService,
    settings: &dyn SettingsRepositoryPort,
    credentials: &dyn CredentialsPort,
    service: KeyServiceDto,
    key: &str,
) -> Result<SourcesStatusDto> {
    let service = KeyService::from(service);
    let key = key.trim();
    if key.is_empty() {
        return Err(AppError::InvalidInput(format!(
            "Enter a {} key",
            service.label()
        )));
    }
    credentials.set_api_key(service.keyring_id(), key).await?;
    reload(web, settings, credentials).await;
    Ok(web.sources_status())
}

/// Removes `service`'s key from the keyring and re-registers the finders.
pub async fn remove_key(
    web: &WebService,
    settings: &dyn SettingsRepositoryPort,
    credentials: &dyn CredentialsPort,
    service: KeyServiceDto,
) -> Result<SourcesStatusDto> {
    let service = KeyService::from(service);
    credentials.delete_api_key(service.keyring_id()).await?;
    reload(web, settings, credentials).await;
    Ok(web.sources_status())
}

#[cfg(test)]
#[path = "sources_tests.rs"]
mod tests;
