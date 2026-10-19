//! Settings → Sources (design §3.14, §3.15, §3.17): every finder in the
//! catalogue with what it needs, where its query goes, the user's switch, why
//! it cannot run and how it is doing, plus whether each key service has a key
//! stored. Never a key itself: the panel learns only that one is stored.

use std::collections::BTreeSet;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::SourcingSettingsDto;
use crate::domain::sourcing::{
    Auth, FinderDescriptor, FinderHealth, HealthState, KeyService, Reach, SkipReason,
};

/// A service whose key lives in the keyring, as the panel and the key
/// commands name it.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, specta::Type,
)]
#[serde(rename_all = "camelCase")]
pub enum KeyServiceDto {
    Brave,
    Tavily,
    Exa,
    #[serde(rename = "github")]
    GitHub,
    SemanticScholar,
    ApiDataGov,
    StackExchange,
}

impl From<KeyService> for KeyServiceDto {
    fn from(service: KeyService) -> Self {
        match service {
            KeyService::Brave => Self::Brave,
            KeyService::Tavily => Self::Tavily,
            KeyService::Exa => Self::Exa,
            KeyService::GitHub => Self::GitHub,
            KeyService::SemanticScholar => Self::SemanticScholar,
            KeyService::ApiDataGov => Self::ApiDataGov,
            KeyService::StackExchange => Self::StackExchange,
        }
    }
}

impl From<KeyServiceDto> for KeyService {
    fn from(service: KeyServiceDto) -> Self {
        match service {
            KeyServiceDto::Brave => Self::Brave,
            KeyServiceDto::Tavily => Self::Tavily,
            KeyServiceDto::Exa => Self::Exa,
            KeyServiceDto::GitHub => Self::GitHub,
            KeyServiceDto::SemanticScholar => Self::SemanticScholar,
            KeyServiceDto::ApiDataGov => Self::ApiDataGov,
            KeyServiceDto::StackExchange => Self::StackExchange,
        }
    }
}

/// What part of the world a finder reaches; the panel groups finders by it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum ReachGroupDto {
    OpenWeb,
    OwnerSites,
    Registry,
    CodeHost,
    Scholarly,
    Standards,
    Law,
    Community,
    Encyclopedia,
    Archive,
    LinkedPages,
}

impl From<Reach> for ReachGroupDto {
    fn from(reach: Reach) -> Self {
        match reach {
            Reach::OpenWeb => Self::OpenWeb,
            Reach::OwnerSites => Self::OwnerSites,
            Reach::Registry(_) => Self::Registry,
            Reach::CodeHost => Self::CodeHost,
            Reach::Scholarly => Self::Scholarly,
            Reach::Standards => Self::Standards,
            Reach::Law => Self::Law,
            Reach::Community => Self::Community,
            Reach::Encyclopedia => Self::Encyclopedia,
            Reach::Archive => Self::Archive,
            Reach::LinkedPages => Self::LinkedPages,
        }
    }
}

/// What a finder needs before it can run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum FinderNeedDto {
    Nothing,
    /// Runs without a key; the user's key raises its limits.
    OptionalKey {
        service: KeyServiceDto,
    },
    /// Runs only with the user's key.
    RequiredKey {
        service: KeyServiceDto,
    },
    /// Runs only once the user names their own instance.
    InstanceUrl,
    /// Runs only once the user sets a contact.
    Contact,
}

impl From<Auth> for FinderNeedDto {
    fn from(auth: Auth) -> Self {
        match auth {
            Auth::None => Self::Nothing,
            Auth::OptionalKey { service } => Self::OptionalKey {
                service: service.into(),
            },
            Auth::RequiredKey { service, .. } => Self::RequiredKey {
                service: service.into(),
            },
            Auth::UserInstance { .. } => Self::InstanceUrl,
            Auth::Contact => Self::Contact,
        }
    }
}

/// Why a finder cannot run under the user's settings and keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum FinderGateDto {
    /// The user turned it off (or, for the archive, the archive fallback).
    Disabled,
    NoKey,
    NoInstance,
    NoContact,
}

