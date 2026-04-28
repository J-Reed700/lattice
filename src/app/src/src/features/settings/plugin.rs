//! Settings Plugin - Settings management commands
//!
//! Thin plugin wrapper for settings CRUD operations via use cases.

use crate::features::settings::dto::{
    ExportSettingsRequestDto, ImportSettingsRequestDto, SettingsCategory, UpdateSettingsRequestDto,
};
use crate::interfaces::di::Container;
use crate::shared::api_result::ApiError;
use crate::shared::error::AppError;
use crate::shared::utils::reqwest_client_builder;
use reqwest::{
    header::{HeaderMap, HeaderName, HeaderValue},
    Client,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{
    plugin::{Builder, TauriPlugin},
    Runtime, State, Theme,
};
use url::Url;

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSettingsRequest {
    pub category: Option<String>,
    pub updates: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ImportSettingsRequest {
    pub settings: String,
    pub merge: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct TestOllamaConnectionRequest {
    pub ollama_url: String,
    #[serde(default)]
    pub auth_header_name: String,
    #[serde(default)]
    pub auth_header_value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct TestOllamaConnectionResponse {
    pub endpoint: String,
    pub models: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct TestCustomToolRequest {
    pub endpoint: String,
    pub query_param: String,
    pub max_results_param: Option<String>,
    pub default_max_results: u32,
    pub query: String,
    pub max_results: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct TestCustomToolResponse {
    pub final_url: String,
    pub status: u16,
    pub content_type: Option<String>,
    pub body_preview: String,
}

fn parse_category(category: &str) -> Result<SettingsCategory, AppError> {
    SettingsCategory::from_str(category).map_err(AppError::InvalidInput)
}

fn temp_settings_path(prefix: &str) -> std::path::PathBuf {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    std::env::temp_dir().join(format!("{}-{}.json", prefix, timestamp))
}

fn validate_and_normalize_ollama_url(raw_url: &str) -> Result<String, AppError> {
    let trimmed = raw_url.trim();
    if trimmed.is_empty() {
        return Err(AppError::InvalidInput("Ollama URL is required".to_string()));
    }

    let parsed = Url::parse(trimmed)
        .map_err(|e| AppError::InvalidInput(format!("Invalid Ollama URL: {}", e)))?;

    if parsed.scheme() != "http" && parsed.scheme() != "https" {
        return Err(AppError::InvalidInput(
            "Ollama URL must use http or https".to_string(),
        ));
    }

    if parsed.host_str().is_none() {
        return Err(AppError::InvalidInput(
            "Ollama URL must include a host".to_string(),
        ));
    }

    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err(AppError::InvalidInput(
            "Credentials in URL are not allowed. Use auth header fields instead.".to_string(),
        ));
    }

    Ok(trimmed.trim_end_matches('/').to_string())
}

fn build_http_client(request: &TestOllamaConnectionRequest) -> Result<Client, AppError> {
    let auth_name = request.auth_header_name.trim();
    let auth_value = request.auth_header_value.trim();
    if auth_name.is_empty() ^ auth_value.is_empty() {
        return Err(AppError::InvalidInput(
            "authHeaderName and authHeaderValue must both be set".to_string(),
        ));
    }

    let mut builder = reqwest_client_builder().timeout(std::time::Duration::from_secs(15));
    if !auth_name.is_empty() {
        let header_name = HeaderName::from_bytes(auth_name.as_bytes())
            .map_err(|e| AppError::InvalidInput(format!("Invalid auth header name: {}", e)))?;
        let header_value = HeaderValue::from_str(auth_value)
            .map_err(|e| AppError::InvalidInput(format!("Invalid auth header value: {}", e)))?;
        let mut headers = HeaderMap::new();
        headers.insert(header_name, header_value);
        builder = builder.default_headers(headers);
    }

    builder
        .build()
        .map_err(|e| AppError::Network(format!("Failed to build HTTP client: {}", e)))
}

async fn fetch_models_from_v1(client: &Client, base_url: &str) -> Result<Vec<String>, AppError> {
    let endpoint = format!("{}/v1/models", base_url);
    let response = client
        .get(&endpoint)
        .send()
        .await
        .map_err(|e| AppError::Network(format!("Failed to reach {}: {}", endpoint, e)))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(AppError::Network(format!(
            "{} returned {}: {}",
            endpoint, status, body
        )));
    }

    let body: serde_json::Value = response.json().await.map_err(|e| {
        AppError::Deserialization(format!("Invalid response from {}: {}", endpoint, e))
    })?;

    let mut models = Vec::new();
    if let Some(items) = body.get("data").and_then(|data| data.as_array()) {
        for item in items {
            if let Some(model_id) = item.get("id").and_then(|id| id.as_str()) {
                let name = model_id.trim();
                if !name.is_empty() && !models.iter().any(|m| m == name) {
                    models.push(name.to_string());
                }
            }
        }
    }

    if models.is_empty() {
        return Err(AppError::Deserialization(format!(
            "{} did not include any model ids",
            endpoint
        )));
    }

    Ok(models)
}

async fn fetch_models_from_tags(client: &Client, base_url: &str) -> Result<Vec<String>, AppError> {
    let endpoint = format!("{}/api/tags", base_url);
    let response = client
        .get(&endpoint)
        .send()
        .await
        .map_err(|e| AppError::Network(format!("Failed to reach {}: {}", endpoint, e)))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(AppError::Network(format!(
            "{} returned {}: {}",
            endpoint, status, body
        )));
    }

    let body: serde_json::Value = response.json().await.map_err(|e| {
        AppError::Deserialization(format!("Invalid response from {}: {}", endpoint, e))
    })?;

    let mut models = Vec::new();
    if let Some(items) = body.get("models").and_then(|data| data.as_array()) {
        for item in items {
            if let Some(name) = item.get("name").and_then(|id| id.as_str()) {
                let model_name = name.trim();
                if !model_name.is_empty() && !models.iter().any(|m| m == model_name) {
                    models.push(model_name.to_string());
                }
            }
        }
    }

    if models.is_empty() {
        return Err(AppError::Deserialization(format!(
            "{} did not include any model names",
            endpoint
        )));
    }

    Ok(models)
}

#[tauri::command]
#[specta::specta]
pub async fn test_ollama_connection(
    request: TestOllamaConnectionRequest,
) -> Result<TestOllamaConnectionResponse, ApiError> {
    let base_url =
        validate_and_normalize_ollama_url(&request.ollama_url).map_err(ApiError::from)?;
    let client = build_http_client(&request).map_err(ApiError::from)?;

    match fetch_models_from_v1(&client, &base_url).await {
        Ok(models) => {
            return Ok(TestOllamaConnectionResponse {
                endpoint: "/v1/models".to_string(),
                models,
            });
        }
        Err(v1_error) => match fetch_models_from_tags(&client, &base_url).await {
            Ok(models) => Ok(TestOllamaConnectionResponse {
                endpoint: "/api/tags".to_string(),
                models,
            }),
            Err(tags_error) => Err(ApiError::from(AppError::Network(format!(
                "Failed to fetch models from both /v1/models and /api/tags. v1 error: {}. tags error: {}",
                v1_error, tags_error
            )))),
        },
    }
}

#[tauri::command]
#[specta::specta]
pub async fn test_custom_tool(
    request: TestCustomToolRequest,
) -> Result<TestCustomToolResponse, ApiError> {
    let endpoint = request.endpoint.trim();
    if endpoint.is_empty() {
        return Err(ApiError::from(AppError::InvalidInput(
            "Endpoint is required".to_string(),
        )));
    }

    let query_param = request.query_param.trim();
    if query_param.is_empty() {
        return Err(ApiError::from(AppError::InvalidInput(
            "queryParam is required".to_string(),
        )));
    }

    let mut url = Url::parse(endpoint).map_err(|error| {
        ApiError::from(AppError::InvalidInput(format!(
            "Invalid endpoint URL '{}': {}",
            endpoint, error
        )))
    })?;
    match url.scheme() {
        "http" | "https" => {}
        scheme => {
            return Err(ApiError::from(AppError::InvalidInput(format!(
                "Unsupported URL scheme '{}'. Use http or https.",
                scheme
            ))));
        }
    }

    let query = request.query.trim();
    if query.is_empty() {
        return Err(ApiError::from(AppError::InvalidInput(
            "Test query is required".to_string(),
        )));
    }

    let requested_max = request.max_results.unwrap_or(request.default_max_results);
    let clamped_max = requested_max.clamp(1, 100);

    {
        let mut pairs = url.query_pairs_mut();
        pairs.append_pair(query_param, query);
        if let Some(max_param) = request
            .max_results_param
            .as_ref()
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
        {
            pairs.append_pair(max_param, &clamped_max.to_string());
        }
    }

    let client = reqwest_client_builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|error| {
            ApiError::from(AppError::Network(format!(
                "Failed to build HTTP client: {}",
                error
            )))
        })?;

    let response = client.get(url.clone()).send().await.map_err(|error| {
        ApiError::from(AppError::Network(format!(
            "Failed to call custom tool endpoint: {}",
            error
        )))
    })?;

    let status = response.status();
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.to_string());
    let body = response.text().await.unwrap_or_default();
    let mut body_preview = body.chars().take(2000).collect::<String>();
    if body.chars().count() > 2000 {
        body_preview.push_str("…");
    }

    if !status.is_success() {
        return Err(ApiError::from(AppError::Network(format!(
            "Custom tool endpoint returned {}. {}",
            status, body_preview
        ))));
    }

    Ok(TestCustomToolResponse {
        final_url: url.to_string(),
        status: status.as_u16(),
        content_type,
        body_preview,
    })
}

#[tauri::command]
#[specta::specta]
pub async fn get_settings(container: State<'_, Container>) -> Result<serde_json::Value, ApiError> {
    let settings = container
        .get_settings_use_case()
        .execute()
        .await
        .map_err(ApiError::from)?;

    serde_json::to_value(settings).map_err(|e| {
        ApiError::from(AppError::Serialization(format!(
            "Failed to serialize settings: {}",
            e
        )))
    })
}

#[tauri::command]
#[specta::specta]
pub async fn get_settings_category(
    category: String,
    container: State<'_, Container>,
) -> Result<serde_json::Value, ApiError> {
    let category = parse_category(&category).map_err(ApiError::from)?;
    container
        .get_settings_use_case()
        .get_category(category)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn update_settings(
    settings: UpdateSettingsRequest,
    container: State<'_, Container>,
) -> Result<serde_json::Value, ApiError> {
    let category = match settings.category {
        Some(category) => Some(parse_category(&category).map_err(ApiError::from)?),
        None => None,
    };
    let invalidate_llm_cache = matches!(
        category,
        Some(crate::features::settings::dto::SettingsCategory::Llm)
    );

    let updated = container
        .update_settings_use_case()
        .execute(UpdateSettingsRequestDto {
            category,
            updates: settings.updates,
        })
        .await
        .map_err(ApiError::from)?;

    if invalidate_llm_cache {
        container.invalidate_llm_cache();
        container.invalidate_router_llm_cache();
        container.refresh_custom_tools_from_settings().await;
    }

    serde_json::to_value(updated).map_err(|e| {
        ApiError::from(AppError::Serialization(format!(
            "Failed to serialize settings: {}",
            e
        )))
    })
}

#[tauri::command]
#[specta::specta]
pub async fn reset_settings(
    container: State<'_, Container>,
) -> Result<serde_json::Value, ApiError> {
    let reset = container
        .reset_settings_use_case()
        .reset_all()
        .await
        .map_err(ApiError::from)?;

    serde_json::to_value(reset).map_err(|e| {
        ApiError::from(AppError::Serialization(format!(
            "Failed to serialize settings: {}",
            e
        )))
    })
}

#[tauri::command]
#[specta::specta]
pub async fn export_settings(container: State<'_, Container>) -> Result<String, ApiError> {
    let export_path = temp_settings_path("lattice-settings-export");
    let request = ExportSettingsRequestDto {
        path: export_path.to_string_lossy().to_string(),
    };

    container
        .export_settings_use_case()
        .execute(request)
        .await
        .map_err(ApiError::from)?;

    let settings_json = tokio::fs::read_to_string(&export_path).await.map_err(|e| {
        ApiError::from(AppError::Io {
            message: format!("Failed to read exported settings: {}", e),
            kind: e.kind().to_string(),
        })
    })?;

    let _ = tokio::fs::remove_file(&export_path).await;

    Ok(settings_json)
}

#[tauri::command]
#[specta::specta]
pub async fn import_settings(
    request: ImportSettingsRequest,
    container: State<'_, Container>,
) -> Result<serde_json::Value, ApiError> {
    let import_path = temp_settings_path("lattice-settings-import");
    tokio::fs::write(&import_path, request.settings)
        .await
        .map_err(|e| {
            ApiError::from(AppError::Io {
                message: format!("Failed to write import settings: {}", e),
                kind: e.kind().to_string(),
            })
        })?;

    let import_request = ImportSettingsRequestDto {
        path: import_path.to_string_lossy().to_string(),
        merge: request.merge.unwrap_or(false),
    };

    let imported = container
        .import_settings_use_case()
        .execute(import_request)
        .await
        .map_err(ApiError::from)?;

    let _ = tokio::fs::remove_file(&import_path).await;

    serde_json::to_value(imported.settings).map_err(|e| {
        ApiError::from(AppError::Serialization(format!(
            "Failed to serialize settings: {}",
            e
        )))
    })
}

#[tauri::command]
#[specta::specta]
pub fn get_system_theme<R: Runtime>(window: tauri::Window<R>) -> Result<String, ApiError> {
    let theme_value = match window.theme() {
        Ok(Theme::Dark) => "dark",
        Ok(Theme::Light) => "light",
        _ => "system",
    };

    Ok(theme_value.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn validate_folder_path(
    path: String,
    container: State<'_, Container>,
) -> Result<bool, ApiError> {
    let use_case = container.validate_settings_use_case();
    Ok(use_case.validate_folder_path(&path))
}

/// Adds a folder to the indexed-paths watch list.
///
/// Thin wrapper around `UpdateSettingsUseCase` — read current
/// indexed_paths, append, replay through update. The use case enforces
/// CWE-22 path validation on the resulting array, so a malicious
/// frontend cannot bypass via a raw `update_settings` call either.
///
/// Idempotent: adding a path that's already present is a no-op (no
/// duplicate entry).
#[tauri::command]
#[specta::specta]
pub async fn add_watch_folder(
    path: String,
    container: State<'_, Container>,
) -> Result<(), ApiError> {
    let trimmed = path.trim().to_string();
    if trimmed.is_empty() {
        return Err(ApiError::from(AppError::InvalidInput(
            "watch folder path cannot be empty".to_string(),
        )));
    }

    // Read current indexed_paths from the SSOT.
    let current = container
        .get_settings_use_case()
        .execute()
        .await
        .map_err(ApiError::from)?;

    if current.indexing.indexed_paths.contains(&trimmed) {
        // Already present — idempotent no-op.
        return Ok(());
    }

    let mut next_paths = current.indexing.indexed_paths.clone();
    next_paths.push(trimmed);

    let mut updates = HashMap::new();
    updates.insert(
        "indexedPaths".to_string(),
        serde_json::to_value(next_paths).map_err(|e| {
            ApiError::from(AppError::Serialization(format!(
                "Failed to serialize indexedPaths: {e}"
            )))
        })?,
    );

    container
        .update_settings_use_case()
        .update_category(SettingsCategory::Indexing, updates)
        .await
        .map_err(ApiError::from)?;

    Ok(())
}

/// Removes a folder from the indexed-paths watch list.
///
/// Thin wrapper around `UpdateSettingsUseCase`. Idempotent: removing a
/// path that isn't in the list is a no-op (no error).
#[tauri::command]
#[specta::specta]
pub async fn remove_watch_folder(
    path: String,
    container: State<'_, Container>,
) -> Result<(), ApiError> {
    let trimmed = path.trim().to_string();
    if trimmed.is_empty() {
        return Err(ApiError::from(AppError::InvalidInput(
            "watch folder path cannot be empty".to_string(),
        )));
    }

    let current = container
        .get_settings_use_case()
        .execute()
        .await
        .map_err(ApiError::from)?;

    if !current.indexing.indexed_paths.contains(&trimmed) {
        // Not present — idempotent no-op.
        return Ok(());
    }

    let next_paths: Vec<String> = current
        .indexing
        .indexed_paths
        .iter()
        .filter(|p| **p != trimmed)
        .cloned()
        .collect();

    let mut updates = HashMap::new();
    updates.insert(
        "indexedPaths".to_string(),
        serde_json::to_value(next_paths).map_err(|e| {
            ApiError::from(AppError::Serialization(format!(
                "Failed to serialize indexedPaths: {e}"
            )))
        })?,
    );

    container
        .update_settings_use_case()
        .update_category(SettingsCategory::Indexing, updates)
        .await
        .map_err(ApiError::from)?;

    Ok(())
}

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("settings")
        .invoke_handler(tauri::generate_handler![
            get_settings,
            get_settings_category,
            update_settings,
            reset_settings,
            export_settings,
            import_settings,
            get_system_theme,
            validate_folder_path,
            test_ollama_connection,
            test_custom_tool,
            add_watch_folder,
            remove_watch_folder,
        ])
        .build()
}
