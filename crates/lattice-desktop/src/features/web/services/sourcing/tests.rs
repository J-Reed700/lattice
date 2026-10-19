//! The runner's resilience, over scripted finders and a clock that moves
//! only when the runner waits.

use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::{DateTime, TimeZone, Utc};
use futures::future::BoxFuture;
use lattice_core::application::contracts::settings::SourcingSettingsDto;
use lattice_core::application::ports::{Budget, SourceFinding};
use lattice_core::domain::sourcing::{
    FailureCause, FinderResult, Freshness, HealthState, Identifier, NeedKind, Origin, SkipReason,
    SourceNeed, SubjectHint, SubjectKind,
};
use tokio_util::sync::CancellationToken;

use super::runner::Clock;
use super::SourcingService;
use crate::features::web::mocks::web::ScriptedFinder;
use crate::features::web::services::finders::{
    bing_html, duckduckgo, github, site_discovery, FinderEntry,
};

/// A clock that stands still until the runner sleeps, then jumps.
struct FakeClock {
    now: parking_lot::Mutex<DateTime<Utc>>,
    slept: parking_lot::Mutex<Vec<Duration>>,
}

impl FakeClock {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            now: parking_lot::Mutex::new(Utc.with_ymd_and_hms(2026, 10, 18, 12, 0, 0).unwrap()),
            slept: parking_lot::Mutex::new(Vec::new()),
        })
    }

    fn advance(&self, by: Duration) {
        let mut now = self.now.lock();
        *now += chrono::TimeDelta::from_std(by).unwrap();
    }

    fn slept(&self) -> Vec<Duration> {
        self.slept.lock().clone()
    }
}

impl Clock for FakeClock {
    fn now(&self) -> DateTime<Utc> {
        *self.now.lock()
    }

    fn sleep(&self, wait: Duration) -> BoxFuture<'static, ()> {
        self.slept.lock().push(wait);
        self.advance(wait);
        Box::pin(async {})
    }
}

fn entry(finder: &Arc<ScriptedFinder>, page_size: usize) -> FinderEntry {
    FinderEntry {
        finder: Arc::clone(finder) as Arc<dyn lattice_core::application::ports::SourceFinder>,
        page_size,
    }
}

fn service(entries: Vec<FinderEntry>, clock: &Arc<FakeClock>) -> SourcingService {
    SourcingService::with_clock(
        entries,
        SourcingSettingsDto::default(),
        BTreeSet::new(),
        Arc::clone(clock) as Arc<dyn Clock>,
    )
}

fn web_need(want: usize, alternatives: &[&str]) -> SourceNeed {
    SourceNeed {
        kind: NeedKind::Web,
        query: "rust async".to_string(),
        subject: None,
        scope: None,
        freshness: Freshness::Any,
        want,
        alternatives: alternatives.iter().map(|text| text.to_string()).collect(),
        also: Vec::new(),
    }
}

fn budget(within: Duration) -> Budget {
    Budget {
        deadline: Instant::now() + within,
        stop: CancellationToken::new(),
        requests: None,
    }
}

const PLENTY: Duration = Duration::from_secs(600);

fn result_of<'a>(
    outcomes: &'a [lattice_core::domain::sourcing::FinderOutcome],
    finder: &str,
) -> &'a FinderResult {
    &outcomes
        .iter()
        .find(|outcome| outcome.finder.as_str() == finder)
        .unwrap_or_else(|| panic!("no outcome for {finder}"))
        .result
}

