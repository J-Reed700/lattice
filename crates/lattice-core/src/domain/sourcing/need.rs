//! What a caller asks the source-finding service for: a [`SourceNeed`], never
//! a provider query. Only the router turns needs into provider requests.

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use url::Url;

/// The kind of source a need asks for. It selects the router's recipe (which
/// finders run in which wave), so it is a closed vocabulary: the chat tool's
/// `kind` enum, the lesson claim profile and the eval set all use these names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NeedKind {
    /// Pages about a question; the chat's default.
    Web,
    /// The subject owner's own documentation.
    OfficialDocs,
    /// A package's registry record, repository and docs.
    Package,
    /// A paper by title, DOI or arXiv id, or a measurement it reports.
    Paper,
    /// A standard from its body (RFC, W3C, WHATWG, ECMA).
    Standard,
    /// A law or regulation from its official publisher.
    Law,
    /// A question-and-answer or discussion thread.
    Community,
    /// An encyclopedia article and its references.
    Encyclopedia,
    /// A dated archived copy of a URL.
    Archive,
    /// The links a page makes, followed to what it cites.
    Leads,
}

impl NeedKind {
    /// Every kind, in declaration order (tests and the eval runner iterate it).
    pub const ALL: [NeedKind; 10] = [
        NeedKind::Web,
        NeedKind::OfficialDocs,
        NeedKind::Package,
        NeedKind::Paper,
        NeedKind::Standard,
        NeedKind::Law,
        NeedKind::Community,
        NeedKind::Encyclopedia,
        NeedKind::Archive,
        NeedKind::Leads,
    ];

    /// The wire name (`official_docs`), as the tool schema and logs spell it.
    pub fn as_str(self) -> &'static str {
        match self {
            NeedKind::Web => "web",
            NeedKind::OfficialDocs => "official_docs",
            NeedKind::Package => "package",
            NeedKind::Paper => "paper",
            NeedKind::Standard => "standard",
            NeedKind::Law => "law",
            NeedKind::Community => "community",
            NeedKind::Encyclopedia => "encyclopedia",
            NeedKind::Archive => "archive",
            NeedKind::Leads => "leads",
        }
    }
}

/// A request for sources. Callers state what they need; the router decides
/// which finders run and with what scope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceNeed {
    pub kind: NeedKind,
    /// Public subject terms only (the planner rule stays: nothing private).
    pub query: String,
    /// The entity that owns the answer, when known.
    pub subject: Option<SubjectHint>,
    /// Search within this site; enforced on every lead and after redirects.
    pub scope: Option<Origin>,
    pub freshness: Freshness,
    /// Leads the caller can use, derived from its window or budget.
    pub want: usize,
    /// Rewordings the caller's model wrote, tried only by reformulated waves.
    /// Reformulation is the model's job; the service never word-mines.
    pub alternatives: Vec<String>,
    /// Further kinds whose first-wave finders join wave 1 (chat's
    /// "include Wikipedia" is `also: [Encyclopedia]`).
    pub also: Vec<NeedKind>,
}

impl SourceNeed {
    /// The package ecosystem the need names through an identifier, if any.
    pub fn ecosystem(&self) -> Option<Ecosystem> {
        self.subject.as_ref().and_then(|subject| {
            subject.identifiers.iter().find_map(|id| match id {
                Identifier::PackageName { ecosystem, .. } => Some(*ecosystem),
                _ => None,
            })
        })
    }
}

/// The subject a need is about, so owner-first routing can find its owner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubjectHint {
    pub name: String,
    pub kind: SubjectKind,
    pub identifiers: Vec<Identifier>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubjectKind {
    Software,
    Organisation,
    Paper,
    Law,
    Standard,
    Person,
    Other,
}

impl SubjectKind {
    /// The wire name (`software`), as the tools, logs and cache keys spell it.
    pub fn as_str(self) -> &'static str {
        match self {
            SubjectKind::Software => "software",
            SubjectKind::Organisation => "organisation",
            SubjectKind::Paper => "paper",
            SubjectKind::Law => "law",
            SubjectKind::Standard => "standard",
            SubjectKind::Person => "person",
            SubjectKind::Other => "other",
        }
    }
}

/// An exact identifier for a subject or document.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Identifier {
    Doi(String),
    Arxiv(String),
    PackageName {
        ecosystem: Ecosystem,
        name: String,
    },
    Repo {
        host: String,
        owner: String,
        name: String,
    },
    Rfc(u32),
    Celex(String),
    WikidataQid(String),
    Url(Url),
}

/// A package registry's ecosystem.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Ecosystem {
    Crates,
    PyPi,
    Npm,
    NuGet,
    RubyGems,
    Maven,
    Go,
}

/// A host, optionally narrowed to a path prefix (`github.com/dotnet`). Used
/// both as a search scope and as the owner a finder asserts for a subject.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Origin {
    host: String,
    path_prefix: Option<String>,
}

impl Origin {
    /// Builds an origin from a host and optional path prefix; the host is
    /// lowercased and a leading `www.` kept (it is a different host).
    pub fn new(host: &str, path_prefix: Option<&str>) -> Self {
        let path_prefix = path_prefix
            .map(|prefix| prefix.trim_matches('/'))
            .filter(|prefix| !prefix.is_empty())
            .map(|prefix| format!("/{prefix}"));
        Self {
            host: host.trim().trim_end_matches('.').to_ascii_lowercase(),
            path_prefix,
        }
    }

    /// Parses `host` or `host/path`, as the tools' `site` argument spells it.
    pub fn parse(site: &str) -> Option<Self> {
        let site = site.trim();
        let site = site
            .strip_prefix("https://")
            .or_else(|| site.strip_prefix("http://"))
            .unwrap_or(site);
        let (host, path) = match site.split_once('/') {
            Some((host, path)) => (host, Some(path)),
            None => (site, None),
        };
        if host.is_empty() || host.contains(char::is_whitespace) {
            return None;
        }
        Some(Self::new(host, path))
    }

    pub fn host(&self) -> &str {
        &self.host
    }

    pub fn path_prefix(&self) -> Option<&str> {
        self.path_prefix.as_deref()
    }

    /// True when `url` is on this host or one of its subdomains, and under
    /// the path prefix when there is one (segment-wise, so `/dotnet` does not
    /// cover `/dotnetfoundation`).
    pub fn covers(&self, url: &Url) -> bool {
        let Some(host) = url.host_str() else {
            return false;
        };
        let host = host.to_ascii_lowercase();
        let on_host = host == self.host
            || host
                .strip_suffix(self.host.as_str())
                .is_some_and(|rest| rest.ends_with('.'));
        if !on_host {
            return false;
        }
        match &self.path_prefix {
            None => true,
            Some(prefix) => {
                let path = url.path();
                path.strip_prefix(prefix.as_str())
                    .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
            }
        }
    }
}

impl std::fmt::Display for Origin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}{}",
            self.host,
            self.path_prefix.as_deref().unwrap_or("")
        )
    }
}

/// How current the sources must be.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Freshness {
    #[default]
    Any,
    /// Only pages published after this date; engines are asked for recency
    /// where they support it and older cached pages are bypassed.
    After(NaiveDate),
    /// The page as it stood on this date: routes to the archive finders.
    AsOf(NaiveDate),
}
