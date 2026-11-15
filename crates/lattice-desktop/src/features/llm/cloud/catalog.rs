//! Discover models with the same credentials used for completions. Keys stay
//! in the native process; catalogs contain only model identities and names.
use std::{collections::HashSet, time::Duration};

use lattice_core::{
    application::{
        ports::credentials_port::CredentialsPort,
        services::chat_model_resolution::cloud_provider_id,
    },
    domain::models::model_ref::CloudProvider,
    shared::{
        error::{AppError, Result},
        http::reqwest_client_builder,
    },
};
use serde::{Deserialize, Serialize};

use super::protocol::protocols;
use lattice_core::shared::error::WithCauses;

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CloudModelDto {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Default, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CloudModelCatalogDto {
    pub has_api_key: bool,
    pub models: Vec<CloudModelDto>,
}

/// The key saved for `provider`, if one is.
pub(crate) async fn saved_key(
    credentials: &dyn CredentialsPort,
    provider: CloudProvider,
) -> Result<Option<String>> {
    lattice_core::application::services::credentials::cloud_api_key(
        credentials,
        cloud_provider_id(provider),
    )
    .await
}

pub async fn list_models(
    credentials: &dyn CredentialsPort,
    provider: CloudProvider,
) -> Result<CloudModelCatalogDto> {
    let key = saved_key(credentials, provider).await?;
    catalog(
        provider,
        key.as_deref(),
        protocols(provider).catalog.catalog_url(),
    )
    .await
}

async fn catalog(
    provider: CloudProvider,
    key: Option<&str>,
    endpoint: &str,
) -> Result<CloudModelCatalogDto> {
    let Some(key) = key.filter(|key| !key.trim().is_empty()) else {
        return Ok(CloudModelCatalogDto::default());
    };
    let models = tokio::time::timeout(
        Duration::from_secs(30),
        fetch_models(provider, key, endpoint),
    )
    .await
    .map_err(|_| {
        AppError::Network(format!(
            "{} model list timed out. Try refreshing.",
            protocols(provider).catalog.shown_name()
        ))
    })??;
    Ok(CloudModelCatalogDto {
        has_api_key: true,
        models,
    })
}

async fn fetch_models(
    provider: CloudProvider,
    key: &str,
    endpoint: &str,
) -> Result<Vec<CloudModelDto>> {
    let protocol = protocols(provider).catalog;
    let label = protocol.shown_name();
    let client = reqwest_client_builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|e| AppError::Network(WithCauses(&e).to_string()))?;
    let mut models = Vec::new();
    let mut seen_ids = HashSet::new();
    let mut seen_cursors = HashSet::new();
    let mut cursor: Option<String> = None;
    loop {
        let request = protocol.list_request(&client, endpoint, key, cursor.as_deref());
        let response = request.send().await.map_err(|_| {
            AppError::Network(format!(
                "Could not load {label} models. Check your connection and try again."
            ))
        })?;
        let status = response.status();
        if !status.is_success() {
            // Provider bodies can echo credentials or other submitted data.
            let reason = match status.as_u16() {
                401 | 403 => "API key was rejected. Check the key and its model-list permissions",
                429 => "rate limit reached. Try refreshing shortly",
                _ => "model list request failed. Try refreshing",
            };
            return Err(AppError::Network(format!(
                "{label} {reason} (HTTP {status})"
            )));
        }
        let value = response
            .json::<serde_json::Value>()
            .await
            .map_err(|_| AppError::Network(format!("{label} returned an invalid model list")))?;
        let page = protocol.parse_model_page(value)?;
        models.extend(
            page.models
                .into_iter()
                .filter(|model| !model.id.trim().is_empty() && seen_ids.insert(model.id.clone())),
        );
        let Some(next) = page.next.filter(|cursor| !cursor.is_empty()) else {
            return Ok(models);
        };
        if !seen_cursors.insert(next.clone()) {
            return Err(AppError::Network(format!(
                "{label} repeated a model-list cursor. Try refreshing."
            )));
        }
        cursor = Some(next);
    }
}

#[cfg(test)]
mod tests;