/// A challenge from DuckDuckGo opens its breaker at once (scraped engines
/// trip on the first refusal), the same call still returns Bing's leads,
/// and DuckDuckGo is never asked again within the call: its later step is
/// skipped as circuit-open, naming the challenge.
#[tokio::test]
async fn a_challenged_engine_opens_its_breaker_and_the_call_still_returns_the_other_engines_leads()
{
    let clock = FakeClock::new();
    let ddg = Arc::new(ScriptedFinder::failing(
        duckduckgo::DESCRIPTOR,
        FailureCause::Challenge,
    ));
    // Short pages, so the first wave is thin and the reworded wave runs.
    let bing = Arc::new(ScriptedFinder::answering(bing_html::DESCRIPTOR, 4));
    let sourcing = service(vec![entry(&ddg, 10), entry(&bing, 10)], &clock);

    let findings = sourcing
        .find(web_need(10, &["rust async runtime"]), budget(PLENTY))
        .await
        .expect("Bing answered");

    assert_eq!(
        findings.leads.len(),
        8,
        "both of Bing's queries, deduplicated"
    );
    assert!(findings.leads.iter().all(|lead| lead
        .found_by
        .iter()
        .all(|(finder, _)| finder.as_str() == "bing_html")));
    assert_eq!(
        ddg.calls().len(),
        1,
        "a challenge is never retried in-process"
    );
    match result_of(&findings.outcomes, "duckduckgo") {
        FinderResult::Skipped(SkipReason::CircuitOpen { last, .. }) => {
            assert_eq!(**last, FailureCause::Challenge)
        }
        other => panic!("DuckDuckGo should be skipped as circuit-open, got {other:?}"),
    }
    assert_eq!(
        result_of(&findings.outcomes, "bing_html"),
        &FinderResult::Answered
    );
    let ddg_health = sourcing
        .health()
        .into_iter()
        .find(|health| health.id.as_str() == "duckduckgo")
        .unwrap();
    assert!(
        matches!(&ddg_health.state, HealthState::Open { cause, .. } if *cause == FailureCause::Challenge),
        "{:?}",
        ddg_health.state
    );
}

/// The next call inside the open window does not ask DuckDuckGo at all.
#[tokio::test]
async fn an_open_breaker_skips_its_finder_on_the_next_call_without_a_request() {
    let clock = FakeClock::new();
    let ddg = Arc::new(ScriptedFinder::failing(
        duckduckgo::DESCRIPTOR,
        FailureCause::Challenge,
    ));
    let bing = Arc::new(ScriptedFinder::answering(bing_html::DESCRIPTOR, 10));
    let sourcing = service(vec![entry(&ddg, 10), entry(&bing, 10)], &clock);

    sourcing
        .find(web_need(5, &[]), budget(PLENTY))
        .await
        .unwrap();
    // The window (twice DuckDuckGo's spacing, for a first trip) is longer
    // than this caller has, so the router is told DuckDuckGo is refused.
    let second = sourcing
        .find(web_need(5, &[]), budget(Duration::from_secs(1)))
        .await
        .expect("Bing answered");

    assert_eq!(ddg.calls().len(), 1);
    assert!(matches!(
        result_of(&second.outcomes, "duckduckgo"),
        FinderResult::Skipped(SkipReason::CircuitOpen { .. })
    ));
    assert_eq!(second.leads.len(), 10);
}

/// A host that says `Retry-After: 60` is waited out, in full, when the call
/// still needs its leads and the deadline is further off than that.
#[tokio::test]
async fn retry_after_sixty_is_waited_out_when_the_deadline_allows() {
    let clock = FakeClock::new();
    let ddg = Arc::new(
        ScriptedFinder::answering(duckduckgo::DESCRIPTOR, 10).failing_first(vec![
            FailureCause::Status {
                code: 429,
                retry_after: Some(Duration::from_secs(60)),
            },
        ]),
    );
    let sourcing = service(vec![entry(&ddg, 10)], &clock);

    let findings = sourcing
        .find(web_need(10, &[]), budget(Duration::from_secs(120)))
        .await
        .expect("answered after the wait");

    assert_eq!(clock.slept(), vec![Duration::from_secs(60)]);
    assert_eq!(ddg.calls().len(), 2);
    assert_eq!(
        result_of(&findings.outcomes, "duckduckgo"),
        &FinderResult::Answered
    );
    assert_eq!(findings.leads.len(), 10);
}

