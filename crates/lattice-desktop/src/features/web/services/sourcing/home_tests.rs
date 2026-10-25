//! A scoped site's home is read once a day and shared. A read that fails
//! keeps the scope as asked and is retried after a while; a search stopped
//! or out of time neither waits for the site nor stands in for its answer.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use chrono::{TimeZone, Utc};
use futures::future::BoxFuture;
use tokio_util::sync::CancellationToken;

use super::*;

struct FakeClock {
    now: parking_lot::Mutex<DateTime<Utc>>,
}

impl FakeClock {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            now: parking_lot::Mutex::new(Utc.with_ymd_and_hms(2026, 10, 25, 12, 0, 0).unwrap()),
        })
    }

    fn advance(&self, by: Duration) {
        *self.now.lock() += chrono::TimeDelta::from_std(by).unwrap();
    }
}

impl Clock for FakeClock {
    fn now(&self) -> DateTime<Utc> {
        *self.now.lock()
    }

    fn sleep(&self, wait: Duration) -> BoxFuture<'static, ()> {
        self.advance(wait);
        Box::pin(async {})
    }
}

/// One answer a scripted site gives.
#[derive(Clone)]
enum Reply {
    Hops(Vec<Hop>),
    Fails,
    Hangs,
}

/// Gives its replies in order, the last one to every read after it,
/// counting reads and noting the address read.
struct ScriptedHops {
    replies: parking_lot::Mutex<Vec<Reply>>,
    reads: AtomicUsize,
    asked: parking_lot::Mutex<Vec<String>>,
}

impl ScriptedHops {
    fn new(replies: Vec<Reply>) -> Arc<Self> {
        Arc::new(Self {
            replies: parking_lot::Mutex::new(replies),
            reads: AtomicUsize::new(0),
            asked: parking_lot::Mutex::new(Vec::new()),
        })
    }

