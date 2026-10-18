//! A finder's descriptor: what it finds, what it costs, what it needs, where
//! the query goes and how fast it may be called. The router, the settings
//! panel and the disclosures read this data; nothing about a finder is buried
//! in a match arm.

use std::borrow::Cow;
use std::fmt;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use super::need::{Ecosystem, NeedKind};

/// A finder's stable id (`duckduckgo`, `wikidata`, `crates_io`); settings and
/// logs key on it.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct FinderId(Cow<'static, str>);

impl FinderId {
    /// An id known at compile time, for descriptors.
    pub const fn from_static(id: &'static str) -> Self {
        Self(Cow::Borrowed(id))
    }

    /// An id read back from settings or a log.
    pub fn new(id: impl Into<String>) -> Self {
        Self(Cow::Owned(id.into()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for FinderId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Which wave of a kind's recipe a finder joins (§3.4's columns).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum WaveSlot {
    First,
    /// Runs when the first wave was thin.
    Second,
    Third,
}

impl WaveSlot {
    pub const ALL: [WaveSlot; 3] = [WaveSlot::First, WaveSlot::Second, WaveSlot::Third];

    /// Zero-based wave index.
    pub fn index(self) -> usize {
        match self {
            WaveSlot::First => 0,
            WaveSlot::Second => 1,
            WaveSlot::Third => 2,
        }
    }
}

/// One kind a finder serves and the wave it joins for that kind. General web
/// engines declare `Web` roles: `First` for keyless engines, `Second` for
/// opt-in ones; the router places them in other kinds' recipes by that role.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Role {
    pub kind: NeedKind,
    pub wave: WaveSlot,
}

/// What part of the world a finder reaches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Reach {
    OpenWeb,
    /// The subject owner's own site (sitemap, llms.txt).
    OwnerSites,
    /// One package registry.
    Registry(Ecosystem),
    /// A code host's repository metadata (GitHub).
    CodeHost,
    Scholarly,
    Standards,
    Law,
    Community,
    Encyclopedia,
    Archive,
    /// The links of a page already read; no network beyond that page.
    LinkedPages,
}

/// A service whose key lives in the keyring (`CredentialsPort`), named by the
/// keyring service id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KeyService {
    Brave,
    Tavily,
    Exa,
    GitHub,
    SemanticScholar,
    ApiDataGov,
    StackExchange,
}

impl KeyService {
    pub const ALL: [KeyService; 7] = [
        KeyService::Brave,
        KeyService::Tavily,
        KeyService::Exa,
        KeyService::GitHub,
        KeyService::SemanticScholar,
        KeyService::ApiDataGov,
        KeyService::StackExchange,
    ];

    /// The keyring service id (`CredentialsPort::*_api_key(service)`).
    pub fn keyring_id(self) -> &'static str {
        match self {
            KeyService::Brave => "brave",
            KeyService::Tavily => "tavily",
            KeyService::Exa => "exa",
            KeyService::GitHub => "github",
            KeyService::SemanticScholar => "semantic_scholar",
            KeyService::ApiDataGov => "api_data_gov",
            KeyService::StackExchange => "stack_exchange",
        }
    }
}

/// A setting that names a user-run instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstanceSetting {
    /// `SourcingSettingsDto::searxng_url`.
    SearxngUrl,
}

/// What a key costs the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeyPlan {
    /// A free key from a sign-up.
    Free,
    /// A free allowance of credits, then paid.
    Credits,
    Paid,
}

/// What a finder needs before it can run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Auth {
    None,
    /// Works without a key; the user's key raises its limits.
    OptionalKey {
        service: KeyService,
    },
    /// Runs only with the user's key in the keyring.
    RequiredKey {
        service: KeyService,
        plan: KeyPlan,
    },
    /// Runs only once the user names their own instance.
    UserInstance {
        setting: InstanceSetting,
    },
    /// A polite pool that runs only with the user's contact set.
    Contact,
}