/// The same instruction with only 30 seconds left: no wait, no second
/// request, the outcome names the instruction, and the finder stays held
/// until the minute is up, then is asked again.
#[tokio::test]
async fn retry_after_past_the_deadline_is_not_waited_and_holds_the_finder_until_then() {
    let clock = FakeClock::new();
    let ddg = Arc::new(
        ScriptedFinder::answering(duckduckgo::DESCRIPTOR, 10).failing_first(vec![
            FailureCause::Status {
                code: 429,
                retry_after: Some(Duration::from_secs(60)),
            },
        ]),
    );
    let bing = Arc::new(ScriptedFinder::answering(bing_html::DESCRIPTOR, 3));
    let sourcing = service(vec![entry(&ddg, 10), entry(&bing, 10)], &clock);
    let short = Duration::from_secs(30);

    let first = sourcing
        .find(web_need(10, &[]), budget(short))
        .await
        .unwrap();
    assert!(clock.slept().is_empty());
    assert_eq!(ddg.calls().len(), 1);
    assert_eq!(
        result_of(&first.outcomes, "duckduckgo"),
        &FinderResult::Failed(FailureCause::Status {
            code: 429,
            retry_after: Some(Duration::from_secs(60)),
        })
    );

    let second = sourcing
        .find(web_need(10, &[]), budget(short))
        .await
        .unwrap();
    assert_eq!(
        ddg.calls().len(),
        1,
        "held for the minute the host asked for"
    );
    assert!(matches!(
        result_of(&second.outcomes, "duckduckgo"),
        FinderResult::Skipped(SkipReason::CircuitOpen { .. })
    ));

    clock.advance(Duration::from_secs(61));
    let third = sourcing
        .find(web_need(10, &[]), budget(short))
        .await
        .unwrap();
    assert_eq!(ddg.calls().len(), 2);
    assert_eq!(
        result_of(&third.outcomes, "duckduckgo"),
        &FinderResult::Answered
    );
}

/// Transport failures are retried after the finder's own spacing until its
/// breaker trips (three in a row): the trip count is the retry bound.
#[tokio::test]
async fn transport_failures_are_retried_until_the_breaker_trips() {
    let clock = FakeClock::new();
    let cause = FailureCause::Transport("connection reset".to_string());
    let ddg = Arc::new(ScriptedFinder::failing(
        duckduckgo::DESCRIPTOR,
        cause.clone(),
    ));
    let bing = Arc::new(ScriptedFinder::answering(bing_html::DESCRIPTOR, 10));
    let sourcing = service(vec![entry(&ddg, 10), entry(&bing, 10)], &clock);

    let findings = sourcing
        .find(web_need(5, &[]), budget(PLENTY))
        .await
        .unwrap();

    assert_eq!(ddg.calls().len(), 3);
    let spacing = duckduckgo::DESCRIPTOR.rate.min_interval;
    assert_eq!(clock.slept(), vec![spacing, spacing]);
    assert_eq!(
        result_of(&findings.outcomes, "duckduckgo"),
        &FinderResult::Failed(cause)
    );
}

/// When every finder fails, the error names each one's cause.
#[tokio::test]
async fn when_every_finder_fails_the_error_names_every_cause() {
    let clock = FakeClock::new();
    let ddg = Arc::new(ScriptedFinder::failing(
        duckduckgo::DESCRIPTOR,
        FailureCause::Challenge,
    ));
    let bing = Arc::new(ScriptedFinder::failing(
        bing_html::DESCRIPTOR,
        FailureCause::Status {
            code: 403,
            retry_after: None,
        },
    ));
    let sourcing = service(vec![entry(&ddg, 10), entry(&bing, 10)], &clock);

    let error = sourcing
        .find(web_need(5, &[]), budget(PLENTY))
        .await
        .unwrap_err()
        .to_string();

    assert!(
        error.contains("duckduckgo: failed (bot challenge)"),
        "{error}"
    );
    assert!(error.contains("bing_html: failed (HTTP 403)"), "{error}");
}

