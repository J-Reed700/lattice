//! Tauri API boundary helpers.
//!
//! Provides helpers for converting ApiResult into Tauri-friendly JSON values.

use crate::shared::api_result::ApiResult;
use serde::Serialize;

pub trait TauriResultBoundary {
    fn to_tauri_result(self) -> Result<serde_json::Value, String>;
}

impl<T> TauriResultBoundary for ApiResult<T>
where
    T: Serialize,
{
    fn to_tauri_result(self) -> Result<serde_json::Value, String> {
        serde_json::to_value(&self).map_err(|e| e.to_string())
    }
}