impl FinderGateDto {
    /// The settings gate's reason. The other skip reasons are a call's
    /// (an open breaker, a spent allowance), which the health reports.
    fn from_gate(reason: &SkipReason) -> Option<Self> {
        match reason {
            SkipReason::Disabled => Some(Self::Disabled),
            SkipReason::NoKey { .. } => Some(Self::NoKey),
            SkipReason::NoInstance { .. } => Some(Self::NoInstance),
            SkipReason::NoContact => Some(Self::NoContact),
            SkipReason::CircuitOpen { .. }
            | SkipReason::QuotaSpent { .. }
            | SkipReason::NotApplicable
            | SkipReason::OutOfBudget => None,
        }
    }
}

/// A finder's breaker as the panel reads it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum FinderHealthStateDto {
    Healthy,
    /// Refused or failing; not called again until `until`.
    CoolingDown,
    /// Its pause is over; the next call decides whether it is healthy.
    Retrying,
}

/// A finder's health since the app started.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct FinderHealthDto {
    pub state: FinderHealthStateDto,
    /// When a cooling-down finder may be called again.
    pub until: Option<DateTime<Utc>>,
    /// The latest failure, as the user reads it.
    pub last_error: Option<String>,
    pub calls: u64,
    pub answered: u64,
    pub empty: u64,
    pub failed: u64,
    pub skipped: u64,
    pub median_ms: Option<u64>,
}

impl From<&FinderHealth> for FinderHealthDto {
    fn from(health: &FinderHealth) -> Self {
        let (state, until, open_cause) = match &health.state {
            HealthState::Open { until, cause } => (
                FinderHealthStateDto::CoolingDown,
                Some(*until),
                Some(cause.to_string()),
            ),
            HealthState::HalfOpen => (FinderHealthStateDto::Retrying, None, None),
            // A gated finder's breaker is reported apart from its gate.
            HealthState::Healthy | HealthState::Unavailable(_) => {
                (FinderHealthStateDto::Healthy, None, None)
            }
        };
        Self {
            state,
            until,
            last_error: health
                .last_cause
                .as_ref()
                .map(ToString::to_string)
                .or(open_cause),
            calls: health.calls,
            answered: health.answered,
            empty: health.empty,
            failed: health.failed,
            skipped: health.skipped,
            median_ms: health.median_ms,
        }
    }
}

/// One finder in the panel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct FinderStatusDto {
    pub id: String,
    /// The name the user reads (`DuckDuckGo`).
    pub name: String,
    pub reach: ReachGroupDto,
    pub needs: FinderNeedDto,
    /// Where the query goes, rendered from the descriptor.
    pub disclosure: String,
    /// The user's switch: on unless they turned it off.
    pub enabled: bool,
    /// Why it cannot run, or `None` when it may.
    pub gate: Option<FinderGateDto>,
    pub health: FinderHealthDto,
}

/// Whether a key service has a key stored, and where to get one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct KeyStatusDto {
    pub service: KeyServiceDto,
    pub name: String,
    /// The provider's own page for getting a key.
    pub key_page_url: String,
    pub stored: bool,
}

/// Everything Settings → Sources shows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SourcesStatusDto {
    pub finders: Vec<FinderStatusDto>,
    pub keys: Vec<KeyStatusDto>,
}

impl SourcesStatusDto {
    /// The panel's view of `finders`, each descriptor with its breaker
    /// health, under `settings`; `keys` holds the services with a key
    /// stored.
    pub fn new(
        finders: &[(&FinderDescriptor, &FinderHealth)],
        settings: &SourcingSettingsDto,
        keys: &BTreeSet<KeyService>,
    ) -> Self {
        Self {
            finders: finders
                .iter()
                .map(|(descriptor, health)| FinderStatusDto {
                    id: descriptor.id.to_string(),
                    name: descriptor.name.to_string(),
                    reach: descriptor.reaches.into(),
                    needs: descriptor.auth.into(),
                    disclosure: descriptor.disclosure(),
                    enabled: settings.finder_enabled(&descriptor.id),
                    gate: settings
                        .gate(descriptor, keys)
                        .as_ref()
                        .and_then(FinderGateDto::from_gate),
                    health: FinderHealthDto::from(*health),
                })
                .collect(),
            keys: KeyService::ALL
                .iter()
                .map(|service| KeyStatusDto {
                    service: (*service).into(),
                    name: service.label().to_string(),
                    key_page_url: service.key_page_url().to_string(),
                    stored: keys.contains(service),
                })
                .collect(),
        }
    }
}