/// Who sees the query, and whether it identifies the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Privacy {
    /// The service receives the query and nothing that identifies the user.
    AnonymousQuery,
    /// The service receives the query and may log it.
    QueryLogged { by: &'static str },
    /// The query travels under the user's key.
    IdentifiedByKey { by: &'static str },
    /// The query travels with the user's contact.
    IdentifiedByContact { by: &'static str },
    /// The query goes to the instance the user named; its operator may log it.
    SentToUserInstance,
}

/// What a call costs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Cost {
    Free,
    Credits { per_request: u32 },
    Metered,
}

/// A request allowance per period that the service documents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Quota {
    pub requests: u32,
    pub per: Duration,
}

/// How fast a finder may be called, from the service's own policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RatePolicy {
    /// The service's documented minimum spacing between calls.
    pub min_interval: Duration,
    /// The service's documented allowance, when it has one.
    pub quota: Option<Quota>,
    pub honours_retry_after: bool,
    /// Opens the breaker on the first refusal instead of the policy's streak,
    /// for hosts whose penalty for a second request is a long block (the
    /// Wayback CDX API blocks for an hour).
    pub trip_on_first_refusal: bool,
    /// The URL of the policy these numbers come from.
    pub source: &'static str,
}

/// The HTTP identity a finder presents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Identity {
    /// Browser headers, for HTML engines and ordinary pages.
    BrowserLike,
    /// `Lattice/<version> (+<project url>; <contact if set>)`, for APIs.
    DeclaredClient,
}

/// Where the query goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Destination {
    Host(&'static str),
    /// The instance the user named.
    UserInstance,
    /// The subject owner's site (site discovery); the owner sees the fetch.
    SubjectOwner,
    /// Nowhere beyond a page already read.
    Local,
}

/// When a finder runs, as derived from what it needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Activation {
    /// On unless the user turns it off.
    Always,
    /// Off until the user adds this key.
    WhenKeySet(KeyService),
    /// Off until the user names the instance.
    WhenInstanceSet(InstanceSetting),
    /// Off until the user sets a contact.
    WhenContactSet,
}

/// Everything the router and the settings panel know about a finder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinderDescriptor {
    pub id: FinderId,
    /// The name the user reads (`DuckDuckGo`).
    pub name: &'static str,
    pub finds: &'static [Role],
    pub reaches: Reach,
    pub auth: Auth,
    pub privacy: Privacy,
    pub cost: Cost,
    pub rate: RatePolicy,
    /// Has a native `site:` or domain filter.
    pub supports_scope: bool,
    pub identity: Identity,
    pub sends_to: Destination,
}

impl FinderDescriptor {
    /// The wave this finder joins for `kind`, if it serves that kind.
    pub fn slot_for(&self, kind: NeedKind) -> Option<WaveSlot> {
        self.finds
            .iter()
            .find(|role| role.kind == kind)
            .map(|role| role.wave)
    }

    /// When the finder runs. Anonymous, keyless finders are on by default;
    /// anything that needs a key, an instance or a contact is off until the
    /// user configures it (the user can always turn a finder off).
    pub fn activation(&self) -> Activation {
        match self.auth {
            Auth::None | Auth::OptionalKey { .. } => Activation::Always,
            Auth::RequiredKey { service, .. } => Activation::WhenKeySet(service),
            Auth::UserInstance { setting } => Activation::WhenInstanceSet(setting),
            Auth::Contact => Activation::WhenContactSet,
        }
    }

    /// The one-line disclosure the settings panel shows, rendered from the
    /// descriptor rather than hand-written.
    pub fn disclosure(&self) -> String {
        let base = match self.privacy {
            Privacy::AnonymousQuery => {
                format!(
                    "{} receives your query and nothing that identifies you",
                    self.name
                )
            }
            Privacy::QueryLogged { by } => format!("Your query is sent to {by}, which may log it"),
            Privacy::IdentifiedByKey { by } => format!("Your query is sent to {by} under your key"),
            Privacy::IdentifiedByContact { by } => {
                format!("Your query and your contact are sent to {by}")
            }
            Privacy::SentToUserInstance => {
                "Your query is sent to the instance you set; its operator may log it".to_string()
            }
        };
        match self.auth {
            Auth::OptionalKey { .. } => format!("{base}; under your key when you add one"),
            _ => base,
        }
    }
}
