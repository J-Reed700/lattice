//! Dates a page declares about itself, chosen by a fixed precedence (§4, §9).
//!
//! Only machine-readable fields are read: JSON-LD, `meta` tags and
//! `<time datetime>` attributes. A field that is present but does not parse
//! is recorded as malformed rather than guessed at.

use chrono::{DateTime, NaiveDate, NaiveDateTime};
use lattice_core::application::contracts::web::DateBasis;

/// Datetime layouts pages write besides RFC 3339, tried in order: an offset
/// without a colon (`+0000`), and local times taken as UTC.
const ZONED_LAYOUTS: &[&str] = &["%Y-%m-%dT%H:%M:%S%.f%z", "%Y-%m-%dT%H:%M%z"];
const LOCAL_LAYOUTS: &[&str] = &[
    "%Y-%m-%dT%H:%M:%S%.f",
    "%Y-%m-%dT%H:%M",
    "%Y-%m-%d %H:%M:%S%.f",
    "%Y-%m-%d %H:%M",
];
/// Calendar dates without a time, taken as midnight UTC. `YYYY/MM/DD` is the
/// Highwire `citation_publication_date` form.
const DATE_LAYOUTS: &[&str] = &["%Y-%m-%d", "%Y/%m/%d"];

/// One place a date may be declared, read before the others below it.
pub(crate) struct DateField {
    /// Where it was found, as a malformed entry names it.
    pub field: String,
    pub raw: String,
    pub basis: DateBasis,
}

/// The chosen date and every field that was present but did not parse.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ChosenDate {
    pub at: Option<(i64, DateBasis)>,
    pub malformed: Vec<String>,
}

/// The first field that parses, in the order given.
pub(crate) fn choose(fields: Vec<DateField>) -> ChosenDate {
    let mut chosen = ChosenDate::default();
    for field in fields {
        match parse_date(&field.raw) {
            Some(at) if chosen.at.is_none() => chosen.at = Some((at, field.basis)),
            Some(_) => {}
            None => chosen.malformed.push(field.field),
        }
    }
    chosen
}

/// A page date as unix seconds.
pub(crate) fn parse_date(raw: &str) -> Option<i64> {
    let text = raw.trim();
    if text.is_empty() {
        return None;
    }
    if let Ok(at) = DateTime::parse_from_rfc3339(text) {
        return Some(at.timestamp());
    }
    if let Some(at) = ZONED_LAYOUTS
        .iter()
        .find_map(|layout| DateTime::parse_from_str(text, layout).ok())
    {
        return Some(at.timestamp());
    }
    if let Some(at) = LOCAL_LAYOUTS
        .iter()
        .find_map(|layout| NaiveDateTime::parse_from_str(text, layout).ok())
    {
        return Some(at.and_utc().timestamp());
    }
    if let Some(day) = DATE_LAYOUTS
        .iter()
        .find_map(|layout| NaiveDate::parse_from_str(text, layout).ok())
    {
        return day.and_hms_opt(0, 0, 0).map(|at| at.and_utc().timestamp());
    }
    DateTime::parse_from_rfc2822(text)
        .ok()
        .map(|at| at.timestamp())
}

/// An HTTP date header (`Last-Modified`), RFC 7231's IMF-fixdate.
pub(crate) fn parse_http_date(raw: &str) -> Option<i64> {
    DateTime::parse_from_rfc2822(raw.trim())
        .ok()
        .map(|at| at.timestamp())
}
