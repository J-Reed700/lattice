use chrono::Utc;
use serde::Serialize;

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ErrorEvent {
    pub error_type: String,
    pub message: String,
    pub context: serde_json::Value,
    pub timestamp: String,
}

pub fn track_error(error_type: &str, message: &str, context: serde_json::Value) {
    tracing::error!(
        error_type = %error_type,
        message = %message,
        context = ?context,
        timestamp = %Utc::now().to_rfc3339(),
        "Error occurred"
    );
}

pub fn track_error_with_source(
    error_type: &str,
    message: &str,
    source: &dyn std::error::Error,
    context: serde_json::Value,
) {
    tracing::error!(
        error_type = %error_type,
        message = %message,
        source = %source,
        context = ?context,
        timestamp = %Utc::now().to_rfc3339(),
        "Error occurred with source"
    );
}
