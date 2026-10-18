//! The identity of a publishing origin: who a URL belongs to after
//! redirects, archive wrappers and canonical links are accounted for (§4,
//! §7). Two versions share an origin when their [`OriginKey`]s are equal.

use chrono::{NaiveDate, NaiveDateTime};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use url::{Host, Url};

/// Identity of a publishing origin after canonicalisation and archive
/// unwrapping.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Origin {
    pub key: OriginKey,
    /// The host the page was served from (the original's host for an
    /// unwrapped archive), lower-case ASCII.
    #[serde(default)]
    pub host: Option<String>,
    /// The registrable domain by the Public Suffix List (compiled in, no
    /// I/O). `None` for an IP address, a single-label host or a suffix the
    /// list does not know.
    #[serde(default)]
    pub registrable_domain: Option<String>,
    pub tld_registry: TldRegistry,
    /// `rel=canonical` / `og:url` when it names the same registrable domain;
    /// a cross-domain canonical is a `CanonicalOf` relation instead.
    #[serde(default)]
    pub canonical_url: Option<String>,
    /// Set when the URL was an archive or cache wrapper around another URL.
    #[serde(default)]
    pub archived: Option<ArchivedFrom>,
}

/// `web:<registrable domain>` (or `web:<host>` when there is none),
/// `file:<sha256 of the file name>`, or `pasted:<source version id>`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct OriginKey(pub String);

/// What an archive or cache wrapper said about the page it wraps.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArchivedFrom {
    pub original_url: String,
    /// Unix seconds, when the wrapper's URL carries the capture time.
    #[serde(default)]
    pub archived_at: Option<i64>,
    pub archive_host: String,
}

/// Who may register under the host's suffix. A restricted registry vets its
/// registrants, which makes the suffix a signal about the publisher; an open
/// one says nothing. A country code alone is never a signal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TldRegistry {
    Open,
    Restricted {
        kind: RegistryKind,
        suffix: String,
    },
    /// No public suffix: an IP address, a private host or an unknown TLD.
    Unknown,
}

/// The class of registrant a restricted registry admits
/// (`registry_suffixes.txt`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RegistryKind {
    Government,
    Education,
    Military,
    Intergovernmental,
}

wire_names!(RegistryKind {
    Government => "government",
    Education => "education",
    Military => "military",
    Intergovernmental => "intergovernmental",
});

/// The restricted-registry table (`registry_suffixes.txt`): its source and
/// date are in its header.
const REGISTRY_SUFFIXES: &str = include_str!("registry_suffixes.txt");

/// Hosts of the Wayback Machine, whose capture URLs are
/// `/web/<timestamp><flags>/<original>`.
const WAYBACK_HOSTS: [&str; 2] = ["web.archive.org", "wayback.archive.org"];

/// Hosts of archive.today and its mirrors, whose full capture URLs are
/// `/<timestamp>/<original>` or `/o/<id>/<original>`. The short form
/// `/<id>` names no original and is not unwrapped.
const ARCHIVE_TODAY_HOSTS: [&str; 7] = [
    "archive.ph",
    "archive.today",
    "archive.is",
    "archive.li",
    "archive.vn",
    "archive.md",
    "archive.fo",
];

/// Google's cache wrapper: `/search?q=cache:<id>:<original>`.
const GOOGLE_CACHE_HOST: &str = "webcache.googleusercontent.com";

/// Why a URL has no web origin.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum OriginError {
    #[error("not a web address: {0}")]
    NotWeb(String),
    #[error("the address has no host: {0}")]
    NoHost(String),
}

/// What a page's canonical link means for its origin.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CanonicalLink {
    /// Same registrable domain: recorded as the origin's canonical URL.
    SameOrigin,
    /// Another domain: syndication, recorded as a `CanonicalOf` relation;
    /// the page's own origin stays its origin.
    CrossOrigin { url: String },
    /// Not a web address; ignored.
    Ignored,
}

