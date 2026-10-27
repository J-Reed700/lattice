//! What this app has already read from the web, kept across turns as one JSON
//! file per entry under `web-cache/` (no table or migration: deleting it loses
//! nothing), so a follow-up, a regenerate or the reader does not refetch pages
//! or retry sites that refused. Only refusals a retry cannot change are
//! remembered (an authorization wall, a missing page, a legal block), and only
//! when the archive had nothing either. The Internet Archive's lookups and
//! copies live here too (design §3.7, §3.8). Every failure here is a miss:
//! nothing in the cache may stop a fetch.
//!
//! [`FetchMemory`]: crate::features::conversation::chat

use std::path::{Path, PathBuf};
use std::time::Duration;

use chrono::{DateTime, NaiveDate, Utc};
use lattice_core::application::contracts::web::ReadVia;
use lattice_core::application::services::sourcing::cache_policy;
use lattice_core::domain::sourcing::{canonical, SourcingPolicy};
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tracing::{debug, warn};
use url::Url;

use super::wayback::Capture;

/// How long a page that was read stays good. Long enough that a session's
/// worth of follow-ups and regenerates all read it once; short enough that a
/// page the user comes back to tomorrow is read fresh.
const PAGE_TTL_SECONDS: i64 = 24 * 60 * 60;

/// How long a refusal is taken at its word. Short, because the reasons a site
/// refuses are the reasons it may stop refusing: a paywall changes, a page
/// comes back, a block is lifted.
const REFUSAL_TTL_SECONDS: i64 = 60 * 60;

/// Most entries kept on disk.
const MAX_ENTRIES: usize = 500;

/// Most bytes kept on disk. Pages are stored whole, up to the fetcher's
/// decoded-body limit each, so a count alone could hold gigabytes. This is an
/// eviction bound: an evicted page is fetched again, nothing is cut.
const MAX_BYTES: u64 = 64 * 1024 * 1024;

/// How much the directory may hold before the oldest entries go.
#[derive(Debug, Clone, Copy)]
struct Bounds {
    entries: usize,
    bytes: u64,
}

const BOUNDS: Bounds = Bounds {
    entries: MAX_ENTRIES,
    bytes: MAX_BYTES,
};

/// The statuses worth remembering: the site answered, and it will answer the
/// same way if we ask again straight away. Everything else — timeouts,
/// connection errors, 429, 5xx — is the site having a moment and stays
/// retryable.
pub(in crate::features::web) fn refusal_is_settled(status: StatusCode) -> bool {
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
    /// How the text was read, so a page served from here keeps its label.
    pub(in crate::features::web) read_via: ReadVia,
}

/// What the archive said about a URL the live site would not give up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::features::web) enum ArchiveAnswer {
    /// The archive holds no readable copy.
    NothingArchived,
    /// The archive was not asked: the user turned the fallback off, or the
    /// URL is an archived copy already.
    NotAsked,
    /// The archive failed, refused or is paused, so a copy may exist.
    Unanswered,
}

/// What an earlier archive lookup found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::features::web) enum CachedLookup {
    Found(Capture),
    NothingArchived,
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
    /// An archived copy, under its archive key.
    Archived(CachedPage),
    /// An archive lookup, under its lookup key; `found: None` is a settled
    /// "nothing archived".
    ArchiveLookup {
        found: Option<Capture>,
        looked_at: DateTime<Utc>,
    },
}

/// What the cache reads the time from. Production passes `Utc::now`; the tests
/// hand over a fixed instant, so expiry is exercised without sleeping.
type Clock = Box<dyn Fn() -> DateTime<Utc> + Send + Sync>;

pub(in crate::features::web) struct PageCache {
    dir: PathBuf,
    now: Clock,
    /// How long an archive lookup is trusted (`SourcingPolicy`).
    archive_lookup_ttl: Duration,
}

impl PageCache {
    pub(in crate::features::web) fn new(dir: PathBuf) -> Self {
        Self::with_clock(dir, Utc::now)
    }

    pub(in crate::features::web) fn with_clock(
        dir: PathBuf,
        now: impl Fn() -> DateTime<Utc> + Send + Sync + 'static,
    ) -> Self {
        Self {
            dir,
            now: Box::new(now),
            archive_lookup_ttl: SourcingPolicy::default().archive_lookup_ttl,
        }
    }

