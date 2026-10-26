//! `SourcingPolicy`: the named parameters of source finding, each with the
//! reason for its value. These are the only numbers the sourcing code uses;
//! every other bound comes from the caller's budget, the host's own
//! `Retry-After`, or a finder's documented rate policy.

use std::time::Duration;

/// Cormack, Clarke and Büttcher (SIGIR 2009) fuse with k = 60 and found the
/// result insensitive to k across their sweep.
pub const DEFAULT_RRF_K: u32 = 60;

/// The most finders one wave runs at once. Hosts are paced serially by the
/// site pacer anyway; this ceiling protects the user's machine and the shared
/// LLM budget, the same reasoning as the memory cap on build agents.
pub const DEFAULT_FINDER_CONCURRENCY: usize = 4;

/// One 403/429 or challenge may be the pacer's fault; two in a row, with
/// pacing, means the host means it.
pub const BREAKER_REFUSALS_TO_TRIP: u32 = 2;

/// Transport failures are noisier than refusals (a dropped connection, a
/// slow DNS answer), so the breaker waits for a third in a row.
pub const BREAKER_TRANSPORT_FAILURES_TO_TRIP: u32 = 3;

/// The longest a breaker stays open on its own backoff, without a host
/// instruction. The service allowances the catalogue documents reset at most
/// daily, so a self-derived pause past one day has no basis; a host's own
/// `Retry-After` is honoured in full and is not held to this.
pub const BREAKER_CEILING: Duration = Duration::from_secs(24 * 60 * 60);

/// Search engines re-rank within minutes, and a lesson's research passes
/// repeat inside this window (today's `SEARCH_CACHE_TTL`).
pub const SEARCH_PAGE_TTL: Duration = Duration::from_secs(15 * 60);

/// Subjects change owners on acquisition timescales; an entry is refreshed on
/// first use after expiry.
pub const OWNER_TTL: Duration = Duration::from_secs(30 * 24 * 60 * 60);

/// Wayback captures accrue slowly: a miss today is a miss this week.
pub const ARCHIVE_LOOKUP_TTL: Duration = Duration::from_secs(7 * 24 * 60 * 60);

/// Sites move on rebrand and re-platform timescales, and a move is one
/// redirect read to learn again: a site's home is re-read after a day, so a
/// site that moves this week is searched where it lives by tomorrow.
pub const SITE_HOME_TTL: Duration = Duration::from_secs(24 * 60 * 60);

/// A site that could not be read (down, hanging, refusing) is searched as
/// asked, and the results found that way are reused for [`SEARCH_PAGE_TTL`]
/// anyway: reading the site again sooner would rarely change what a search
/// returns, and a hanging site costs one wait per window, not one per search.
pub const SITE_HOME_RETRY: Duration = SEARCH_PAGE_TTL;

/// When a breaker opens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BreakerTrip {
    /// Consecutive refusals (403, 429, challenge) that open it.
    pub refusals: u32,
    /// Consecutive transport failures (including 5xx) that open it.
    pub transport_failures: u32,
}

/// The parameters of routing, fusion, health and caching.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourcingPolicy {
    pub rrf_k: u32,
    pub finder_concurrency: usize,
    pub breaker_trip: BreakerTrip,
    pub breaker_ceiling: Duration,
    pub search_page_ttl: Duration,
    pub owner_ttl: Duration,
    pub archive_lookup_ttl: Duration,
    pub site_home_ttl: Duration,
    pub site_home_retry: Duration,
}

impl Default for SourcingPolicy {
    fn default() -> Self {
        Self {
            rrf_k: DEFAULT_RRF_K,
            finder_concurrency: DEFAULT_FINDER_CONCURRENCY,
            breaker_trip: BreakerTrip {
                refusals: BREAKER_REFUSALS_TO_TRIP,
                transport_failures: BREAKER_TRANSPORT_FAILURES_TO_TRIP,
            },
            breaker_ceiling: BREAKER_CEILING,
            search_page_ttl: SEARCH_PAGE_TTL,
            owner_ttl: OWNER_TTL,
            archive_lookup_ttl: ARCHIVE_LOOKUP_TTL,
            site_home_ttl: SITE_HOME_TTL,
            site_home_retry: SITE_HOME_RETRY,
        }
    }
}

impl SourcingPolicy {
    /// The policy with the user's concurrency setting (at least one).
    pub fn with_finder_concurrency(self, finder_concurrency: usize) -> Self {
        Self {
            finder_concurrency: finder_concurrency.max(1),
            ..self
        }
    }

    /// How many of a wave's finders run at once: one per distinct
    /// destination host, at most `finder_concurrency`, at least one.
    pub fn wave_concurrency(&self, distinct_destinations: usize) -> usize {
        distinct_destinations.min(self.finder_concurrency).max(1)
    }

    /// A wave is thin when it added fewer new fused leads than half of what
    /// the caller asked for: the point where another wave beats returning.
    pub fn is_thin(&self, new_leads: usize, want: usize) -> bool {
        new_leads.saturating_mul(2) < want
    }

    /// Whether to run the next wave: the caller still wants leads and the
    /// last wave was thin. (The budget check is the runner's.)
    pub fn wants_another_wave(&self, fused_leads: usize, new_leads: usize, want: usize) -> bool {
        fused_leads < want && self.is_thin(new_leads, want)
    }
}
