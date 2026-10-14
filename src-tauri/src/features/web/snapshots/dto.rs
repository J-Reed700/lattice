//! What the viewer and the events see of a frozen copy (§11).

use serde::{Deserialize, Serialize};

/// Emitted to the main window on every status flip and on progress.
pub const CHANGED_EVENT: &str = "web-page-snapshot:changed";
/// Emitted to the main window when a link inside a frozen copy is clicked.
pub const LINK_EVENT: &str = "web-page-snapshot:link";

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum WebPageSnapshotStatus {
    Pending,
    Ready,
    Failed,
}

impl WebPageSnapshotStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Ready => "ready",
            Self::Failed => "failed",
        }
    }

    pub fn parse(value: &str) -> Self {
        match value {
            "ready" => Self::Ready,
            "failed" => Self::Failed,
            _ => Self::Pending,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct WebPageSnapshotDto {
    pub id: String,
    pub conversation_id: String,
    pub requested_url: String,
    pub final_url: Option<String>,
    pub normalized_url: String,
    pub title: Option<String>,
    pub status: WebPageSnapshotStatus,
    /// "http" | "browser"
    pub method: Option<String>,
    pub captured_at: Option<String>,
    pub byte_size: Option<u64>,
    pub sha256: Option<String>,
    pub resource_count: u32,
    pub missing_resource_count: u32,
    pub partial: bool,
    pub partial_reasons: Vec<String>,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    pub favicon_data_url: Option<String>,
    /// While pending.
    pub progress_message: Option<String>,
    /// Ready rows: whether the file's hash was re-checked when this DTO was built.
    pub verified: Option<bool>,
}

/// Payload of [`CHANGED_EVENT`].
#[derive(Serialize, Deserialize, Clone, Debug, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct WebPageSnapshotChangedEvent {
    pub conversation_id: String,
    pub url: String,
    pub snapshot: WebPageSnapshotDto,
}

/// Payload of [`LINK_EVENT`].
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct WebPageSnapshotLinkEvent {
    pub snapshot_id: String,
    pub url: String,
}
