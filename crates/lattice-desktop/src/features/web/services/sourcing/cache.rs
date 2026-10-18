//! The provider-page cache (design §3.7): one finder's page of leads for one
//! need, kept for `search_page_ttl`. Paging a search reads the pool from
//! here, so page 2 never re-fetches any provider's page 1.
//!
//! In memory on purpose: rankings go stale far faster than the pages behind
//! them, and nobody should see yesterday's ranking because the app restarted.
//! Bounded by the TTL rather than a count: an entry lives a quarter hour and
//! holds one page of titles and snippets.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use lattice_core::application::services::sourcing::cache_policy;
use lattice_core::domain::sourcing::{Freshness, Lead, SourcingPolicy};

pub(super) struct ProviderPageCache {
    policy: SourcingPolicy,
    entries: parking_lot::Mutex<HashMap<String, CachedPage>>,
}

struct CachedPage {
    stored_at: DateTime<Utc>,
    leads: Vec<Lead>,
}

impl ProviderPageCache {
    pub(super) fn new(policy: SourcingPolicy) -> Self {
        Self {
            policy,
            entries: parking_lot::Mutex::new(HashMap::new()),
        }
    }

    /// The page stored under `key`, when it may answer a need of this
    /// freshness now.
    pub(super) fn get(
        &self,
        key: &str,
        freshness: Freshness,
        now: DateTime<Utc>,
    ) -> Option<Vec<Lead>> {
        let entries = self.entries.lock();
        let page = entries.get(key)?;
        cache_policy::provider_page_usable(page.stored_at, now, freshness, &self.policy)
            .then(|| page.leads.clone())
    }

    /// Keep a page that answered. Only answered pages are kept: an empty or
    /// failed page cached for a quarter hour would make a provider's bad
    /// minute last fifteen.
    pub(super) fn store(&self, key: String, leads: Vec<Lead>, now: DateTime<Utc>) {
        let ttl = cache_policy::ttl(cache_policy::SourcingCache::ProviderPage, &self.policy);
        let mut entries = self.entries.lock();
        entries.retain(|_, page| cache_policy::is_fresh(page.stored_at, now, ttl));
        entries.insert(
            key,
            CachedPage {
                stored_at: now,
                leads,
            },
        );
    }
}
