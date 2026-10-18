//! The page cache: pages, settled refusals, the archive lookups and archived
//! copies, and the bounds.

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
        read_via: ReadVia::Direct,
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

/// Fragments and host case do not change the resource; path case and a
/// trailing slash can, so cache identity preserves both.
#[tokio::test]
async fn a_url_that_differs_only_cosmetically_is_the_same_entry() {
    let dir = TempDir::new().unwrap();
    let clock = TestClock::new();
    let cache = cache(&dir, &clock);

    cache
        .remember_page(
            "https://Example.test/a/b",
            &page("https://Example.test/a/b", &clock),
        )
        .await;

    for variant in [
        "https://example.test/a/b",
        "https://EXAMPLE.test/a/b#section",
        "  https://example.test/a/b  ",
    ] {
        assert!(
            cache.get(variant).await.is_some(),
            "{variant} should have hit the stored page"
        );
    }
    for distinct in ["https://example.test/a/b/", "https://example.test/A/b"] {
        assert!(
            cache.get(distinct).await.is_none(),
            "{distinct} is a distinct resource"
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
            ArchiveAnswer::NothingArchived,
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
        cache
            .remember_refusal(&url, status, "refused", ArchiveAnswer::NothingArchived)
            .await;
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
        cache
            .remember_refusal(&url, status, "refused", ArchiveAnswer::NothingArchived)
            .await;
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
            ArchiveAnswer::NothingArchived,
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

/// Pages are stored whole now, so a count alone let the cache hold
/// gigabytes. The oldest entries go until the bytes fit too.
#[tokio::test]
async fn the_cache_stops_growing_at_its_byte_bound() {
    let dir = TempDir::new().unwrap();
    let clock = TestClock::new();
    let cache = cache(&dir, &clock);
    let mut big = page("https://example.test/big", &clock);
    big.content = "x".repeat(10_000);
    for index in 0..10 {
        let url = format!("https://example.test/{index}");
        cache.remember_page(&url, &big).await;
    }
    let newest = cache.path_for("https://example.test/9");
    sweep(
        &cache.dir,
        &newest,
        Bounds {
            entries: MAX_ENTRIES,
            bytes: 35_000,
        },
    );
    let mut count = 0usize;
    let mut bytes = 0u64;
    let mut entries = tokio::fs::read_dir(cache.dir.clone()).await.unwrap();
    while let Some(entry) = entries.next_entry().await.unwrap() {
        count += 1;
        bytes += entry.metadata().await.unwrap().len();
    }
    assert!(bytes <= 35_000, "{bytes} bytes in {count} entries");
    assert!(count >= 3, "only the oldest go: {count} left");
    assert!(cache.get("https://example.test/9").await.is_some());
}

fn capture(timestamp: &str) -> Capture {
    Capture {
        timestamp: timestamp.to_string(),
        captured_at: chrono::NaiveDateTime::parse_from_str(timestamp, "%Y%m%d%H%M%S")
            .unwrap()
            .and_utc(),
    }
}

fn url(raw: &str) -> Url {
    Url::parse(raw).unwrap()
}

/// The archive could not answer (refused, failed, paused): a copy may still
/// exist, so the refusal stays unsettled and the next read asks again.
#[tokio::test]
async fn a_refusal_the_archive_could_not_answer_is_not_remembered() {
    let dir = TempDir::new().unwrap();
    let clock = TestClock::new();
    let cache = cache(&dir, &clock);
    let gone = "https://gone.test/page";
    cache
        .remember_refusal(
            gone,
            StatusCode::NOT_FOUND,
            "HTTP 404",
            ArchiveAnswer::Unanswered,
        )
        .await;
    assert_eq!(cache.get(gone).await, None);

    // With the fallback off there is nothing more to ask: settled as before.
    cache
        .remember_refusal(
            gone,
            StatusCode::NOT_FOUND,
            "HTTP 404",
            ArchiveAnswer::NotAsked,
        )
        .await;
    assert!(matches!(
        cache.get(gone).await,
        Some(CachedFetch::Refusal { .. })
    ));
}

/// A rescued URL is cached as its archived copy, which replaces any earlier
/// refusal and keeps its label.
#[tokio::test]
async fn a_rescued_page_replaces_a_refusal_and_keeps_its_label() {
    let dir = TempDir::new().unwrap();
    let clock = TestClock::new();
    let cache = cache(&dir, &clock);
    let gone = "https://gone.test/page";
    cache
        .remember_refusal(
            gone,
            StatusCode::NOT_FOUND,
            "HTTP 404",
            ArchiveAnswer::NothingArchived,
        )
        .await;
    let mut rescued = page(
        "https://web.archive.org/web/20150102000131/https://gone.test/page",
        &clock,
    );
    rescued.read_via = ReadVia::Archive {
        captured_at: capture("20150102000131").captured_at,
        original: gone.to_string(),
    };
    cache.remember_page(gone, &rescued).await;
    match cache.get(gone).await {
        Some(CachedFetch::Page(page)) => assert_eq!(page.read_via, rescued.read_via),
        other => panic!("expected the archived copy, got {other:?}"),
    }
}

/// A lookup is trusted for the archive-lookup TTL (a week), a miss included,
/// and asked again after it.
#[tokio::test]
async fn an_archive_lookup_is_held_for_its_ttl() {
    let dir = TempDir::new().unwrap();
    let clock = TestClock::new();
    let cache = cache(&dir, &clock);
    let found = url("https://example.test/a");
    let missing = url("https://example.test/b");
    let day = NaiveDate::from_ymd_opt(2015, 1, 1).unwrap();
    cache
        .remember_archive_lookup(&found, Some(day), Some(&capture("20150102000131")))
        .await;
    cache.remember_archive_lookup(&missing, None, None).await;

    assert_eq!(
        cache.archive_lookup(&found, Some(day)).await,
        Some(CachedLookup::Found(capture("20150102000131")))
    );
    // Another day is another lookup.
    assert_eq!(cache.archive_lookup(&found, None).await, None);
    assert_eq!(
        cache.archive_lookup(&missing, None).await,
        Some(CachedLookup::NothingArchived)
    );

    let ttl_hours =
        i64::try_from(SourcingPolicy::default().archive_lookup_ttl.as_secs() / 3600).unwrap();
    clock.advance_hours(ttl_hours + 1);
    assert_eq!(cache.archive_lookup(&found, Some(day)).await, None);
    assert_eq!(cache.archive_lookup(&missing, None).await, None);
}

/// An archived copy lives under (original, capture time) with no TTL, is
/// shared by links that differ only in scheme, and never answers a plain
/// page lookup.
#[tokio::test]
async fn an_archived_copy_is_kept_under_its_archive_key() {
    let dir = TempDir::new().unwrap();
    let clock = TestClock::new();
    let cache = cache(&dir, &clock);
    let original = url("https://example.test/a");
    let taken = capture("20150102000131");
    let copy = page(
        "https://web.archive.org/web/20150102000131/https://example.test/a",
        &clock,
    );
    cache.remember_archived(&original, &taken, &copy).await;

    clock.advance_hours(24 * 365);
    assert_eq!(cache.archived(&original, &taken).await, Some(copy.clone()));
    assert_eq!(
        cache.archived(&url("http://example.test/a"), &taken).await,
        Some(copy)
    );
    assert_eq!(
        cache.archived(&original, &capture("20200101000000")).await,
        None
    );
    assert_eq!(cache.get("https://example.test/a").await, None);
}