/// A page that answered is reused within its TTL; a later call that needs
/// more than the pool holds asks only for the next page.
#[tokio::test]
async fn a_cached_page_costs_no_request_and_only_missing_pages_are_fetched() {
    let clock = FakeClock::new();
    let ddg = Arc::new(ScriptedFinder::answering(duckduckgo::DESCRIPTOR, 10));
    let bing = Arc::new(ScriptedFinder::answering(bing_html::DESCRIPTOR, 10));
    let sourcing = service(vec![entry(&ddg, 10), entry(&bing, 10)], &clock);

    let first = sourcing
        .find(web_need(12, &[]), budget(PLENTY))
        .await
        .unwrap();
    assert_eq!(first.leads.len(), 20);
    let second = sourcing
        .find(web_need(24, &[]), budget(PLENTY))
        .await
        .unwrap();

    let query = "rust async".to_string();
    assert_eq!(ddg.calls(), vec![(query.clone(), 1), (query.clone(), 2)]);
    assert_eq!(
        bing.calls(),
        vec![(query, 1)],
        "page 1 is never fetched twice"
    );
    assert_eq!(second.leads.len(), 30);
    assert!(!second.exhausted, "every page was full, so more may exist");
}

/// A short page means the service has no more: no further page is asked
/// for, and the findings say the need is exhausted.
#[tokio::test]
async fn a_short_page_ends_paging_and_marks_the_need_exhausted() {
    let clock = FakeClock::new();
    let ddg = Arc::new(ScriptedFinder::answering(duckduckgo::DESCRIPTOR, 4));
    let sourcing = service(vec![entry(&ddg, 10)], &clock);

    let findings = sourcing
        .find(web_need(24, &[]), budget(PLENTY))
        .await
        .unwrap();

    assert_eq!(ddg.calls().len(), 1);
    assert_eq!(findings.leads.len(), 4);
    assert!(findings.exhausted);
}

/// A search hit never becomes the subject's owner. With nothing named,
/// GitHub's repository search scopes nothing, so the owner-site step is
/// skipped without a request. Naming the repository makes its record the
/// owner, and the owner-site step then reads that site.
#[tokio::test]
async fn only_a_named_repository_scopes_the_owner_wave() {
    let package_need = |identifiers: Vec<Identifier>| SourceNeed {
        kind: NeedKind::Package,
        query: "http client".to_string(),
        subject: (!identifiers.is_empty()).then(|| SubjectHint {
            name: "reqwest".to_string(),
            kind: SubjectKind::Software,
            identifiers,
        }),
        scope: None,
        freshness: Freshness::Any,
        want: 50,
        alternatives: Vec::new(),
        also: Vec::new(),
    };
    let run = |need: SourceNeed| async move {
        let clock = FakeClock::new();
        let repos = Arc::new(
            ScriptedFinder::answering(github::DESCRIPTOR, 3)
                .asserting_owner(Origin::parse("github.com/seanmonstar/reqwest").unwrap()),
        );
        let site = Arc::new(ScriptedFinder::answering(site_discovery::DESCRIPTOR, 3));
        let sourcing = service(vec![entry(&repos, 30), entry(&site, 30)], &clock);
        let findings = sourcing.find(need, budget(PLENTY)).await.unwrap();
        (site.calls().len(), findings.outcomes)
    };

    let (site_calls, outcomes) = run(package_need(Vec::new())).await;
    assert_eq!(site_calls, 0);
    assert_eq!(
        result_of(&outcomes, "site_discovery"),
        &FinderResult::Skipped(SkipReason::NotApplicable)
    );

    let (site_calls, _) = run(package_need(vec![Identifier::Repo {
        host: "github.com".to_string(),
        owner: "seanmonstar".to_string(),
        name: "reqwest".to_string(),
    }]))
    .await;
    assert_eq!(site_calls, 1);
}
