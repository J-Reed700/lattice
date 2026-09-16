//! Canonical database timestamp helpers.

use chrono::{DateTime, NaiveDateTime, SecondsFormat, Utc};

/// Formats a UTC timestamp as RFC3339 with fixed millisecond precision and a
/// `Z` suffix. Text values in this form preserve chronological ordering.
pub fn format_db_timestamp(timestamp: DateTime<Utc>) -> String {
    timestamp.to_rfc3339_opts(SecondsFormat::Millis, true)
}

/// Returns the current time in the canonical database representation.
pub fn now_db_timestamp() -> String {
    format_db_timestamp(Utc::now())
}

/// Parses both the canonical representation and legacy SQLite timestamp
/// strings. The legacy forms remain readable while migrations normalize them.
pub fn parse_db_timestamp(value: &str) -> Result<DateTime<Utc>, String> {
    if let Ok(parsed) = DateTime::parse_from_rfc3339(value) {
        return Ok(parsed.with_timezone(&Utc));
    }

    const LEGACY_FORMATS: [&str; 3] = [
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%dT%H:%M:%S",
        "%Y-%m-%d %H:%M:%S%.f",
    ];
    for format in LEGACY_FORMATS {
        if let Ok(parsed) = NaiveDateTime::parse_from_str(value, format) {
            return Ok(DateTime::<Utc>::from_naive_utc_and_offset(parsed, Utc));
        }
    }

    Err(format!("unsupported database timestamp: {value}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_canonical_offset_and_legacy_sqlite_timestamps() {
        for value in [
            "2026-08-01T12:30:45.123Z",
            "2026-08-01T07:30:45-05:00",
            "2026-08-01 12:30:45",
        ] {
            assert!(parse_db_timestamp(value).is_ok(), "failed to parse {value}");
        }
    }

    #[test]
    fn canonical_format_sorts_lexicographically() {
        let earlier = parse_db_timestamp("2026-08-01T12:00:00Z").expect("parse earlier");
        let later = parse_db_timestamp("2026-08-01T12:00:01Z").expect("parse later");
        assert!(format_db_timestamp(earlier) < format_db_timestamp(later));
    }
}
