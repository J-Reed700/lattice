//! Per-site request pacing and the short-lived search cache.
use super::{
    WebSearchInput, WebSearchOutput, MAX_CACHED_SEARCHES, MAX_TRACKED_SITES,
    MIN_SITE_REQUEST_INTERVAL, SEARCH_CACHE_TTL, SITE_ALIASES, SITE_SLOT_WAIT_TIMEOUT,
};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{Mutex as AsyncMutex, OwnedMutexGuard};
use tokio::time::sleep;
use tracing::warn;
use url::Url;

/// Paces requests per site.
///
/// Guarantees two things per site: one request in flight at a time, and at
/// least [`MIN_SITE_REQUEST_INTERVAL`] between the starts of consecutive
/// requests. A slot covers a single request, never a retry sequence or a
/// FlareSolverr round trip, so a slow site delays its own next request instead
/// of every request queued behind it. Waiting is capped by
/// [`SITE_SLOT_WAIT_TIMEOUT`]; past that the request is sent unpaced. Different
/// sites never wait on each other.
pub(super) struct SitePacer {
    /// Per-site slot, holding when that site's last request started.
    sites: parking_lot::Mutex<HashMap<String, Arc<AsyncMutex<Option<Instant>>>>>,
    min_interval: Duration,
}

impl Default for SitePacer {
    fn default() -> Self {
        Self {
            sites: parking_lot::Mutex::new(HashMap::new()),
            min_interval: MIN_SITE_REQUEST_INTERVAL,
        }
    }
}

impl SitePacer {
    #[cfg(test)]
    pub(super) fn with_interval(min_interval: Duration) -> Self {
        Self {
            min_interval,
            ..Self::default()
        }
    }

    /// Queue key for `url`: its full host, with the mirror hosts above folded
    /// onto the site they mirror. Keying on a public suffix instead would put
    /// unrelated sites in one queue. `None` when there is no host to pace,
    /// which leaves the request unpaced rather than inventing a per-URL queue.
    pub(super) fn site_key(url: &str) -> Option<String> {
        let host = Url::parse(url).ok()?.host_str()?.to_ascii_lowercase();
        let host = host.trim_start_matches("www.");
        Some(
            SITE_ALIASES
                .iter()
                .find(|(mirror, _)| *mirror == host)
                .map_or_else(|| host.to_string(), |(_, site)| (*site).to_string()),
        )
    }

    /// Take `url`'s site slot, then wait out whatever is left of that site's
    /// minimum interval. Dropping the guard releases the slot; `None` means the
    /// request is not paced at all — it has no host, or the wait timed out.
    pub(super) async fn acquire(&self, url: &str) -> Option<OwnedMutexGuard<Option<Instant>>> {
        let key = Self::site_key(url)?;
        let min_interval = self.min_interval;
        let slot = {
            let mut sites = self.sites.lock();
            if sites.len() >= MAX_TRACKED_SITES {
                // Drop only entries nobody holds whose interval has already
                // elapsed: their next request has nothing left to wait for.
                sites.retain(|_, slot| {
                    Arc::strong_count(slot) > 1
                        || slot.try_lock().map_or(true, |last| {
                            last.is_some_and(|started| started.elapsed() < min_interval)
                        })
                });
            }
            Arc::clone(sites.entry(key).or_default())
        };

        let mut last_request =
            match tokio::time::timeout(SITE_SLOT_WAIT_TIMEOUT, slot.lock_owned()).await {
                Ok(guard) => guard,
                Err(_) => {
                    warn!(url, "Site slot wait timed out; sending the request unpaced");
                    return None;
                }
            };
        if let Some(remaining) =
            last_request.and_then(|started| min_interval.checked_sub(started.elapsed()))
        {
            sleep(remaining).await;
        }
        *last_request = Some(Instant::now());
        Some(last_request)
    }
}

/// The last few searches, so the same one is not run twice in a quarter hour.
///
/// One deep-research search is a dozen or more requests to DuckDuckGo and
/// Bing, and a regenerate re-issues every one of them for a query whose
/// results have not moved. That is how the providers start answering 429. In
/// memory rather than on disk on purpose: search results go stale much faster
/// than the pages behind them, and nobody should be shown yesterday's ranking
/// because the app was restarted.
#[derive(Default)]
pub(super) struct SearchCache {
    entries: parking_lot::Mutex<HashMap<String, (Instant, WebSearchOutput)>>,
}

impl SearchCache {
    /// Everything that changes what comes back. A search that differs in any
    /// of these is a different search.
    fn key(input: &WebSearchInput) -> String {
        let mut providers = input.providers.clone();
        providers.sort();
        format!(
            "{}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}",
            input.query.trim().to_lowercase(),
            input.max_results,
            input.page,
            input.offset,
            providers.join(","),
            input.include_wikipedia,
            input.depth,
            input.branch_queries,
            input.followup_queries.join("\u{1e}")
        )
    }

    pub(super) fn get(&self, input: &WebSearchInput) -> Option<WebSearchOutput> {
        let mut entries = self.entries.lock();
        let key = Self::key(input);
        let (stored_at, output) = entries.get(&key)?;
        if stored_at.elapsed() > SEARCH_CACHE_TTL {
            entries.remove(&key);
            return None;
        }
        Some(output.clone())
    }

    /// Keep `output` under `input`. Only ever called with a search that
    /// actually found something: caching "nothing found" would make a
    /// provider's bad minute last fifteen.
    pub(super) fn store(&self, input: &WebSearchInput, output: &WebSearchOutput) {
        let mut entries = self.entries.lock();
        if entries.len() >= MAX_CACHED_SEARCHES {
            entries.retain(|_, (stored_at, _)| stored_at.elapsed() <= SEARCH_CACHE_TTL);
        }
        if entries.len() >= MAX_CACHED_SEARCHES {
            let oldest = entries
                .iter()
                .min_by_key(|(_, (stored_at, _))| *stored_at)
                .map(|(key, _)| key.clone());
            if let Some(oldest) = oldest {
                entries.remove(&oldest);
            }
        }
        entries.insert(Self::key(input), (Instant::now(), output.clone()));
    }
}
