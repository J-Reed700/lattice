//! What this app has already read from the web, kept across turns.
//!
//! A deep-research turn searches, then opens the top results. The turn's own
//! [`FetchMemory`] stops it reading the same page twice, but it dies with the
//! turn: a follow-up question, a regenerate, or the user opening a cited
//! article in the reader all went back to the network for text we already had
//! — and back to sites that had already answered 403. That is slow, it is how
//! we get rate limited, and it is rude to the sites.
//!
//! So pages live on disk instead, one JSON file per entry under `web-cache/`
//! in the app data directory. There is deliberately no table and no migration:
//! nothing here is worth keeping, and a user who deletes the folder loses
//! nothing but a few seconds.
//!
//! Refusals are remembered too, but only the ones a retry a minute later
//! cannot change — an authorization wall, a missing page, a legal block. A
//! timeout, a dropped connection, a 429 or a 5xx are the site having a moment,
//! and those must stay retryable.
//!
//! Every failure in here is a miss. A broken cache directory, a half-written
//! file, a disk that is full — none of them may stop a fetch, so they are
//! logged and stepped over.
//!
//! [`FetchMemory`]: crate::features::conversation::chat

use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tracing::{debug, warn};

/// How long a page that was read stays good. Long enough that a session's
/// worth of follow-ups and regenerates all read it once; short enough that a
/// page the user comes back to tomorrow is read fresh.
const PAGE_TTL_SECONDS: i64 = 24 * 60 * 60;

/// How long a refusal is taken at its word. Short, because the reasons a site
/// refuses are the reasons it may stop refusing: a paywall changes, a page
/// comes back, a block is lifted.
const REFUSAL_TTL_SECONDS: i64 = 60 * 60;

/// Most entries kept on disk. Pages are capped at tens of kilobytes each, so
/// this is a few tens of megabytes at worst.
const MAX_ENTRIES: usize = 500;

/// The statuses worth remembering: the site answered, and it will answer the
/// same way if we ask again straight away. Everything else — timeouts,
/// connection errors, 429, 5xx — is the site having a moment and stays
/// retryable.
fn refusal_is_settled(status: StatusCode) -> bool {
    matches!(
        status,
        StatusCode::UNAUTHORIZED
            | StatusCode::FORBIDDEN
            | StatusCode::NOT_FOUND
            | StatusCode::GONE
            | StatusCode::UNAVAILABLE_FOR_LEGAL_REASONS
    )
}

/// A page the cache is holding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(in crate::features::web) struct CachedPage {
    /// Where the fetch actually ended, which a redirect can change.
    pub(in crate::features::web) final_url: String,
    pub(in crate::features::web) title: Option<String>,
    pub(in crate::features::web) content: String,
    pub(in crate::features::web) content_truncated: bool,
    pub(in crate::features::web) word_count: usize,
    pub(in crate::features::web) content_type: Option<String>,
    /// When the text was read off the network, not when it was served from
    /// here. The reader shows this, so it has to be the real moment.
    pub(in crate::features::web) fetched_at: DateTime<Utc>,
}

/// What a lookup found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::features::web) enum CachedFetch {
    /// The page, still inside its TTL.
    Page(CachedPage),
    /// The site refused this URL recently, and said so in a way that a retry
    /// now would not change. `reason` is the live path's own wording.
    Refusal { reason: String },
}

/// One cache file.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
enum Entry {
    Page(CachedPage),
    Refusal {
        reason: String,
        failed_at: DateTime<Utc>,
    },
}

/// What the cache reads the time from. Production passes `Utc::now`; the tests
/// hand over a fixed instant, so expiry is exercised without sleeping.
type Clock = Box<dyn Fn() -> DateTime<Utc> + Send + Sync>;

pub(in crate::features::web) struct PageCache {
    dir: PathBuf,
    now: Clock,
}

impl PageCache {
    pub(in crate::features::web) fn new(dir: PathBuf) -> Self {
        Self {
            dir,
            now: Box::new(Utc::now),
        }
    }

