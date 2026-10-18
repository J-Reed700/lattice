//! Per-site request pacing, shared by page reads and every finder.
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{Mutex as AsyncMutex, OwnedMutexGuard};
use tokio::time::sleep;
use tracing::warn;
use url::Url;

/// Least time between the starts of two requests to the same site.
pub(crate) const MIN_SITE_REQUEST_INTERVAL: Duration = Duration::from_millis(900);
/// Longest wait for a site's slot. A wedged holder must not queue the rest of
/// the search behind it, so past this the request goes out unpaced.
const SITE_SLOT_WAIT_TIMEOUT: Duration = Duration::from_secs(15);
/// Sites tracked at once before idle entries are dropped. `WebService` is a
/// container singleton, so the map would otherwise live for the whole process.
const MAX_TRACKED_SITES: usize = 64;
/// Hosts that queue with the site they mirror rather than on their own.
const SITE_ALIASES: &[(&str, &str)] = &[
    ("html.duckduckgo.com", "duckduckgo.com"),
    ("lite.duckduckgo.com", "duckduckgo.com"),
];

/// Paces requests per site.
///
/// Guarantees two things per site: one request in flight at a time, and at
/// least [`MIN_SITE_REQUEST_INTERVAL`] between the starts of consecutive
/// requests. A slot covers a single request, never a retry sequence or a
/// challenge, so a slow site delays its own next request instead
/// of every request queued behind it. Waiting is capped by
/// [`SITE_SLOT_WAIT_TIMEOUT`]; past that the request is sent unpaced. Different
/// sites never wait on each other.
pub(crate) struct SitePacer {
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
    pub(crate) fn with_interval(min_interval: Duration) -> Self {
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
    pub(crate) async fn acquire(&self, url: &str) -> Option<OwnedMutexGuard<Option<Instant>>> {
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