    /// What the cache holds for `url`, if anything still current.
    ///
    /// An entry past its TTL is deleted on the way out: the request that found
    /// it is about to replace it anyway, and leaving it would have it counted
    /// against the bound.
    pub(in crate::features::web) async fn get(&self, url: &str) -> Option<CachedFetch> {
        let path = self.path_for(url);
        let entry = read_entry(&path, url).await?;
        let now = (self.now)();
        let (stamp, ttl) = match &entry {
            Entry::Page(page) => (page.fetched_at, PAGE_TTL_SECONDS),
            Entry::Refusal { failed_at, .. } => (*failed_at, REFUSAL_TTL_SECONDS),
            Entry::Archived(_) | Entry::ArchiveLookup { .. } => {
                debug!(url, "An archive entry under a page key; ignoring it");
                return None;
            }
        };
        if now.signed_duration_since(stamp).num_seconds() > ttl {
            remove_quietly(&path).await;
            return None;
        }

        match entry {
            Entry::Page(page) => Some(CachedFetch::Page(page)),
            Entry::Refusal { reason, .. } => Some(CachedFetch::Refusal { reason }),
            Entry::Archived(_) | Entry::ArchiveLookup { .. } => None,
        }
    }

    /// The archived copy of `original` taken at `capture`, if it was read
    /// before. No TTL: a capture never changes.
    pub(in crate::features::web) async fn archived(
        &self,
        original: &Url,
        capture: &Capture,
    ) -> Option<CachedPage> {
        let key = archived_key(original, capture);
        match read_entry(&self.path_for_key(&key), &key).await? {
            Entry::Archived(page) => Some(page),
            _ => None,
        }
    }

    /// Keep an archived copy under its archive key.
    pub(in crate::features::web) async fn remember_archived(
        &self,
        original: &Url,
        capture: &Capture,
        page: &CachedPage,
    ) {
        let key = archived_key(original, capture);
        self.write_key(&key, &Entry::Archived(page.clone())).await;
    }

    /// What the archive said about `url` near `as_of` (the latest when
    /// `None`), while that answer is inside the archive-lookup TTL.
    pub(in crate::features::web) async fn archive_lookup(
        &self,
        url: &Url,
        as_of: Option<NaiveDate>,
    ) -> Option<CachedLookup> {
        let key = lookup_key(url, as_of);
        let path = self.path_for_key(&key);
        let Entry::ArchiveLookup { found, looked_at } = read_entry(&path, &key).await? else {
            return None;
        };
        if !cache_policy::is_fresh(looked_at, (self.now)(), self.archive_lookup_ttl) {
            remove_quietly(&path).await;
            return None;
        }
        Some(found.map_or(CachedLookup::NothingArchived, CachedLookup::Found))
    }

