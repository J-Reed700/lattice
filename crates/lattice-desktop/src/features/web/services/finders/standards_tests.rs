//! Unit and wiremock integration tests for all 5 standards finders.

use std::sync::Arc;
use std::time::{Duration, Instant};

use lattice_core::application::ports::{Budget, SourceFinder};
use lattice_core::domain::sourcing::{
    Auth, Cost, Destination, FinderResult, Freshness, Identifier, Identity, LeadKind, NeedKind,
    Origin, Privacy, Reach, SourceNeed, SubjectHint, SubjectKind, WaveSlot,
};
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

use super::ecma_pattern::{self, EcmaPattern};
use super::ietf_datatracker::{self, IetfDatatracker};
use super::rfc_index::{self, RfcIndex};
use super::w3c::{self, W3c};
use super::whatwg_index::{self, WhatwgIndex};
use super::FinderHttp;
use crate::features::web::services::web::SitePacer;

fn test_http() -> FinderHttp {
    FinderHttp {
        client: reqwest::Client::new(),
        pacer: Arc::new(SitePacer::with_interval(Duration::ZERO)),
        parsers: Arc::new(Semaphore::new(2)),
        contact: None,
    }
}

fn test_need(query: &str) -> SourceNeed {
    SourceNeed {
        kind: NeedKind::Standard,
        query: query.to_string(),
        subject: None,
        scope: None,
        freshness: Freshness::Any,
        want: 10,
        alternatives: Vec::new(),
        also: Vec::new(),
    }
}

fn test_budget() -> Budget {
    Budget {
        deadline: Instant::now() + Duration::from_secs(60),
        stop: CancellationToken::new(),
        requests: None,
    }
}

#[tokio::test]
async fn rfc_index_resolves_rfc_9110_wiremock_and_404_fallback() {
    let server = MockServer::start().await;
    let json =
        r#"{"title":"HTTP Semantics","abstract":"HTTP protocol semantics","doc_id":"RFC9110"}"#;

    Mock::given(method("GET"))
        .and(path("/rfc/rfc9110.json"))
        .respond_with(ResponseTemplate::new(200).set_body_string(json))
        .expect(1)
        .mount(&server)
        .await;

    let finder = RfcIndex::with_api(test_http(), server.uri());
    let outcome = finder.find(&test_need("RFC 9110"), 1, &test_budget()).await;
    assert_eq!(outcome.result, FinderResult::Answered);
    assert_eq!(outcome.leads.len(), 1);

    let lead = &outcome.leads[0];
    assert_eq!(
        lead.url.as_str(),
        "https://www.rfc-editor.org/rfc/rfc9110.html"
    );
    assert_eq!(lead.kind, LeadKind::Document(Identifier::Rfc(9110)));
    assert_eq!(lead.owner, Some(Origin::new("rfc-editor.org", None)));
    assert_eq!(lead.title.as_deref(), Some("HTTP Semantics"));
    assert!(lead.snippet.is_some());

    // Fallback on 404 with subject hint
    let server404 = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/rfc/rfc9110.json"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server404)
        .await;

    let finder404 = RfcIndex::with_api(test_http(), server404.uri());
    let mut need_hint = test_need("http");
    need_hint.subject = Some(SubjectHint {
        name: "RFC 9110".into(),
        kind: SubjectKind::Standard,
        identifiers: vec![Identifier::Rfc(9110)],
    });
    let outcome404 = finder404.find(&need_hint, 1, &test_budget()).await;
    assert_eq!(outcome404.result, FinderResult::Answered);
    assert_eq!(
        outcome404.leads[0].url.as_str(),
        "https://www.rfc-editor.org/rfc/rfc9110.html"
    );
    assert_eq!(
        outcome404.leads[0].kind,
        LeadKind::Document(Identifier::Rfc(9110))
    );
    assert_eq!(
        outcome404.leads[0].owner,
        Some(Origin::new("rfc-editor.org", None))
    );
}

#[tokio::test]
async fn ietf_datatracker_finds_rfcs_and_drafts() {
    let server = MockServer::start().await;
    let json = r#"{"objects":[
        {"name":"rfc9000","title":"QUIC Transport","rfc":9000,"abstract":"QUIC spec"},
        {"name":"draft-ietf-quic-transport-34","title":"QUIC Transport","rfc":null,"abstract":"Draft"}
    ]}"#;

    Mock::given(method("GET"))
        .and(path("/api/v1/doc/document/"))
        .and(query_param("title__icontains", "quic"))
        .respond_with(ResponseTemplate::new(200).set_body_string(json))
        .expect(1)
        .mount(&server)
        .await;

    let finder = IetfDatatracker::with_api(test_http(), server.uri());
    let outcome = finder.find(&test_need("quic"), 1, &test_budget()).await;
    assert_eq!(outcome.result, FinderResult::Answered);
    assert_eq!(outcome.leads.len(), 2);

    assert_eq!(
        outcome.leads[0].url.as_str(),
        "https://www.rfc-editor.org/rfc/rfc9000.html"
    );
    assert_eq!(
        outcome.leads[0].kind,
        LeadKind::Document(Identifier::Rfc(9000))
    );
    assert_eq!(
        outcome.leads[0].owner,
        Some(Origin::new("rfc-editor.org", None))
    );

    assert_eq!(
        outcome.leads[1].url.as_str(),
        "https://datatracker.ietf.org/doc/draft-ietf-quic-transport-34/"
    );
    assert_eq!(outcome.leads[1].kind, LeadKind::Page);
    assert_eq!(outcome.leads[1].owner, Some(Origin::new("ietf.org", None)));
}