    #[cfg(test)]
    fn with_clock(dir: PathBuf, now: impl Fn() -> DateTime<Utc> + Send + Sync + 'static) -> Self {
        Self {
            dir,
            now: Box::new(now),
        }
    }

    /// What the cache holds for `url`, if anything still current.
    ///
    /// An entry past its TTL is deleted on the way out: the request that found
    /// it is about to replace it anyway, and leaving it would have it counted
    /// against the bound.
    pub(in crate::features::web) async fn get(&self, url: &str) -> Option<CachedFetch> {
        let path = self.path_for(url);
        let raw = match tokio::fs::read(&path).await {
            Ok(raw) => raw,
            // A miss is the normal case, so it is not worth a log line.
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return None,
            Err(error) => {
                warn!(url, %error, "Could not read a page cache entry; fetching instead");
                return None;
            }
        };
        let entry = match serde_json::from_slice::<Entry>(&raw) {
            Ok(entry) => entry,
            Err(error) => {
                // A truncated or older-shaped file. The next store overwrites it.
                debug!(url, %error, "Discarding an unreadable page cache entry");
                return None;
            }
        };

        let now = (self.now)();
        let (stamp, ttl) = match &entry {
            Entry::Page(page) => (page.fetched_at, PAGE_TTL_SECONDS),
            Entry::Refusal { failed_at, .. } => (*failed_at, REFUSAL_TTL_SECONDS),
        };
        if now.signed_duration_since(stamp).num_seconds() > ttl {
            remove_quietly(&path).await;
            return None;
        }

        Some(match entry {
            Entry::Page(page) => CachedFetch::Page(page),
            Entry::Refusal { reason, .. } => CachedFetch::Refusal { reason },
        })
    }

    /// Keep `page`, filed under the URL that was asked for and, when a
    /// redirect moved it, under where it landed as well. The model and the
    /// reader cite whichever of the two they happened to see.
    pub(in crate::features::web) async fn remember_page(
        &self,
        requested_url: &str,
        page: &CachedPage,
    ) {
        let entry = Entry::Page(page.clone());
        self.write(requested_url, &entry).await;
        if normalize(&page.final_url) != normalize(requested_url) {
            self.write(&page.final_url, &entry).await;
        }
    }

    /// Keep the fact that `url` was refused, if the status means it will be
    /// refused again. Anything else is dropped on the floor.
    pub(in crate::features::web) async fn remember_refusal(
        &self,
        url: &str,
        status: StatusCode,
        reason: &str,
    ) {
        if !refusal_is_settled(status) {
            return;
        }
        let entry = Entry::Refusal {
            reason: reason.to_string(),
            failed_at: (self.now)(),
        };
        self.write(url, &entry).await;
    }

    /// Where `url`'s entry lives. The name is a hash because a URL is not a
    /// file name: it carries slashes, query strings and characters no file
    /// system agrees on, and it can be longer than any of them allow.
    fn path_for(&self, url: &str) -> PathBuf {
        let digest = Sha256::digest(normalize(url).as_bytes());
        self.dir.join(format!("{digest:x}.json"))
    }

    async fn write(&self, url: &str, entry: &Entry) {
        let path = self.path_for(url);
        let Ok(encoded) = serde_json::to_vec(entry) else {
            debug!(url, "Could not encode a page cache entry; not caching it");
            return;
        };
        if let Err(error) = tokio::fs::create_dir_all(&self.dir).await {
            warn!(%error, "Could not create the page cache directory; not caching");
            return;
        }
        if let Err(error) = tokio::fs::write(&path, &encoded).await {
            warn!(url, %error, "Could not write a page cache entry");
            return;
        }
        self.prune(&path).await;
    }

