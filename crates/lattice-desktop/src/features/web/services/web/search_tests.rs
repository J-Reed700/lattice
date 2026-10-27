//! `search_web` over scripted finders: the tool's output carries every
//! finder's outcome, and a later page is a slice of the pool.

use std::collections::BTreeSet;
use std::sync::Arc;

use lattice_core::application::contracts::settings::SourcingSettingsDto;
use lattice_core::application::contracts::web::{FinderOutcomeDto, WebSearchInput};
use lattice_core::application::ports::{SourceFinder, WebServiceTrait};
use lattice_core::domain::sourcing::FailureCause;

use super::super::WebService;
use crate::features::web::mocks::web::ScriptedFinder;
use crate::features::web::services::finders::{bing_html, duckduckgo, FinderEntry};
use crate::features::web::services::sourcing::SourcingService;

fn web_service(finders: &[(&Arc<ScriptedFinder>, usize)]) -> WebService {
    let entries = finders
        .iter()
        .map(|(finder, page_size)| FinderEntry {
            finder: Arc::clone(finder) as Arc<dyn SourceFinder>,
            page_size: *page_size,
        })
        .collect();
    let sourcing = SourcingService::new(entries, SourcingSettingsDto::default(), BTreeSet::new());
    WebService::with_sourcing(
        &std::env::temp_dir().join("lattice-web-search-tests"),
        Arc::new(sourcing),
    )
    .unwrap()
}

fn input(query: &str, max_results: usize, page: usize) -> WebSearchInput {
    WebSearchInput {
        query: query.to_string(),
        kind: None,
        subject: None,
        identifier: None,
        ecosystem: None,
        site: None,
        after: None,
        as_of: None,
        max_results,
        page,
        offset: 0,
        providers: vec!["duckduckgo".to_string(), "bing".to_string()],
        include_wikipedia: false,
        depth: 1,
        branch_queries: 1,
        followup_queries: Vec::new(),
        alternatives: Vec::new(),
    }
}

/// The model and the step line see that DuckDuckGo was challenged, not a
/// silently shorter list.
#[tokio::test]
async fn web_search_returns_every_finders_outcome() {
    let ddg = Arc::new(ScriptedFinder::failing(
        duckduckgo::DESCRIPTOR,
        FailureCause::Challenge,
    ));
    let bing = Arc::new(ScriptedFinder::answering(bing_html::DESCRIPTOR, 10));
    let service = web_service(&[(&ddg, 10), (&bing, 10)]);

    let output = service
        .search_web(&input("rust async", 5, 1))
        .await
        .unwrap();

    assert_eq!(
        output.outcomes,
        vec![
            FinderOutcomeDto {
                finder: "duckduckgo".into(),
                result: "failed".into(),
                leads: 0,
                elapsed_ms: 0,
                cause: Some("bot challenge".into()),
            },
            FinderOutcomeDto {
                finder: "bing_html".into(),
                result: "answered".into(),
                leads: 10,
                elapsed_ms: 0,
                cause: None,
            },
        ]
    );
    assert_eq!(output.providers_used, ["bing_html"]);
    assert_eq!(output.results.len(), 5);
    let json = serde_json::to_value(&output).unwrap();
    assert_eq!(json["outcomes"][0]["result"], "failed");

    // Inside the breaker's window the next search skips DuckDuckGo and says so.
    let again = service
        .search_web(&input("rust async", 5, 1))
        .await
        .unwrap();
    // DuckDuckGo's window fits the search's budget, so it is held behind
    // Bing and settles after it: find its outcome by name, not position.
    assert_eq!(ddg.calls().len(), 1);
    let skipped = again
        .outcomes
        .iter()
        .find(|outcome| outcome.finder == "duckduckgo")
        .expect("DuckDuckGo's outcome is reported");
    assert_eq!(skipped.result, "skipped");
    assert!(
        skipped
            .cause
            .as_deref()
            .is_some_and(|cause| cause.contains("bot challenge")),
        "{skipped:?}"
    );
}