#[tokio::test]
async fn w3c_finds_and_scores_specifications() {
    let server = MockServer::start().await;
    let json = r#"{"_embedded":{"specifications":[
        {"title":"WebAssembly Core Specification","shortname":"wasm-core-1","shortlink":"https://www.w3.org/TR/wasm-core-1/"},
        {"title":"Indexed Database API 3.0","shortname":"IndexedDB-3","shortlink":"https://www.w3.org/TR/IndexedDB-3/"}
    ]}}"#;

    Mock::given(method("GET"))
        .and(path("/specifications"))
        .and(query_param("items", "100"))
        .and(query_param("page", "1"))
        .respond_with(ResponseTemplate::new(200).set_body_string(json))
        .expect(1)
        .mount(&server)
        .await;

    let finder = W3c::with_api(test_http(), server.uri());
    let outcome = finder
        .find(&test_need("webassembly"), 1, &test_budget())
        .await;
    assert_eq!(outcome.result, FinderResult::Answered);
    assert_eq!(outcome.leads.len(), 1);
    assert_eq!(
        outcome.leads[0].url.as_str(),
        "https://www.w3.org/TR/wasm-core-1/"
    );
    assert_eq!(outcome.leads[0].owner, Some(Origin::new("w3.org", None)));
}

#[tokio::test]
async fn whatwg_index_matches_living_standards() {
    let finder = WhatwgIndex::new(test_http());

    let fetch_out = finder.find(&test_need("fetch"), 1, &test_budget()).await;
    assert_eq!(fetch_out.result, FinderResult::Answered);
    assert_eq!(
        fetch_out.leads[0].url.as_str(),
        "https://fetch.spec.whatwg.org/"
    );
    assert_eq!(fetch_out.leads[0].kind, LeadKind::OfficialSite);
    assert_eq!(
        fetch_out.leads[0].owner,
        Some(Origin::new("spec.whatwg.org", None))
    );

    let dom_out = finder.find(&test_need("dom"), 1, &test_budget()).await;
    assert_eq!(dom_out.result, FinderResult::Answered);
    assert_eq!(
        dom_out.leads[0].url.as_str(),
        "https://dom.spec.whatwg.org/"
    );

    let empty_out = finder
        .find(&test_need("unknown_random_query"), 1, &test_budget())
        .await;
    assert_eq!(empty_out.result, FinderResult::Empty);
}

#[tokio::test]
async fn ecma_pattern_matches_standards() {
    let finder = EcmaPattern::new(test_http());

    let out_262 = finder.find(&test_need("ECMA-262"), 1, &test_budget()).await;
    assert_eq!(out_262.result, FinderResult::Answered);
    assert_eq!(out_262.leads[0].url.as_str(), "https://tc39.es/ecma262/");
    assert_eq!(out_262.leads[0].owner, Some(Origin::new("tc39.es", None)));

    let out_json = finder.find(&test_need("JSON"), 1, &test_budget()).await;
    assert_eq!(out_json.result, FinderResult::Answered);
    assert_eq!(
        out_json.leads[0].url.as_str(),
        "https://ecma-international.org/publications-and-standards/standards/ecma-404/"
    );
    assert_eq!(
        out_json.leads[0].owner,
        Some(Origin::new("ecma-international.org", None))
    );

    let out_cs = finder.find(&test_need("C#"), 1, &test_budget()).await;
    assert_eq!(out_cs.result, FinderResult::Answered);
    assert_eq!(
        out_cs.leads[0].url.as_str(),
        "https://ecma-international.org/publications-and-standards/standards/ecma-334/"
    );

    let out_custom = finder
        .find(&test_need("ECMA-1234"), 1, &test_budget())
        .await;
    assert_eq!(out_custom.result, FinderResult::Answered);
    assert_eq!(
        out_custom.leads[0].url.as_str(),
        "https://ecma-international.org/publications-and-standards/standards/ecma-1234/"
    );
}

#[test]
fn all_standards_descriptors_are_valid() {
    let descriptors = [
        &rfc_index::DESCRIPTOR,
        &ietf_datatracker::DESCRIPTOR,
        &w3c::DESCRIPTOR,
        &whatwg_index::DESCRIPTOR,
        &ecma_pattern::DESCRIPTOR,
    ];

    for desc in descriptors {
        assert_eq!(desc.cost, Cost::Free);
        assert_eq!(desc.auth, Auth::None);
        assert_eq!(desc.reaches, Reach::Standards);
        assert_eq!(desc.identity, Identity::DeclaredClient);
        assert!(!desc.supports_scope);
        assert_eq!(desc.slot_for(NeedKind::Standard), Some(WaveSlot::First));
    }

    assert_eq!(rfc_index::DESCRIPTOR.id.as_str(), "rfc_index");
    assert_eq!(ietf_datatracker::DESCRIPTOR.id.as_str(), "ietf_datatracker");
    assert_eq!(w3c::DESCRIPTOR.id.as_str(), "w3c");
    assert_eq!(whatwg_index::DESCRIPTOR.id.as_str(), "whatwg_index");
    assert_eq!(ecma_pattern::DESCRIPTOR.id.as_str(), "ecma_pattern");

    assert_eq!(whatwg_index::DESCRIPTOR.privacy, Privacy::AnonymousQuery);
    assert_eq!(whatwg_index::DESCRIPTOR.sends_to, Destination::Local);
    assert_eq!(ecma_pattern::DESCRIPTOR.privacy, Privacy::AnonymousQuery);
    assert_eq!(ecma_pattern::DESCRIPTOR.sends_to, Destination::Local);
}
