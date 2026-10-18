//! The identity of a publishing origin: who a URL belongs to after
//! redirects, archive wrappers and canonical links are accounted for (§4,
//! §7). Two versions share an origin when their [`OriginKey`]s are equal.

use serde::{Deserialize, Serialize};

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
