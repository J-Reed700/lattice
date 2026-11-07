//! The Wikibooks finder against a fixture server, its place in the
//! registry, and the Wikipedia finder's request kept as it is.

use std::sync::Arc;
use std::time::{Duration, Instant};

use lattice_core::application::ports::{Budget, SourceFinder};
use lattice_core::domain::sourcing::{
    Auth, Cost, Destination, FailureCause, FinderResult, Freshness, Identity, NeedKind, Privacy,
    SourceNeed, WaveSlot, FINDER_IDS,
};
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;
use wiremock::matchers::{header_regex, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::*;
use crate::features::web::services::web::SitePacer;

fn http() -> FinderHttp {
    FinderHttp {
        client: reqwest::Client::new(),
        pacer: Arc::new(SitePacer::with_interval(Duration::ZERO)),
        parsers: Arc::new(Semaphore::new(2)),
        contact: None,
        public_only: false,
    }
}

fn need(query: &str) -> SourceNeed {
    SourceNeed {
        kind: NeedKind::Web,
        query: query.to_string(),
        subject: None,
        scope: None,
        freshness: Freshness::Any,
        want: 10,
        alternatives: Vec::new(),
        also: Vec::new(),
    }
}

fn budget() -> Budget {
    Budget {
        deadline: Instant::now() + Duration::from_secs(60),
        stop: CancellationToken::new(),
        requests: None,
    }
}

fn finder(server: &MockServer) -> wikibooks::Wikibooks {
    wikibooks::Wikibooks::with_api(http(), format!("{}/w/api.php", server.uri()))
}

#[tokio::test]
async fn a_wikibooks_search_is_asked_as_a_declared_client_and_each_result_becomes_a_lead() {
    let server = MockServer::start().await;
    Mock::given(path("/w/api.php"))
        .and(header_regex("user-agent", r"^Lattice/\S+ \(\+https://"))
        .and(header_regex("accept", "application/json"))
        .and(query_param("action", "query"))
        .and(query_param("format", "json"))
        .and(query_param("list", "search"))
        .and(query_param("srprop", "snippet"))
        .and(query_param("srsearch", "b-tree index"))
        .and(query_param(
            "srlimit",
            wikibooks::RESULTS_PER_PAGE.to_string(),
        ))
        .and(query_param("sroffset", "0"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "query": { "search": [
                { "title": "PostgreSQL/Indexes", "snippet": "A <span class=\"searchmatch\">b-tree</span>  is\nthe default" },
                { "title": "Music Theory/Circle of Fifths", "snippet": "" },
                { "title": "Cooking/Café", "snippet": "Coffee" }
            ]}
        })))
        .expect(1)
        .mount(&server)
        .await;

    let outcome = finder(&server)
        .find(&need("b-tree index"), 1, &budget())
        .await;

    assert_eq!(outcome.result, FinderResult::Answered);
    let urls: Vec<&str> = outcome.leads.iter().map(|lead| lead.url.as_str()).collect();
    assert_eq!(
        urls,
        [
            "https://en.wikibooks.org/wiki/PostgreSQL/Indexes",
            "https://en.wikibooks.org/wiki/Music_Theory/Circle_of_Fifths",
            "https://en.wikibooks.org/wiki/Cooking/Caf%C3%A9"
        ]
    );
    let titles: Vec<Option<&str>> = outcome
        .leads
        .iter()
        .map(|lead| lead.title.as_deref())
        .collect();
    assert_eq!(
        titles,
        [
            Some("PostgreSQL/Indexes"),
            Some("Music Theory/Circle of Fifths"),
            Some("Cooking/Café")
        ]
    );
    assert_eq!(
        outcome.leads[0].snippet.as_deref(),
        Some("A b-tree is the default")
    );
    assert_eq!(outcome.leads[1].snippet, None);
    let ranks: Vec<usize> = outcome
        .leads
        .iter()
        .map(|lead| lead.found_by[0].1)
        .collect();
    assert_eq!(ranks, [1, 2, 3]);
    assert!(outcome
        .leads
        .iter()
        .all(|lead| lead.found_by[0].0 == wikibooks::ID));
}

