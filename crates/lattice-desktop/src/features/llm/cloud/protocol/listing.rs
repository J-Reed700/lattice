//! What the providers' model lists share: the `data` page OpenAI and
//! Anthropic both return, and the filter for a model that talks text.

use lattice_core::shared::error::{AppError, Result};
use serde::Deserialize;

use crate::features::llm::cloud::catalog::CloudModelDto;

#[derive(Deserialize)]
pub(super) struct ApiModel {
    pub(super) id: String,
    display_name: Option<String>,
    pub(super) created: Option<i64>,
    shutdown_date: Option<String>,
}

#[derive(Deserialize)]
pub(super) struct ApiPage {
    pub(super) data: Vec<ApiModel>,
    #[serde(default)]
    pub(super) has_more: bool,
    pub(super) last_id: Option<String>,
}

impl ApiModel {
    /// Whether the model is still offered today.
    pub(super) fn is_offered(&self, today: &str) -> bool {
        self.shutdown_date
            .as_deref()
            .is_none_or(|date| date > today)
    }

    pub(super) fn into_dto(self) -> CloudModelDto {
        CloudModelDto {
            name: self
                .display_name
                .filter(|name| !name.is_empty())
                .unwrap_or_else(|| self.id.clone()),
            id: self.id,
        }
    }
}

/// Today's date, to tell a retired model from an offered one.
pub(super) fn today() -> String {
    chrono::Utc::now().date_naive().to_string()
}

/// The error for a model list that is not one.
pub(super) fn invalid_list(shown: &str) -> AppError {
    AppError::Network(format!("{shown} returned an invalid model list"))
}

/// A model list page read as the `data` format.
pub(super) fn api_page(value: serde_json::Value, shown: &str) -> Result<ApiPage> {
    serde_json::from_value(value).map_err(|_| invalid_list(shown))
}

pub(super) fn is_text_model(id: &str) -> bool {
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