    /// Bring the directory back under [`MAX_ENTRIES`] by deleting the entries
    /// written longest ago. `just_written` is spared: it is the one entry we
    /// know is wanted, and file timestamps are too coarse to rely on when a
    /// wave of fetches lands together.
    ///
    /// The whole sweep runs in one blocking hop rather than as a few hundred
    /// separate `tokio::fs` calls, which is the difference between a prune
    /// costing nothing and a prune costing more than the fetch it followed.
    /// Nothing here can fail a fetch — the caller has already written its
    /// entry and does not wait on the outcome.
    async fn prune(&self, just_written: &Path) {
        let dir = self.dir.clone();
        let just_written = just_written.to_path_buf();
        let swept = tokio::task::spawn_blocking(move || sweep(&dir, &just_written)).await;
        if let Err(error) = swept {
            debug!(%error, "The page cache prune did not run");
        }
    }
}

/// Delete the oldest entries in `dir` until it holds at most [`MAX_ENTRIES`].
fn sweep(dir: &Path, just_written: &Path) {
    let listing = match std::fs::read_dir(dir) {
        Ok(listing) => listing,
        Err(error) => {
            debug!(%error, "Could not list the page cache; skipping the prune");
            return;
        }
    };

    let mut ages: Vec<(std::time::SystemTime, PathBuf)> = Vec::new();
    for entry in listing.flatten() {
        let path = entry.path();
        if path == just_written {
            continue;
        }
        let modified = entry
            .metadata()
            .and_then(|meta| meta.modified())
            .unwrap_or(std::time::UNIX_EPOCH);
        ages.push((modified, path));
    }

    // The just-written entry is not in the list but does occupy a slot.
    let over = (ages.len() + 1).saturating_sub(MAX_ENTRIES);
    if over == 0 {
        return;
    }
    ages.sort_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.cmp(&right.1)));
    for (_, path) in ages.into_iter().take(over) {
        if let Err(error) = std::fs::remove_file(&path) {
            debug!(path = %path.display(), %error, "Could not drop a page cache entry");
        }
    }
}

async fn remove_quietly(path: &Path) {
    if let Err(error) = tokio::fs::remove_file(path).await {
        if error.kind() != std::io::ErrorKind::NotFound {
            debug!(path = %path.display(), %error, "Could not drop a page cache entry");
        }
    }
}