    /// Keep a settled lookup: the capture found, or `None` for "nothing
    /// archived". An unanswered lookup is never kept.
    pub(in crate::features::web) async fn remember_archive_lookup(
        &self,
        url: &Url,
        as_of: Option<NaiveDate>,
        found: Option<&Capture>,
    ) {
        let entry = Entry::ArchiveLookup {
            found: found.cloned(),
            looked_at: (self.now)(),
        };
        self.write_key(&lookup_key(url, as_of), &entry).await;
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
    /// refused again and the archive had nothing either (or was not asked).
    /// An archive that could not answer leaves the refusal unsettled: the
    /// next read asks again, and the archive's breaker decides whether it
    /// is asked. Anything else is dropped on the floor.
    pub(in crate::features::web) async fn remember_refusal(
        &self,
        url: &str,
        status: StatusCode,
        reason: &str,
        archive: ArchiveAnswer,
    ) {
        if !refusal_is_settled(status) || archive == ArchiveAnswer::Unanswered {
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
        self.path_for_key(&normalize(url))
    }

    /// Where the entry for an already-normalised key lives.
    fn path_for_key(&self, key: &str) -> PathBuf {
        let digest = Sha256::digest(key.as_bytes());
        self.dir.join(format!("{digest:x}.json"))
    }

    async fn write(&self, url: &str, entry: &Entry) {
        self.write_at(self.path_for(url), url, entry).await;
    }

    async fn write_key(&self, key: &str, entry: &Entry) {
        self.write_at(self.path_for_key(key), key, entry).await;
    }

    async fn write_at(&self, path: PathBuf, url: &str, entry: &Entry) {
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

    /// Brings the directory back under [`MAX_ENTRIES`] by deleting the oldest
    /// entries, sparing `just_written` (timestamps are too coarse when a wave of
    /// fetches lands together). One blocking sweep that never fails a fetch.
    async fn prune(&self, just_written: &Path) {
        let dir = self.dir.clone();
        let just_written = just_written.to_path_buf();
        let swept = tokio::task::spawn_blocking(move || sweep(&dir, &just_written, BOUNDS)).await;
        if let Err(error) = swept {
            debug!(%error, "The page cache prune did not run");
        }
    }
}

/// Delete the oldest entries in `dir` until it holds at most `bounds.entries`
/// entries and `bounds.bytes` bytes.
fn sweep(dir: &Path, just_written: &Path, bounds: Bounds) {
    let listing = match std::fs::read_dir(dir) {
        Ok(listing) => listing,
        Err(error) => {
            debug!(%error, "Could not list the page cache; skipping the prune");
            return;
        }
    };

    let mut ages: Vec<(std::time::SystemTime, u64, PathBuf)> = Vec::new();
    // The just-written entry is not in the list but does occupy a slot.
    let mut count = 1usize;
    let mut bytes = std::fs::metadata(just_written).map_or(0, |meta| meta.len());
    for entry in listing.flatten() {
        let path = entry.path();
        if path == just_written {
            continue;
        }
        let meta = entry.metadata().ok();
        let modified = meta
            .as_ref()
            .and_then(|meta| meta.modified().ok())
            .unwrap_or(std::time::UNIX_EPOCH);
        let size = meta.map_or(0, |meta| meta.len());
        count += 1;
        bytes = bytes.saturating_add(size);
        ages.push((modified, size, path));
    }
    if count <= bounds.entries && bytes <= bounds.bytes {
        return;
    }
    ages.sort_by(|left, right| left.0.cmp(&right.0).then_with(|| left.2.cmp(&right.2)));
    for (_, size, path) in ages {
        if count <= bounds.entries && bytes <= bounds.bytes {
            break;
        }
        if let Err(error) = std::fs::remove_file(&path) {
            debug!(path = %path.display(), %error, "Could not drop a page cache entry");
            continue;
        }
        count -= 1;
        bytes = bytes.saturating_sub(size);
    }
}

async fn remove_quietly(path: &Path) {
    if let Err(error) = tokio::fs::remove_file(path).await {
        if error.kind() != std::io::ErrorKind::NotFound {
            debug!(path = %path.display(), %error, "Could not drop a page cache entry");
        }
    }
}

/// One cache file's entry; a missing, unreadable or older-shaped file is a
/// miss, which the next store overwrites.
async fn read_entry(path: &Path, label: &str) -> Option<Entry> {
    let raw = match tokio::fs::read(path).await {
        Ok(raw) => raw,
        // A miss is the normal case, so it is not worth a log line.
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return None,
        Err(error) => {
            warn!(label, %error, "Could not read a page cache entry; fetching instead");
            return None;
        }
    };
    match serde_json::from_slice::<Entry>(&raw) {
        Ok(entry) => Some(entry),
        Err(error) => {
            debug!(label, %error, "Discarding an unreadable page cache entry");
            None
        }
    }
}

/// An archived copy's key: the original's canonical key and the capture
/// time (design §3.7), so every link to the same page shares its copies.
fn archived_key(original: &Url, capture: &Capture) -> String {
    format!(
        "archived|{}|{}",
        canonical::key(original),
        capture.timestamp
    )
}

fn lookup_key(url: &Url, as_of: Option<NaiveDate>) -> String {
    format!(
        "archive-lookup|{}",
        cache_policy::archive_lookup_key(url, as_of)
    )
}

fn normalize(url: &str) -> String {
    lattice_core::shared::http::url_identity::identity(url)
}

#[cfg(test)]
#[path = "page_cache_tests.rs"]
mod tests;
