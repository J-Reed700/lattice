//! Discover models with the same credentials used for completions. Keys stay
//! in the native process; catalogs contain only model identities and names.
use std::{collections::HashSet, time::Duration};

use lattice_core::{
    application::services::chat_model_resolution::{cloud_provider_id, cloud_provider_name},
    domain::models::model_ref::CloudProvider,
    shared::{
        error::{AppError, Result},
        http::reqwest_client_builder,
    },
};
use serde::{Deserialize, Serialize};

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

pub async fn list_models(provider: CloudProvider) -> Result<CloudModelCatalogDto> {
    let key = tokio::task::spawn_blocking(move || {
        crate::infrastructure::security::keyring_storage::SecureStorage::new()
            .cloud_api_key(cloud_provider_id(provider))
    })
    .await
    .map_err(|_| AppError::Security("Could not read the cloud API key".into()))??;
    let endpoint = match provider {
        CloudProvider::Openai => "https://api.openai.com/v1/models",
        CloudProvider::Anthropic => "https://api.anthropic.com/v1/models",
        CloudProvider::Google => "https://generativelanguage.googleapis.com/v1beta/models",
    };
    catalog(provider, key.as_deref(), endpoint).await
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
            cloud_provider_name(provider)
        ))
    })??;
    Ok(CloudModelCatalogDto {
        has_api_key: true,
        models,
    })
}

#[derive(Deserialize)]
struct ApiModel {
    id: String,
    display_name: Option<String>,
    created: Option<i64>,
    shutdown_date: Option<String>,
}

#[derive(Deserialize)]
struct ApiPage {
    data: Vec<ApiModel>,
    #[serde(default)]
    has_more: bool,
    last_id: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GoogleModel {
    name: String,
    display_name: Option<String>,
    #[serde(default)]
    supported_generation_methods: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GooglePage {
    #[serde(default)]
    models: Vec<GoogleModel>,
    next_page_token: Option<String>,
}

async fn fetch_models(
    provider: CloudProvider,
    key: &str,
    endpoint: &str,
) -> Result<Vec<CloudModelDto>> {
    let label = cloud_provider_name(provider);
    let client = reqwest_client_builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|e| AppError::Network(e.to_string()))?;
    let mut models = Vec::new();
    let mut seen_ids = HashSet::new();
    let mut seen_cursors = HashSet::new();
    let mut cursor: Option<String> = None;
    for _ in 0..32 {
        let request = client.get(endpoint);
        let request = match provider {
            CloudProvider::Openai => request.bearer_auth(key),
            CloudProvider::Anthropic => {
                let request = request
                    .header("x-api-key", key)
                    .header("anthropic-version", "2023-06-01")
                    .query(&[("limit", "100")]);
                match cursor.as_deref() {
                    Some(cursor) => request.query(&[("after_id", cursor)]),
                    None => request,
                }
            }
            CloudProvider::Google => {
                let request = request
                    .header("x-goog-api-key", key)
                    .query(&[("pageSize", "1000")]);
                match cursor.as_deref() {
                    Some(cursor) => request.query(&[("pageToken", cursor)]),
                    None => request,
                }
            }
        };
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
        let (page, next) = if provider == CloudProvider::Google {
            let page: GooglePage = serde_json::from_value(value).map_err(|_| {
                AppError::Network(format!("{label} returned an invalid model list"))
            })?;
            let models = page
                .models
                .into_iter()
                .filter(|model| {
                    model
                        .supported_generation_methods
                        .iter()
                        .any(|method| method == "generateContent")
                })
                .filter(|model| is_text_model(&model.name))
                .map(|model| {
                    let id = model
                        .name
                        .strip_prefix("models/")
                        .unwrap_or(&model.name)
                        .to_owned();
                    CloudModelDto {
                        name: model
                            .display_name
                            .filter(|name| !name.is_empty())
                            .unwrap_or_else(|| id.clone()),
                        id,
                    }
                })
                .collect::<Vec<_>>();
            (models, page.next_page_token)
        } else {
            let mut page: ApiPage = serde_json::from_value(value).map_err(|_| {
                AppError::Network(format!("{label} returned an invalid model list"))
            })?;
            if provider == CloudProvider::Openai {
                page.data
                    .sort_by(|a, b| b.created.cmp(&a.created).then_with(|| a.id.cmp(&b.id)));
            }
            let next = if provider == CloudProvider::Anthropic && page.has_more {
                Some(page.last_id.filter(|id| !id.is_empty()).ok_or_else(|| {
                    AppError::Network(format!("{label} returned an invalid model-list cursor"))
                })?)
            } else {
                None
            };
            let today = chrono::Utc::now().date_naive().to_string();
            let models = page
                .data
                .into_iter()
                .filter(|model| {
                    provider != CloudProvider::Openai || is_openai_chat_model(&model.id)
                })
                .filter(|model| {
                    model
                        .shutdown_date
                        .as_deref()
                        .is_none_or(|date| date > today.as_str())
                })
                .map(|model| CloudModelDto {
                    name: model
                        .display_name
                        .filter(|name| !name.is_empty())
                        .unwrap_or_else(|| model.id.clone()),
                    id: model.id,
                })
                .collect::<Vec<_>>();
            (models, next)
        };
        models.extend(
            page.into_iter()
                .filter(|model| !model.id.trim().is_empty() && seen_ids.insert(model.id.clone())),
        );
        let Some(next) = next.filter(|cursor| !cursor.is_empty()) else {
            return Ok(models);
        };
        if !seen_cursors.insert(next.clone()) {
            return Err(AppError::Network(format!(
                "{label} repeated a model-list cursor. Try refreshing."
            )));
        }
        cursor = Some(next);
    }
    Err(AppError::Network(format!(
        "{label} model list exceeded the page limit"
    )))
}

// OpenAI's model list has no endpoint capabilities. Recognize chat families,
// including fine tunes, and exclude specialized audio/image/search endpoints.
fn is_openai_chat_model(id: &str) -> bool {
    let base = id
        .strip_prefix("ft:")
        .and_then(|id| id.split(':').next())
        .unwrap_or(id);
    (base.starts_with("gpt-")
        || base.starts_with("chatgpt-")
        || base.starts_with("chat-")
        || base
            .strip_prefix('o')
            .is_some_and(|suffix| suffix.starts_with(|c: char| c.is_ascii_digit())))
        && is_text_model(base)
}

fn is_text_model(id: &str) -> bool {
    ![
        "audio",
        "realtime",
        "transcrib",
        "tts",
        "image",
        "search",
        "instruct",
        "embedding",
        "moderation",
        "live",
        "omni",
    ]
    .iter()
    .any(|kind| id.contains(kind))
}

#[cfg(test)]
mod tests;