/// Compare URLs the way a server would: the scheme and host are
/// case-insensitive and a trailing slash on the path is not a different page,
/// so `HTTPS://Example.com/a/` must not read as new after `https://example.com/a`.
///
/// Deliberately a copy of the conversation feature's `fetch_memory::normalize`
/// rather than a shared import: a turn's memory and a disk cache are different
/// features with the same idea of sameness, and the web feature does not reach
/// into the chat feature.
fn normalize(url: &str) -> String {
    let trimmed = url.trim();
    let Ok(mut parsed) = url::Url::parse(trimmed) else {
        return trimmed.to_ascii_lowercase();
    };
    parsed.set_fragment(None);
    let normalized = parsed.as_str().trim_end_matches('/').to_string();
    normalized.to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use std::sync::{Arc, Mutex};
    use tempfile::TempDir;

    /// A clock the test moves by hand, so expiry costs no wall-clock time.
    #[derive(Clone)]
    struct TestClock(Arc<Mutex<DateTime<Utc>>>);

    impl TestClock {
        fn new() -> Self {
            Self(Arc::new(Mutex::new(
                Utc.with_ymd_and_hms(2026, 9, 19, 12, 0, 0).unwrap(),
            )))
        }

        fn advance_hours(&self, hours: i64) {
            let mut now = self.0.lock().expect("test clock poisoned");
            *now += chrono::Duration::hours(hours);
        }

        fn reader(&self) -> impl Fn() -> DateTime<Utc> + Send + Sync + 'static {
            let shared = Arc::clone(&self.0);
            move || *shared.lock().expect("test clock poisoned")
        }
    }

    fn cache(dir: &TempDir, clock: &TestClock) -> PageCache {
        PageCache::with_clock(dir.path().join("web-cache"), clock.reader())
    }

    fn page(final_url: &str, clock: &TestClock) -> CachedPage {
        CachedPage {
            final_url: final_url.to_string(),
            title: Some("Silo Season 2, Explained".to_string()),
            content: "one two three four".to_string(),
            content_truncated: false,
            word_count: 4,
            content_type: Some("text/html".to_string()),
            fetched_at: (clock.reader())(),
        }
    }

    #[tokio::test]
    async fn a_page_that_was_read_is_handed_back_without_a_fetch() {
        let dir = TempDir::new().unwrap();
        let clock = TestClock::new();
        let cache = cache(&dir, &clock);

        let stored = page("https://example.test/recap", &clock);
        cache
            .remember_page("https://example.test/recap", &stored)
            .await;

        assert_eq!(
            cache.get("https://example.test/recap").await,
            Some(CachedFetch::Page(stored))
        );
    }

    #[tokio::test]
    async fn a_page_nobody_has_read_is_a_miss() {
        let dir = TempDir::new().unwrap();
        let clock = TestClock::new();
        assert_eq!(
            cache(&dir, &clock).get("https://example.test/never").await,
            None
        );
    }

    /// The model rarely echoes a URL back byte for byte — it drops a trailing
    /// slash or a fragment. Those are the same page and must not buy a fetch.
    #[tokio::test]
    async fn a_url_that_differs_only_cosmetically_is_the_same_entry() {
        let dir = TempDir::new().unwrap();
        let clock = TestClock::new();
        let cache = cache(&dir, &clock);

        cache
            .remember_page(
                "https://Example.test/a/b/",
                &page("https://Example.test/a/b/", &clock),
            )
            .await;

        for variant in [
            "https://example.test/a/b",
            "https://example.test/a/b/",
            "https://EXAMPLE.test/a/b#section",
            "  https://example.test/a/b  ",
        ] {
            assert!(
                cache.get(variant).await.is_some(),
                "{variant} should have hit the stored page"
            );
        }
        assert_eq!(cache.get("https://example.test/a/c").await, None);
    }

    /// A search result's URL and the page it redirects to are both cited, so a
    /// hit on either must not cost a second fetch.
    #[tokio::test]
    async fn a_redirected_page_is_found_under_both_urls() {
        let dir = TempDir::new().unwrap();
        let clock = TestClock::new();
        let cache = cache(&dir, &clock);

        cache
            .remember_page(
                "https://example.test/short",
                &page("https://example.test/articles/full-story", &clock),
            )
            .await;

        assert!(cache.get("https://example.test/short").await.is_some());
        assert!(cache
            .get("https://example.test/articles/full-story")
            .await
            .is_some());
    }

    #[tokio::test]
    async fn a_page_older_than_a_day_is_read_again() {
        let dir = TempDir::new().unwrap();
        let clock = TestClock::new();
        let cache = cache(&dir, &clock);

        cache
            .remember_page(
                "https://example.test/recap",
                &page("https://example.test/recap", &clock),
            )
            .await;

        clock.advance_hours(23);
        assert!(cache.get("https://example.test/recap").await.is_some());

        clock.advance_hours(2);
        assert_eq!(cache.get("https://example.test/recap").await, None);
    }

    #[tokio::test]
    async fn a_settled_refusal_is_remembered_and_says_so() {
        let dir = TempDir::new().unwrap();
        let clock = TestClock::new();
        let cache = cache(&dir, &clock);

        cache
            .remember_refusal(
                "https://blocked.test/a",
                StatusCode::FORBIDDEN,
                "HTTP 403 Forbidden",
            )
            .await;

        assert_eq!(
            cache.get("https://blocked.test/a").await,
            Some(CachedFetch::Refusal {
                reason: "HTTP 403 Forbidden".to_string(),
            })
        );
    }

    /// A site having a moment must stay retryable. Only an answer that will be
    /// the same answer a minute later is worth keeping.
    #[tokio::test]
    async fn only_a_refusal_that_will_not_change_is_kept() {
        let dir = TempDir::new().unwrap();
        let clock = TestClock::new();
        let cache = cache(&dir, &clock);

        for status in [
            StatusCode::UNAUTHORIZED,
            StatusCode::FORBIDDEN,
            StatusCode::NOT_FOUND,
            StatusCode::GONE,
            StatusCode::UNAVAILABLE_FOR_LEGAL_REASONS,
        ] {
            let url = format!("https://kept.test/{}", status.as_u16());
            cache.remember_refusal(&url, status, "refused").await;
            assert!(
                cache.get(&url).await.is_some(),
                "HTTP {status} should have been remembered"
            );
        }

        for status in [
            StatusCode::TOO_MANY_REQUESTS,
            StatusCode::INTERNAL_SERVER_ERROR,
            StatusCode::BAD_GATEWAY,
            StatusCode::SERVICE_UNAVAILABLE,
            StatusCode::GATEWAY_TIMEOUT,
            StatusCode::REQUEST_TIMEOUT,
            StatusCode::BAD_REQUEST,
        ] {
            let url = format!("https://retried.test/{}", status.as_u16());
            cache.remember_refusal(&url, status, "refused").await;
            assert_eq!(
                cache.get(&url).await,
                None,
                "HTTP {status} should stay retryable"
            );
        }
    }

    #[tokio::test]
    async fn a_refusal_older_than_an_hour_is_tried_again() {
        let dir = TempDir::new().unwrap();
        let clock = TestClock::new();
        let cache = cache(&dir, &clock);

        cache
            .remember_refusal(
                "https://blocked.test/a",
                StatusCode::FORBIDDEN,
                "HTTP 403 Forbidden",
            )
            .await;

        clock.advance_hours(2);
        assert_eq!(cache.get("https://blocked.test/a").await, None);
    }

    /// A half-written file must read as "we have not got it", never as a panic
    /// and never as an empty page handed to the model.
    #[tokio::test]
    async fn a_corrupt_entry_is_a_miss() {
        let dir = TempDir::new().unwrap();
        let clock = TestClock::new();
        let cache = cache(&dir, &clock);

        cache
            .remember_page(
                "https://example.test/recap",
                &page("https://example.test/recap", &clock),
            )
            .await;
        let path = cache.path_for("https://example.test/recap");
        tokio::fs::write(&path, b"{\"outcome\":\"pa").await.unwrap();

        assert_eq!(cache.get("https://example.test/recap").await, None);
    }

    /// A cache directory that cannot be read or written is a slow app, not a
    /// broken one: every operation has to degrade to a miss.
    #[tokio::test]
    async fn an_unusable_directory_is_a_miss_rather_than_a_failure() {
        let dir = TempDir::new().unwrap();
        let clock = TestClock::new();
        // A file where the directory should be: nothing can be created inside it.
        let blocked = dir.path().join("web-cache");
        tokio::fs::write(&blocked, b"not a directory")
            .await
            .unwrap();
        let cache = PageCache::with_clock(blocked, clock.reader());

        cache
            .remember_page(
                "https://example.test/recap",
                &page("https://example.test/recap", &clock),
            )
            .await;
        assert_eq!(cache.get("https://example.test/recap").await, None);
    }

    /// A long research session must not fill the disk with pages nobody will
    /// ask for again.
    #[tokio::test]
    async fn the_cache_stops_growing_at_the_bound() {
        let dir = TempDir::new().unwrap();
        let clock = TestClock::new();
        let cache = cache(&dir, &clock);

        for index in 0..(MAX_ENTRIES + 20) {
            let url = format!("https://example.test/{index}");
            cache.remember_page(&url, &page(&url, &clock)).await;
        }

        let mut count = 0usize;
        let mut entries = tokio::fs::read_dir(cache.dir.clone()).await.unwrap();
        while entries.next_entry().await.unwrap().is_some() {
            count += 1;
        }
        assert!(count <= MAX_ENTRIES, "got {count} entries");

        // The most recent write survived the prune it triggered.
        let newest = format!("https://example.test/{}", MAX_ENTRIES + 19);
        assert!(cache.get(&newest).await.is_some());
    }
}