    fn reads(&self) -> usize {
        self.reads.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl HopReader for ScriptedHops {
    async fn hops(&self, from: &Url) -> Result<Vec<Hop>, String> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        self.asked.lock().push(from.to_string());
        let reply = {
            let mut replies = self.replies.lock();
            if replies.len() > 1 {
                replies.remove(0)
            } else {
                replies.first().cloned().unwrap_or(Reply::Hops(Vec::new()))
            }
        };
        match reply {
            Reply::Hops(hops) => Ok(hops),
            Reply::Fails => Err("HTTP 503".to_owned()),
            Reply::Hangs => std::future::pending().await,
        }
    }
}

fn homes(reader: Arc<dyn HopReader>, clock: &Arc<FakeClock>) -> SiteHomes {
    SiteHomes::new(
        reader,
        SourcingPolicy::default(),
        Arc::clone(clock) as Arc<dyn Clock>,
    )
}

fn budget(within: Duration) -> Budget {
    Budget {
        deadline: Instant::now() + within,
        stop: CancellationToken::new(),
        requests: None,
    }
}

fn origin(site: &str) -> Origin {
    Origin::parse(site).unwrap()
}

fn moved_to(home: &str) -> Reply {
    Reply::Hops(vec![Hop {
        status: 301,
        to: Url::parse(home).unwrap(),
    }])
}

#[tokio::test]
async fn a_moved_site_is_read_once_and_every_search_shares_its_home() {
    let clock = FakeClock::new();
    let reader = ScriptedHops::new(vec![moved_to("https://platform.claude.com/docs/")]);
    let homes = homes(Arc::clone(&reader) as Arc<dyn HopReader>, &clock);

    for _ in 0..3 {
        let site = homes
            .of(
                origin("docs.anthropic.com"),
                &budget(Duration::from_secs(60)),
            )
            .await;
        assert_eq!(site.home(), &origin("platform.claude.com"));
    }
    assert_eq!(reader.reads(), 1);
    assert_eq!(
        reader.asked.lock().as_slice(),
        ["https://docs.anthropic.com/"]
    );
}

#[tokio::test]
async fn the_scope_is_read_at_its_own_address_path_and_all() {
    let clock = FakeClock::new();
    let reader = ScriptedHops::new(vec![Reply::Hops(Vec::new())]);
    let homes = homes(Arc::clone(&reader) as Arc<dyn HopReader>, &clock);

    let site = homes
        .of(
            origin("github.com/dotnet"),
            &budget(Duration::from_secs(60)),
        )
        .await;

    assert!(!site.moved());
    assert_eq!(
        reader.asked.lock().as_slice(),
        ["https://github.com/dotnet"]
    );
}

#[tokio::test]
async fn a_home_is_read_again_once_a_day_has_passed() {
    let clock = FakeClock::new();
    let reader = ScriptedHops::new(vec![Reply::Hops(Vec::new())]);
    let homes = homes(Arc::clone(&reader) as Arc<dyn HopReader>, &clock);
    let within = budget(Duration::from_secs(60));

    homes.of(origin("react.dev"), &within).await;
    clock.advance(SourcingPolicy::default().site_home_ttl - Duration::from_secs(1));
    homes.of(origin("react.dev"), &within).await;
    assert_eq!(reader.reads(), 1, "still fresh");

    clock.advance(Duration::from_secs(1));
    homes.of(origin("react.dev"), &within).await;
    assert_eq!(reader.reads(), 2, "a day old");
}

#[tokio::test]
async fn an_unreadable_site_is_searched_as_asked_and_read_again_after_a_while() {
    let clock = FakeClock::new();
    let reader = ScriptedHops::new(vec![
        Reply::Fails,
        moved_to("https://platform.claude.com/docs/"),
    ]);
    let homes = homes(Arc::clone(&reader) as Arc<dyn HopReader>, &clock);
    let within = budget(Duration::from_secs(60));
    let retry = SourcingPolicy::default().site_home_retry;

    let site = homes.of(origin("docs.anthropic.com"), &within).await;
    assert_eq!(site, SiteHome::unmoved(origin("docs.anthropic.com")));
    clock.advance(retry - Duration::from_secs(1));
    homes.of(origin("docs.anthropic.com"), &within).await;
    assert_eq!(
        reader.reads(),
        1,
        "a site that failed is not asked on every search"
    );

    clock.advance(Duration::from_secs(1));
    let site = homes.of(origin("docs.anthropic.com"), &within).await;
    assert_eq!(reader.reads(), 2);
    assert_eq!(site.home(), &origin("platform.claude.com"));
}

#[tokio::test]
async fn a_stopped_search_does_not_wait_for_a_hanging_site_or_stand_in_for_it() {
    let clock = FakeClock::new();
    let reader = ScriptedHops::new(vec![Reply::Hangs, Reply::Hops(Vec::new())]);
    let homes = homes(Arc::clone(&reader) as Arc<dyn HopReader>, &clock);

    let stopping = budget(Duration::from_secs(600));
    let stop = stopping.stop.clone();
    tokio::spawn(async move {
        tokio::task::yield_now().await;
        stop.cancel();
    });
    let site = homes.of(origin("slow.example"), &stopping).await;
    assert_eq!(site, SiteHome::unmoved(origin("slow.example")));
    assert_eq!(reader.reads(), 1, "the read had started");

    homes
        .of(origin("slow.example"), &budget(Duration::from_secs(60)))
        .await;
    assert_eq!(
        reader.reads(),
        2,
        "the stop is not remembered as the site's answer"
    );
}

#[tokio::test]
async fn a_search_out_of_time_does_not_wait_for_a_hanging_site() {
    let clock = FakeClock::new();
    let reader = ScriptedHops::new(vec![Reply::Hangs, Reply::Hops(Vec::new())]);
    let homes = homes(Arc::clone(&reader) as Arc<dyn HopReader>, &clock);

    let site = homes
        .of(origin("slow.example"), &budget(Duration::from_millis(20)))
        .await;
    assert_eq!(site, SiteHome::unmoved(origin("slow.example")));
    homes
        .of(origin("slow.example"), &budget(Duration::from_secs(60)))
        .await;
    assert_eq!(
        reader.reads(),
        2,
        "running out of time is not the site's answer"
    );
}

#[tokio::test]
async fn a_spent_search_reads_nothing() {
    let clock = FakeClock::new();
    let reader = ScriptedHops::new(vec![Reply::Hops(Vec::new())]);
    let homes = homes(Arc::clone(&reader) as Arc<dyn HopReader>, &clock);

    let stopped = budget(Duration::from_secs(60));
    stopped.stop.cancel();
    let out_of_requests = Budget {
        requests: Some(0),
        ..budget(Duration::from_secs(60))
    };
    for spent in [stopped, budget(Duration::ZERO), out_of_requests] {
        let site = homes.of(origin("react.dev"), &spent).await;
        assert_eq!(site, SiteHome::unmoved(origin("react.dev")));
    }
    assert_eq!(reader.reads(), 0);
}
