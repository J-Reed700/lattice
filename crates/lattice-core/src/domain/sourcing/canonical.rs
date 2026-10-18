//! Canonical keys: one key per resource, so an archived copy, a
//! `utm_`-tagged link and a page's `rel=canonical` target fold into one lead.
//!
//! The key is for deduplication only, never for fetching. It applies RFC 3986
//! §6.2.2 normalisation (case, percent-encoding, dot segments, default port),
//! drops fragments, userinfo and tracking parameters, sorts the query, and
//! unwraps Wayback Machine URLs to the page they archive. The archive reader
//! in provenance calls [`unwrap_archive`] too, so both agree on what an
//! archive URL is a copy of.

use std::fmt;

use chrono::{DateTime, NaiveDate, NaiveDateTime, NaiveTime, Utc};
use url::Url;

use super::lead::{Lead, LeadKind};

/// Query parameters that only track the click. `ref` and `source` are kept
/// in the list for parity with the search path's dedupe today.
pub const TRACKING_PARAMS: &[&str] = &[
    "gclid", "fbclid", "msclkid", "_hsenc", "_hsmi", "mc_cid", "mc_eid", "ref", "source",
];

/// Query parameters with this prefix only track the click.
pub const TRACKING_PARAM_PREFIX: &str = "utm_";

/// Hosts that serve the Wayback Machine's `/web/<timestamp>/<url>` copies.
pub const WAYBACK_HOSTS: &[&str] = &["web.archive.org", "wayback.archive.org"];

/// A resource's dedupe key.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CanonicalKey(String);

impl CanonicalKey {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for CanonicalKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A Wayback copy and the page it archives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchiveCopy {
    pub original: Url,
    /// The capture time the URL names, when it names one.
    pub captured_at: Option<DateTime<Utc>>,
}

/// The key for a URL. Archive URLs key as the page they archive. `http` and
/// `https` share a key: archives record whichever scheme the crawler saw, and
/// a dedupe key never decides what is fetched.
pub fn key(url: &Url) -> CanonicalKey {
    if let Some(copy) = unwrap_archive(url) {
        return normalised(&copy.original);
    }
    normalised(url)
}

/// The key for a lead: its declared `rel=canonical` first, then the original
/// of an archived copy, then its own URL.
pub fn lead_key(lead: &Lead) -> CanonicalKey {
    if let Some(canonical) = &lead.rel_canonical {
        return key(canonical);
    }
    if let LeadKind::ArchivedCopy { original, .. } = &lead.kind {
        return key(original);
    }
    key(&lead.url)
}

/// The page a Wayback URL archives, and the capture time it names:
/// `web.archive.org/web/<timestamp>[<modifier>_]/<original>`.
pub fn unwrap_archive(url: &Url) -> Option<ArchiveCopy> {
    let host = url.host_str()?.to_ascii_lowercase();
    if !WAYBACK_HOSTS.contains(&host.as_str()) || !url.path().starts_with("/web/") {
        return None;
    }
    // The original keeps its own query, which `Url` parsed as the archive
    // URL's, so read it from the serialised form.
    let serialised = url.as_str();
    let after_web = serialised.get(serialised.find("/web/")? + "/web/".len()..)?;
    let (stamp, original) = after_web.split_once('/')?;
    let original = Url::parse(&repair_scheme(original)?).ok()?;
    if !matches!(original.scheme(), "http" | "https") || original.host_str().is_none() {
        return None;
    }
    Some(ArchiveCopy {
        original,
        captured_at: parse_wayback_timestamp(stamp),
    })
}

/// Archived originals arrive as `https://x`, `https:/x` (a proxy collapsed
/// the slashes) or bare `x`; all become `scheme://x`. Any other scheme is not
/// a web page and yields `None`.
fn repair_scheme(original: &str) -> Option<String> {
    for scheme in ["https:", "http:"] {
        if let Some(rest) = original.strip_prefix(scheme) {
            return Some(format!("{scheme}//{}", rest.trim_start_matches('/')));
        }
    }
    let first_segment = original.split('/').next().unwrap_or_default();
    (!first_segment.ends_with(':')).then(|| format!("http://{original}"))
}

/// `YYYY[MM[DD[hh[mm[ss]]]]]`, optionally followed by a modifier (`id_`,
/// `if_`, `*`); missing parts take their first value.
fn parse_wayback_timestamp(stamp: &str) -> Option<DateTime<Utc>> {
    let digits: String = stamp.chars().take_while(char::is_ascii_digit).collect();
    if digits.len() < 4 {
        return None;
    }
    let part = |range: std::ops::Range<usize>, first: u32| -> Option<u32> {
        match digits.get(range) {
            Some(text) if !text.is_empty() => text.parse().ok(),
            _ => Some(first),
        }
    };
    let year: i32 = digits.get(0..4)?.parse().ok()?;
    let date = NaiveDate::from_ymd_opt(year, part(4..6, 1)?, part(6..8, 1)?)?;
    let time = NaiveTime::from_hms_opt(part(8..10, 0)?, part(10..12, 0)?, part(12..14, 0)?)?;
    Some(NaiveDateTime::new(date, time).and_utc())
}

fn normalised(url: &Url) -> CanonicalKey {
    if !matches!(url.scheme(), "http" | "https") {
        return CanonicalKey(url.as_str().to_string());
    }
    // `Url` already lowercases the host, removes dot segments and drops the
    // scheme's default port.
    let host = url.host_str().unwrap_or_default();
    let port = url
        .port()
        .map(|port| format!(":{port}"))
        .unwrap_or_default();

    let path = normalise_percent_encoding(url.path());
    let path = match path.trim_end_matches('/') {
        "" => "/".to_string(),
        trimmed => trimmed.to_string(),
    };

    let mut pairs: Vec<(String, String)> = url
        .query_pairs()
        .filter(|(name, _)| !is_tracking_param(name))
        .map(|(name, value)| (name.into_owned(), value.into_owned()))
        .collect();
    pairs.sort();
    let query = if pairs.is_empty() {
        String::new()
    } else {
        let mut serializer = url::form_urlencoded::Serializer::new(String::new());
        for (name, value) in &pairs {
            serializer.append_pair(name, value);
        }
        format!("?{}", serializer.finish())
    };

    CanonicalKey(format!("//{host}{port}{path}{query}"))
}

fn is_tracking_param(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.starts_with(TRACKING_PARAM_PREFIX) || TRACKING_PARAMS.contains(&lower.as_str())
}

/// RFC 3986 §6.2.2.1-2: decode percent-encoded unreserved characters and
/// uppercase the hex digits of the rest.
fn normalise_percent_encoding(path: &str) -> String {
    let bytes = path.as_bytes();
    let mut out = String::with_capacity(path.len());
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes.get(index).copied().unwrap_or_default();
        let escape = (byte == b'%')
            .then(|| bytes.get(index + 1..index + 3))
            .flatten()
            .and_then(|hex| std::str::from_utf8(hex).ok())
            .and_then(|hex| u8::from_str_radix(hex, 16).ok());
        match escape {
            Some(decoded) if is_unreserved(decoded) => {
                out.push(char::from(decoded));
                index += 3;
            }
            Some(decoded) => {
                out.push_str(&format!("%{decoded:02X}"));
                index += 3;
            }
            None => {
                out.push(char::from(byte));
                index += 1;
            }
        }
    }
    out
}

fn is_unreserved(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~')
}

#[cfg(test)]
#[path = "canonical_tests.rs"]
mod tests;