#[tokio::test]
async fn a_second_wikibooks_page_is_asked_by_offset_and_ranked_after_the_first() {
    let server = MockServer::start().await;
    let offset = wikibooks::RESULTS_PER_PAGE.to_string();
    Mock::given(path("/w/api.php"))
        .and(query_param("sroffset", offset.as_str()))
        .and(query_param("srsearch", "chords"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "query": { "search": [
                { "title": "Music Theory/Chords", "snippet": "" },
                { "title": "Music Theory/Scales", "snippet": "" }
            ]}
        })))
        .expect(1)
        .mount(&server)
        .await;

    let outcome = finder(&server).find(&need("chords"), 2, &budget()).await;

    assert_eq!(outcome.result, FinderResult::Answered);
    let ranks: Vec<usize> = outcome
        .leads
        .iter()
        .map(|lead| lead.found_by[0].1)
        .collect();
    assert_eq!(
        ranks,
        [
            wikibooks::RESULTS_PER_PAGE + 1,
            wikibooks::RESULTS_PER_PAGE + 2
        ]
    );
}

/// The API's own rate-limit error, delivered in a 200, is a 429, as the
/// Wikipedia finder reads it.
#[tokio::test]
async fn wikibooks_ratelimited_error_is_a_refusal() {
    let server = MockServer::start().await;
    Mock::given(path("/w/api.php"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "error": { "code": "ratelimited", "info": "slow down" }
        })))
        .mount(&server)
        .await;

    let outcome = finder(&server).find(&need("chords"), 1, &budget()).await;

    assert_eq!(
        outcome.result,
        FinderResult::Failed(FailureCause::Status {
            code: 429,
            retry_after: None,
        })
    );
    assert!(outcome.leads.is_empty());
}

/// A page shorter than the registered page size is how the runner learns the
/// service has no more, so the size must be the one the search asks for.
#[test]
fn wikibooks_is_registered_keyless_on_by_default_and_paged_as_it_asks() {
    let registry = keyless_registry(&http());
    let entry = registry
        .iter()
        .find(|entry| entry.descriptor().id.as_str() == "wikibooks")
        .expect("wikibooks is registered");
    let descriptor = entry.descriptor();
    assert_eq!(descriptor.id, wikibooks::ID);
    assert_eq!(descriptor.auth, Auth::None);
    assert_eq!(descriptor.cost, Cost::Free);
    assert_eq!(descriptor.identity, Identity::DeclaredClient);
    assert_eq!(descriptor.sends_to, Destination::Host("en.wikibooks.org"));
    assert!(matches!(descriptor.privacy, Privacy::QueryLogged { .. }));
    assert!(!descriptor.rate.trip_on_first_refusal);
    assert!(descriptor.rate.honours_retry_after);
    assert_eq!(descriptor.slot_for(NeedKind::Web), Some(WaveSlot::First));
    assert_eq!(entry.page_size, wikibooks::RESULTS_PER_PAGE);
    assert!(FINDER_IDS.contains(&"wikibooks"));
}

/// Guards the Wikipedia finder's request while a sibling is written beside it.
#[tokio::test]
async fn the_wikipedia_request_is_unchanged() {
    let server = MockServer::start().await;
    Mock::given(path("/w/api.php"))
        .and(header_regex("user-agent", r"^Lattice/\S+ \(\+https://"))
        .and(header_regex("accept", "application/json"))
        .and(query_param("action", "query"))
        .and(query_param("format", "json"))
        .and(query_param("utf8", "1"))
        .and(query_param("list", "search"))
        .and(query_param("srprop", "snippet"))
        .and(query_param("srsearch", "b-tree index"))
        .and(query_param("srlimit", "10"))
        .and(query_param("sroffset", "10"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "query": { "search": [ { "title": "B-tree", "snippet": "" } ] }
        })))
        .expect(1)
        .mount(&server)
        .await;
    let finder = wikipedia::Wikipedia::with_api(http(), format!("{}/w/api.php", server.uri()));

    let outcome = finder.find(&need("b-tree index"), 2, &budget()).await;

    assert_eq!(outcome.result, FinderResult::Answered);
    assert_eq!(
        outcome.leads[0].url.as_str(),
        "https://en.wikipedia.org/wiki/B-tree"
    );
    assert_eq!(wikipedia::RESULTS_PER_PAGE, 10);
}
