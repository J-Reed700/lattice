//! Tool-argument normalization shared by the executor dispatch path.
//!
//! Keeps the canonicalization of LLM-supplied date filters next to the
//! dispatch table in [`super`], so every handler receives already-normalized
//! arguments.

use super::{FunctionExecutor, TOOL_LIST_DOCUMENTS, TOOL_SEMANTIC_SEARCH};
use chrono::{DateTime, NaiveDate, NaiveDateTime, Utc};

impl FunctionExecutor {
    pub(super) fn parse_datetime(value: &str) -> Option<DateTime<Utc>> {
        if let Ok(parsed) = DateTime::parse_from_rfc3339(value) {
            return Some(parsed.with_timezone(&Utc));
        }

        NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S")
            .or_else(|_| NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S"))
            .or_else(|_| NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S%.f"))
            .ok()
            .map(|naive| DateTime::<Utc>::from_naive_utc_and_offset(naive, Utc))
    }

    /// Normalize tool date filters to RFC3339 to satisfy JSON schema validation.
    ///
    /// LLMs sometimes emit date-only values (e.g. `2000-01-01`) or naive datetime
    /// strings. We canonicalize those into UTC RFC3339 before validation/deserialization.
    pub(super) fn normalize_tool_arguments(
        function_name: &str,
        arguments: &serde_json::Value,
    ) -> serde_json::Value {
        if !matches!(function_name, TOOL_SEMANTIC_SEARCH | TOOL_LIST_DOCUMENTS) {
            return arguments.clone();
        }

        let mut normalized = arguments.clone();
        let Some(object) = normalized.as_object_mut() else {
            return normalized;
        };

        Self::normalize_datetime_filter(object, "date_from", false);
        Self::normalize_datetime_filter(object, "date_to", true);

        normalized
    }

    fn normalize_datetime_filter(
        object: &mut serde_json::Map<String, serde_json::Value>,
        key: &str,
        end_of_day: bool,
    ) {
        let Some(raw_value) = object
            .get(key)
            .and_then(|value| value.as_str())
            .map(str::trim)
            .map(str::to_owned)
        else {
            return;
        };

        if raw_value.is_empty() {
            object.remove(key);
            return;
        }

        if let Some(parsed) = Self::parse_datetime(raw_value.as_str()) {
            object.insert(
                key.to_string(),
                serde_json::Value::String(parsed.to_rfc3339()),
            );
            return;
        }

        let Ok(date_only) = NaiveDate::parse_from_str(raw_value.as_str(), "%Y-%m-%d") else {
            return;
        };

        let naive_dt = if end_of_day {
            date_only.and_hms_opt(23, 59, 59)
        } else {
            date_only.and_hms_opt(0, 0, 0)
        };

        if let Some(naive) = naive_dt {
            let datetime = DateTime::<Utc>::from_naive_utc_and_offset(naive, Utc);
            object.insert(
                key.to_string(),
                serde_json::Value::String(datetime.to_rfc3339()),
            );
        }
    }
}
