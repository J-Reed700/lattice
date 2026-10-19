//! Source-finding settings: which finders run, the user's own SearXNG
//! instance, the optional polite-pool contact, the archive fallback and the
//! wave concurrency. Keys never live here; they are in the keyring under the
//! `KeyService` ids.
//!
//! A finder runs when the user has not turned it off *and* what its `Auth`
//! needs is configured: keyless finders are on by default; the keyed engines
//! (Brave, Tavily, Exa: `Auth::RequiredKey`) are off until their key is in the
//! keyring; SearXNG (`Auth::UserInstance`) is off until `searxng_url` is set;
//! polite pools (`Auth::Contact`) are off until `contact` is set.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use url::Url;

use crate::domain::sourcing::policy::DEFAULT_FINDER_CONCURRENCY;
use crate::domain::sourcing::{
    is_known_finder, Auth, FinderDescriptor, FinderId, InstanceSetting, KeyService, Reach,
    SkipReason,
};

/// Source-finding settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SourcingSettingsDto {
    /// Per-finder switches by finder id; a finder left out is on (its `Auth`
    /// still gates it).
    #[serde(default)]
    pub finders: BTreeMap<String, FinderSettingDto>,

    /// The user's SearXNG instance (its JSON API must be enabled). Unset
    /// keeps the SearXNG finder off.
    #[serde(default)]
    pub searxng_url: Option<String>,

    /// An email or URL sent to polite pools (Crossref, Unpaywall). Off by
    /// default and never a project address: one shared address would make
    /// every user look like one client and rate-limit them together.
    #[serde(default)]
    pub contact: Option<String>,

    /// Read a page from the Internet Archive when it will not load, and
    /// allow dated archive reads. Sends the failed URL to archive.org.
    #[serde(default = "default_archive_fallback")]
    pub archive_fallback: bool,

    /// The most finders one wave runs at once.
    #[serde(default = "default_finder_concurrency")]
    pub finder_concurrency: u32,
}

/// One finder's switch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct FinderSettingDto {
    pub enabled: bool,
}

fn default_archive_fallback() -> bool {
    true
}

fn default_finder_concurrency() -> u32 {
    u32::try_from(DEFAULT_FINDER_CONCURRENCY).unwrap_or(u32::MAX)
}

impl Default for SourcingSettingsDto {
    fn default() -> Self {
        Self {
            finders: BTreeMap::new(),
            searxng_url: None,
            contact: None,
            archive_fallback: default_archive_fallback(),
            finder_concurrency: default_finder_concurrency(),
        }
    }
}

impl SourcingSettingsDto {
    /// False only when the user turned this finder off.
    pub fn finder_enabled(&self, id: &FinderId) -> bool {
        self.finders
            .get(id.as_str())
            .is_none_or(|setting| setting.enabled)
    }

    /// The SearXNG instance, when set to an http(s) URL.
    pub fn searxng_url(&self) -> Option<Url> {
        self.searxng_url
            .as_deref()
            .map(str::trim)
            .filter(|raw| !raw.is_empty())
            .and_then(|raw| Url::parse(raw).ok())
            .filter(|url| matches!(url.scheme(), "http" | "https") && url.host_str().is_some())
    }

    /// The polite-pool contact, when set.
    pub fn contact(&self) -> Option<&str> {
        self.contact
            .as_deref()
            .map(str::trim)
            .filter(|contact| !contact.is_empty())
    }

    /// Whether the user named the instance `setting` refers to.
    pub fn instance_set(&self, setting: InstanceSetting) -> bool {
        match setting {
            InstanceSetting::SearxngUrl => self.searxng_url().is_some(),
        }
    }

    /// Problems with these settings, each a sentence for the settings check:
    /// a SearXNG address is an http(s) URL, a contact is an email address or
    /// an http(s) URL, every switch names a registered finder, and a wave
    /// runs at least one finder. Clearing a field is `None`, never blank.
    pub fn problems(&self) -> Vec<String> {
        let mut problems = Vec::new();
        if self.searxng_url.is_some() && self.searxng_url().is_none() {
            problems.push("searxngUrl must be an http:// or https:// address".to_string());
        }
        if self
            .contact
            .as_deref()
            .is_some_and(|contact| !is_contact(contact.trim()))
        {
            problems.push(
                "contact must be an email address or an http:// or https:// address".to_string(),
            );
        }
        for id in self.finders.keys().filter(|id| !is_known_finder(id)) {
            problems.push(format!("finders names a finder that does not exist: {id}"));
        }
        if self.finder_concurrency == 0 {
            problems.push("finderConcurrency must be at least 1".to_string());
        }
        problems
    }

    /// Finders per wave, at least one.
    pub fn finder_concurrency(&self) -> usize {
        usize::try_from(self.finder_concurrency)
            .unwrap_or(DEFAULT_FINDER_CONCURRENCY)
            .max(1)
    }

    /// Why this finder may not run under these settings, or `None` when it
    /// may. `keys` holds the services with a key in the keyring. Turned off
    /// beats unconfigured, so the panel says what the user did.
    pub fn gate(
        &self,
        descriptor: &FinderDescriptor,
        keys: &BTreeSet<KeyService>,
    ) -> Option<SkipReason> {
        if !self.finder_enabled(&descriptor.id)
            || (descriptor.reaches == Reach::Archive && !self.archive_fallback)
        {
            return Some(SkipReason::Disabled);
        }
        match descriptor.auth {
            Auth::None | Auth::OptionalKey { .. } => None,
            Auth::RequiredKey { service, .. } => {
                (!keys.contains(&service)).then_some(SkipReason::NoKey { service })
            }
            Auth::UserInstance { setting } => {
                (!self.instance_set(setting)).then_some(SkipReason::NoInstance { setting })
            }
            Auth::Contact => self.contact().is_none().then_some(SkipReason::NoContact),
        }
    }
}

/// An email address (one `@`, something before it, a dotted domain after
/// it, no spaces) or an http(s) URL with a host. The services that pool by
/// contact accept either; the check is shape only.
fn is_contact(contact: &str) -> bool {
    if contact.is_empty() || contact.chars().any(char::is_whitespace) {
        return false;
    }
    if let Some((local, domain)) = contact.split_once('@') {
        return !local.is_empty()
            && !domain.contains('@')
            && domain.contains('.')
            && domain.split('.').all(|label| !label.is_empty());
    }
    Url::parse(contact)
        .is_ok_and(|url| matches!(url.scheme(), "http" | "https") && url.host_str().is_some())
}

#[cfg(test)]
#[path = "sourcing_tests.rs"]
mod tests;