impl Origin {
    /// The origin of a fetched page's final URL: archive and cache wrappers
    /// unwrapped, the registrable domain by the Public Suffix List, the
    /// suffix's registry class by `registry_suffixes.txt`. No I/O.
    pub fn of_url(url: &str) -> Result<Self, OriginError> {
        let parsed = parse_web(url)?;
        // An archive of an archive, or a wrapped address that does not
        // parse, is judged as the archive host it is.
        if let Some(archived) = unwrap_archive(&parsed) {
            if let Ok(original) = parse_web(&archived.original_url) {
                if unwrap_archive(&original).is_none() {
                    return Ok(Self {
                        archived: Some(archived),
                        ..Self::of_host(&original)?
                    });
                }
            }
        }
        Self::of_host(&parsed)
    }

    /// The name site overrides are keyed by: the registrable domain, or
    /// the host when there is none. `None` for files and pasted text.
    pub fn site(&self) -> Option<&str> {
        self.registrable_domain.as_deref().or(self.host.as_deref())
    }

    /// A library file's origin: its name's sha256, since a file carries no
    /// verifiable publisher of its own.
    pub fn of_file(file_name: &str) -> Self {
        Self::opaque(format!(
            "file:{}",
            hex::encode(Sha256::digest(file_name.as_bytes()))
        ))
    }

    /// Pasted text's origin: the version itself.
    pub fn pasted(source_version_id: &str) -> Self {
        Self::opaque(format!("pasted:{source_version_id}"))
    }

    /// Applies a `rel=canonical` / `og:url` link: kept when it names the same
    /// registrable domain, returned as a cross-origin link otherwise.
    pub fn apply_canonical(&mut self, canonical_url: &str) -> CanonicalLink {
        let Ok(target) = Self::of_url(canonical_url) else {
            return CanonicalLink::Ignored;
        };
        if target.key == self.key {
            self.canonical_url = Some(canonical_url.to_owned());
            CanonicalLink::SameOrigin
        } else {
            CanonicalLink::CrossOrigin {
                url: canonical_url.to_owned(),
            }
        }
    }

    fn opaque(key: String) -> Self {
        Self {
            key: OriginKey(key),
            host: None,
            registrable_domain: None,
            tld_registry: TldRegistry::Unknown,
            canonical_url: None,
            archived: None,
        }
    }

    fn of_host(url: &Url) -> Result<Self, OriginError> {
        let host = match url.host() {
            Some(Host::Domain(domain)) => domain.trim_end_matches('.').to_ascii_lowercase(),
            Some(Host::Ipv4(address)) => address.to_string(),
            Some(Host::Ipv6(address)) => format!("[{address}]"),
            None => return Err(OriginError::NoHost(url.to_string())),
        };
        let is_ip = !matches!(url.host(), Some(Host::Domain(_)));
        let registrable_domain = if is_ip {
            None
        } else {
            registrable_domain(&host)
        };
        let tld_registry = match (restricted_registry(&host), &registrable_domain) {
            _ if is_ip => TldRegistry::Unknown,
            (Some((kind, suffix)), _) => TldRegistry::Restricted { kind, suffix },
            (None, Some(_)) => TldRegistry::Open,
            (None, None) => TldRegistry::Unknown,
        };
        let key = format!("web:{}", registrable_domain.as_deref().unwrap_or(&host));
        Ok(Self {
            key: OriginKey(key),
            host: Some(host),
            registrable_domain,
            tld_registry,
            canonical_url: None,
            archived: None,
        })
    }
}

fn parse_web(url: &str) -> Result<Url, OriginError> {
    let parsed = Url::parse(url.trim()).map_err(|_| OriginError::NotWeb(url.to_owned()))?;
    match parsed.scheme() {
        "http" | "https" => Ok(parsed),
        _ => Err(OriginError::NotWeb(url.to_owned())),
    }
}

/// The registrable domain when the host's suffix is one the Public Suffix
/// List knows; `None` for an unknown TLD or a single label.
fn registrable_domain(host: &str) -> Option<String> {
    let domain = psl::domain(host.as_bytes())?;
    if !domain.suffix().is_known() {
        return None;
    }
    std::str::from_utf8(domain.as_bytes())
        .ok()
        .map(str::to_owned)
}

