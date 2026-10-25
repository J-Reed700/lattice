//! Where a scoped site lives now (core `SiteHome`, toolbelt design §6.7).
//!
//! A search that keeps to a site is run where the site answers from today,
//! not under the name the caller remembered. The site's address is read once
//! a day ([`SourcingPolicy::site_home_ttl`]) and every search shares the
//! answer; the redirects it answers with decide the home by the core's rule.
//! A read that fails says nothing about where the site lives: the scope is
//! searched as asked, and the site is read again after
//! [`SourcingPolicy::site_home_retry`]. A search stopped or out of time
//! before the answer came is the caller's, not the site's, and is not
//! remembered.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use lattice_core::application::ports::Budget;
use lattice_core::application::services::sourcing::cache_policy::{self, SourcingCache};
use lattice_core::domain::sourcing::{Hop, Origin, SiteHome, SourcingPolicy};
use url::Url;

use super::runner::{Clock, SystemClock};

/// Reads the redirects an address answers with.
#[async_trait]
pub(crate) trait HopReader: Send + Sync {
    /// The redirects `from` answers with, in order, up to the first address
    /// that answers with anything else. `Err` when the walk could not reach
    /// such an answer: a request failed, or an address answered with a server
    /// error or asked to be left alone for now (429).
    async fn hops(&self, from: &Url) -> Result<Vec<Hop>, String>;
}

/// The homes of the sites searches keep to, read through `reader` and kept
/// for the policy's site-home TTL.
pub(crate) struct SiteHomes {
    reader: Arc<dyn HopReader>,
    policy: SourcingPolicy,
    clock: Arc<dyn Clock>,
    known: parking_lot::Mutex<HashMap<Origin, Known>>,
}

struct Known {
    read_at: DateTime<Utc>,
    keep_for: Duration,
    site: SiteHome,
}

impl SiteHomes {
    pub(crate) fn new(
        reader: Arc<dyn HopReader>,
        policy: SourcingPolicy,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            reader,
            policy,
            clock,
            known: parking_lot::Mutex::new(HashMap::new()),
        }
    }

    pub(crate) fn with_system_clock(reader: Arc<dyn HopReader>, policy: SourcingPolicy) -> Self {
        Self::new(reader, policy, Arc::new(SystemClock))
    }

    /// Where `asked` answers from now. Its address is requested only when
    /// nothing fresh is known and `budget` has room; a search stopped or out
    /// of time keeps the scope as asked.
    pub(crate) async fn of(&self, asked: Origin, budget: &Budget) -> SiteHome {
        if let Some(known) = self.known.lock().get(&asked) {
            if cache_policy::is_fresh(known.read_at, self.clock.now(), known.keep_for) {
                return known.site.clone();
            }
        }
        if budget.is_spent() {
            return SiteHome::unmoved(asked);
        }
        let Ok(address) = Url::parse(&format!("https://{asked}")) else {
            return SiteHome::unmoved(asked);
        };
        let read = tokio::select! {
            read = tokio::time::timeout(budget.remaining(), self.reader.hops(&address)) => {
                match read {
                    Ok(read) => read,
                    Err(_) => return SiteHome::unmoved(asked),
                }
            }
            () = budget.stop.cancelled() => return SiteHome::unmoved(asked),
        };
        let (site, keep_for) = match read {
            Ok(hops) => (
                SiteHome::after(asked.clone(), &hops),
                cache_policy::ttl(SourcingCache::SiteHome, &self.policy),
            ),
            Err(cause) => {
                tracing::info!(
                    site = %asked,
                    %cause,
                    "Could not read where a scoped site lives; searching it as asked"
                );
                (
                    SiteHome::unmoved(asked.clone()),
                    self.policy.site_home_retry,
                )
            }
        };
        if site.moved() {
            tracing::info!(
                asked = %site.asked(),
                home = %site.home(),
                "A scoped site has moved permanently; searching where it lives now"
            );
        }
        let now = self.clock.now();
        let mut known = self.known.lock();
        known.retain(|_, known| cache_policy::is_fresh(known.read_at, now, known.keep_for));
        known.insert(
            asked,
            Known {
                read_at: now,
                keep_for,
                site: site.clone(),
            },
        );
        site
    }
}

#[cfg(test)]
#[path = "home_tests.rs"]
mod tests;
