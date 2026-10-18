//! What the sourcing caches key on and when an entry may be reused (§3.7).
//! Pure: the stores live in the desktop crate (memory for provider pages,
//! SQLite for owners and archive lookups).

use std::time::Duration;

use chrono::{DateTime, NaiveDate, Utc};
use url::Url;

use crate::domain::sourcing::canonical;
use crate::domain::sourcing::{
    FinderId, Freshness, Origin, SourceNeed, SourcingPolicy, SubjectHint,
};

/// The sourcing caches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourcingCache {
    /// One finder's page of results for one need.
    ProviderPage,
    /// A subject's owner origins.
    OwnerResolution,
    /// Whether, and when, the archive holds a URL.
    ArchiveLookup,
}

/// How long an entry of `cache` stays fresh.
pub fn ttl(cache: SourcingCache, policy: &SourcingPolicy) -> Duration {
    match cache {
        SourcingCache::ProviderPage => policy.search_page_ttl,
        SourcingCache::OwnerResolution => policy.owner_ttl,
        SourcingCache::ArchiveLookup => policy.archive_lookup_ttl,
    }
}

/// True while an entry stored at `stored_at` is younger than `ttl`. An entry
/// stamped in the future (the clock moved back) is treated as stale.
pub fn is_fresh(stored_at: DateTime<Utc>, now: DateTime<Utc>, ttl: Duration) -> bool {
    (now - stored_at).to_std().is_ok_and(|age| age < ttl)
}

/// Whether a cached provider page may answer a need: fresh, and, for
/// `After(date)`, stored on or after that date (an older page cannot hold
/// what was published since).
pub fn provider_page_usable(
    stored_at: DateTime<Utc>,
    now: DateTime<Utc>,
    freshness: Freshness,
    policy: &SourcingPolicy,
) -> bool {
    let fresh = is_fresh(stored_at, now, ttl(SourcingCache::ProviderPage, policy));
    match freshness {
        Freshness::After(date) => fresh && stored_at >= start_of(date),
        Freshness::Any | Freshness::AsOf(_) => fresh,
    }
}

/// The provider-page key: the finder, the need as the step ran it (its
/// effective query and scope), its freshness and the page.
pub fn provider_page_key(
    finder: &FinderId,
    need: &SourceNeed,
    query: &str,
    scope: Option<&Origin>,
    page: usize,
) -> String {
    let scope = scope.map(Origin::to_string).unwrap_or_default();
    let freshness = match need.freshness {
        Freshness::Any => String::new(),
        Freshness::After(date) => format!("after:{date}"),
        Freshness::AsOf(date) => format!("as_of:{date}"),
    };
    format!(
        "{finder}|{}|{}|{scope}|{freshness}|{page}",
        need.kind.as_str(),
        normalise_terms(query),
    )
}

/// The owner-cache key: the subject's name and kind.
pub fn owner_key(subject: &SubjectHint) -> String {
    format!(
        "{}|{}",
        subject.kind.as_str(),
        normalise_terms(&subject.name)
    )
}

/// The archive-lookup key: the URL's canonical key and the day asked for
/// (none for "the latest copy").
pub fn archive_lookup_key(url: &Url, as_of: Option<NaiveDate>) -> String {
    let day = as_of.map(|date| date.to_string()).unwrap_or_default();
    format!("{}|{day}", canonical::key(url))
}

/// Lowercased, with runs of whitespace collapsed to one space.
fn normalise_terms(terms: &str) -> String {
    terms
        .split_whitespace()
        .map(str::to_lowercase)
        .collect::<Vec<_>>()
        .join(" ")
}

fn start_of(date: NaiveDate) -> DateTime<Utc> {
    date.and_time(chrono::NaiveTime::MIN).and_utc()
}

#[cfg(test)]
#[path = "cache_policy_tests.rs"]
mod tests;