/// The longest label-aligned restricted suffix of `host`.
fn restricted_registry(host: &str) -> Option<(RegistryKind, String)> {
    REGISTRY_SUFFIXES
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let suffix = fields.next()?;
            let kind = RegistryKind::from_column(fields.next()?)?;
            let aligned = host == suffix
                || host
                    .strip_suffix(suffix)
                    .is_some_and(|rest| rest.ends_with('.'));
            aligned.then(|| (kind, suffix.to_owned()))
        })
        .max_by_key(|(_, suffix)| suffix.len())
}

/// The page an archive or cache URL wraps, when its form names one.
fn unwrap_archive(url: &Url) -> Option<ArchivedFrom> {
    let host = url.host_str()?.to_ascii_lowercase();
    let path = url.path();
    let (stamp, original) = if WAYBACK_HOSTS.contains(&host.as_str()) {
        let rest = path.strip_prefix("/web/")?;
        let (stamp, original) = rest.split_once('/')?;
        let digits: String = stamp.chars().take_while(char::is_ascii_digit).collect();
        (Some(digits), original.to_owned())
    } else if ARCHIVE_TODAY_HOSTS.contains(&host.as_str()) {
        let rest = path.strip_prefix('/')?;
        if let Some(after) = rest.strip_prefix("o/") {
            let (_, original) = after.split_once('/')?;
            (None, original.to_owned())
        } else {
            let (stamp, original) = rest.split_once('/')?;
            let digits: String = stamp.chars().filter(char::is_ascii_digit).collect();
            (Some(digits), original.to_owned())
        }
    } else if host == GOOGLE_CACHE_HOST {
        let query = url
            .query_pairs()
            .find(|(name, _)| name == "q")
            .map(|(_, value)| value.into_owned())?;
        let target = query.strip_prefix("cache:")?;
        let original = match target.find("http") {
            Some(at) => target.get(at..)?.to_owned(),
            None => target
                .split_once(':')
                .map_or(target, |(_, url)| url)
                .to_owned(),
        };
        (None, original)
    } else {
        return None;
    };
    let original = with_query(original, url);
    let original = restore_scheme(&original)?;
    Some(ArchivedFrom {
        original_url: original,
        archived_at: stamp.as_deref().and_then(archive_timestamp),
        archive_host: host,
    })
}

/// A wrapper's own query string belongs to the wrapped URL (Wayback and
/// archive.today keep it on the outer URL).
fn with_query(original: String, wrapper: &Url) -> String {
    match wrapper.query() {
        Some(query) if wrapper.host_str() != Some(GOOGLE_CACHE_HOST) => {
            format!("{original}?{query}")
        }
        _ => original,
    }
}

/// Wrappers collapse `http://` to `http:/` and sometimes drop the scheme.
fn restore_scheme(original: &str) -> Option<String> {
    for scheme in ["https:", "http:"] {
        if let Some(rest) = original.strip_prefix(scheme) {
            let rest = rest.trim_start_matches('/');
            return Some(format!("{scheme}//{rest}"));
        }
    }
    let first = original.split('/').next()?;
    first.contains('.').then(|| format!("http://{original}"))
}

/// `YYYYMMDDhhmmss` or any prefix of it as unix seconds (missing parts are
/// the earliest value).
fn archive_timestamp(digits: &str) -> Option<i64> {
    if digits.len() < 4 {
        return None;
    }
    let field = |from: usize, to: usize, default: u32| -> Option<u32> {
        match digits.get(from..to) {
            Some(text) if !text.is_empty() => text.parse().ok(),
            _ => Some(default),
        }
    };
    let year: i32 = digits.get(0..4)?.parse().ok()?;
    let date = NaiveDate::from_ymd_opt(year, field(4, 6, 1)?, field(6, 8, 1)?)?;
    let time: NaiveDateTime =
        date.and_hms_opt(field(8, 10, 0)?, field(10, 12, 0)?, field(12, 14, 0)?)?;
    Some(time.and_utc().timestamp())
}

#[cfg(test)]
#[path = "origin_tests.rs"]
mod tests;