/// When the pool already holds page 2 of a search, no provider is asked
/// anything.
#[tokio::test]
async fn page_two_of_a_twelve_result_search_makes_no_provider_request_when_the_pool_holds_it() {
    let ddg = Arc::new(ScriptedFinder::answering(duckduckgo::DESCRIPTOR, 15));
    let bing = Arc::new(ScriptedFinder::answering(bing_html::DESCRIPTOR, 15));
    let service = web_service(&[(&ddg, 15), (&bing, 15)]);

    let first = service
        .search_web(&input("rust async", 12, 1))
        .await
        .unwrap();
    assert_eq!((ddg.calls().len(), bing.calls().len()), (1, 1));
    let second = service
        .search_web(&input("rust async", 12, 2))
        .await
        .unwrap();

    assert_eq!(
        (ddg.calls().len(), bing.calls().len()),
        (1, 1),
        "page 2 is a slice of the pool"
    );
    assert_eq!(second.results.len(), 12);
    assert_eq!(second.offset, 12);
    assert_eq!(second.total_results, 30);
    let first_urls: Vec<&str> = first
        .results
        .iter()
        .map(|result| result.url.as_str())
        .collect();
    assert!(second
        .results
        .iter()
        .all(|result| !first_urls.contains(&result.url.as_str())));
}

/// A search that names no engine is the router's to place, so the user's
/// keyed engines and SearXNG are reached; naming engines still pins them.
#[test]
fn only_named_engines_pin_a_search() {
    let plain = WebSearchInput {
        providers: Vec::new(),
        ..input("rust async", 5, 1)
    };
    let (need, only) = super::web_need(&plain, "rust async", 5);
    assert_eq!(need.kind, lattice_core::domain::sourcing::NeedKind::Web);
    assert_eq!(only, None);

    let typed = WebSearchInput {
        kind: Some("paper".into()),
        ..plain.clone()
    };
    assert_eq!(super::web_need(&typed, "attention", 5).1, None);

    let pinned = input("rust async", 5, 1);
    assert_eq!(
        super::web_need(&pinned, "rust async", 5).1,
        Some(vec![duckduckgo::ID, bing_html::ID])
    );
}

/// A dated read wins over a newer-than date; without one, `after` is kept.
#[test]
fn a_dated_read_routes_to_the_archive_and_after_stays_a_freshness_bound() {
    use chrono::NaiveDate;
    use lattice_core::application::services::sourcing::routed_kind;
    use lattice_core::domain::sourcing::{Freshness, NeedKind};
    let day = |text: &str| NaiveDate::parse_from_str(text, "%Y-%m-%d").unwrap();
    let dated = WebSearchInput {
        kind: Some("official_docs".into()),
        identifier: Some("https://www.python.org/downloads/".into()),
        after: Some("2018-01-01".into()),
        as_of: Some("2019-03-01".into()),
        ..input("python downloads", 5, 1)
    };
    let (need, _) = super::web_need(&dated, "python downloads", 5);
    assert_eq!(need.freshness, Freshness::AsOf(day("2019-03-01")));
    assert_eq!(routed_kind(&need), NeedKind::Archive);

    let newer = WebSearchInput {
        as_of: None,
        alternatives: vec!["CPython latest".into()],
        ..dated
    };
    let (need, _) = super::web_need(&newer, "python downloads", 5);
    assert_eq!(need.freshness, Freshness::After(day("2018-01-01")));
    assert_eq!(need.alternatives, ["CPython latest"]);
}

/// A `leads` search for a paper's references goes to the finders that read
/// a work's reference list, never to the general engines. Its query is the
/// DOI, so the encyclopedia's article lookup (wave 1) finds no article and
/// the reference-list finders (wave 2) run.
#[tokio::test]
async fn a_leads_need_is_routed_to_the_reference_list_finders() {
    use crate::features::web::services::finders::{crossref, openalex, wikipedia};
    let ddg = Arc::new(ScriptedFinder::answering(duckduckgo::DESCRIPTOR, 10));
    let wiki = Arc::new(ScriptedFinder::answering(wikipedia::DESCRIPTOR, 0));
    let cross = Arc::new(ScriptedFinder::answering(crossref::DESCRIPTOR, 3));
    let alex = Arc::new(ScriptedFinder::answering(openalex::DESCRIPTOR, 3));
    let service = web_service(&[(&ddg, 10), (&wiki, 0), (&cross, 3), (&alex, 3)]);
    let leads = WebSearchInput {
        kind: Some("leads".into()),
        identifier: Some("10.48550/arXiv.1706.03762".into()),
        providers: Vec::new(),
        ..input("10.48550/arXiv.1706.03762", 12, 1)
    };
    let output = service.search_web(&leads).await.unwrap();
    let routed: Vec<&str> = output
        .outcomes
        .iter()
        .map(|outcome| outcome.finder.as_str())
        .collect();
    for finder in ["wikipedia", "crossref", "openalex"] {
        assert!(routed.contains(&finder), "{finder} not routed: {routed:?}");
    }
    assert!(
        ddg.calls().is_empty(),
        "no general engine reads a reference list"
    );
}
