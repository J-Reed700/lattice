//! Planned searches are typed needs: each states its kind, takes a subject
//! and a newer-than date only from the gap it serves, keeps to a host only
//! when the planner set one, and reaches the source finder with every field
//! it was planned with.

use std::sync::Mutex;

use lattice_core::application::contracts::web::{
    FetchUrlContentOutput, ResolvedOwnerDto, WebSearchInput, WebSearchOutput, WebSearchResult,
};
use lattice_core::application::ports::WebServiceTrait;
use lattice_core::domain::sourcing::NeedKind;
use lattice_core::shared::error::Result;
use serde_json::json;

use super::web_research::{need_schema, plan_needs, search_need, GapNeed, PlannedNeed};
use crate::features::learning::lessons::course_generation_tests::ScriptedModel;
use crate::features::web::mocks::MockWebService;

fn scripted(outputs: Vec<String>) -> ScriptedModel {
    ScriptedModel {
        outputs: Mutex::new(outputs.into()),
        prompts: Mutex::new(Vec::new()),
    }
}

fn need(query: &str, kind: NeedKind) -> PlannedNeed {
    PlannedNeed {
        kind,
        ..PlannedNeed::web(query.into())
    }
}

#[test]
fn a_need_is_checked_against_what_a_search_allows() {
    let trimmed = PlannedNeed {
        subject: Some("  ".into()),
        ecosystem: Some(" pypi ".into()),
        ..need("  requests timeout  ", NeedKind::Package)
    }
    .checked()
    .unwrap();
    assert_eq!(trimmed.query, "requests timeout");
    assert_eq!(trimmed.subject, None);
    assert_eq!(trimmed.ecosystem.as_deref(), Some("pypi"));

    assert_eq!(need("ab", NeedKind::Web).checked(), None, "too short");
    assert_eq!(
        need("an archived page", NeedKind::Archive).checked(),
        None,
        "an archived copy starts from an address"
    );
    let unknown_registry = PlannedNeed {
        ecosystem: Some("cpan".into()),
        ..need("Moose", NeedKind::Package)
    };
    assert_eq!(unknown_registry.checked(), None);
}

#[test]
fn rewordings_and_dates_are_checked_like_the_query() {
    let page = "https://docs.example.org/2019/limits";
    let dated = PlannedNeed {
        identifier: Some(format!(" {page} ")),
        alternatives: vec![" request size limit ".into()],
        as_of: Some(" 2019-03-01 ".into()),
        ..need("upload limit in 2019", NeedKind::Web)
    }
    .checked()
    .unwrap();
    assert_eq!(dated.identifier.as_deref(), Some(page));
    assert_eq!(dated.alternatives, ["request size limit"]);
    assert_eq!(dated.as_of.as_deref(), Some("2019-03-01"));

    let refused = |need: PlannedNeed| need.checked().is_none();
    let base = || need("upload limit", NeedKind::Web);
    assert!(refused(PlannedNeed {
        alternatives: vec!["ok rewording".into(), "ab".into()],
        ..base()
    }));
    assert!(refused(PlannedNeed {
        after: Some("March 2019".into()),
        ..base()
    }));
    assert!(refused(PlannedNeed {
        after: Some("2019-02-30".into()),
        ..base()
    }));
    // A dated read is of one page: it names the address, and it is not
    // also a search for newer material.
    assert!(refused(PlannedNeed {
        as_of: Some("2019-03-01".into()),
        ..base()
    }));
    assert!(refused(PlannedNeed {
        as_of: Some("2019-03-01".into()),
        identifier: Some("doi:10.1000/182".into()),
        ..base()
    }));
    assert!(refused(PlannedNeed {
        as_of: Some("2019-03-01".into()),
        after: Some("2018-01-01".into()),
        identifier: Some(page.into()),
        ..base()
    }));
    assert!(!refused(PlannedNeed {
        after: Some("2024-06-01".into()),
        ..base()
    }));
}

#[test]
fn a_known_works_cited_sources_can_be_planned() {
    let references = PlannedNeed {
        identifier: Some("10.48550/arXiv.1706.03762".into()),
        ..need("10.48550/arXiv.1706.03762", NeedKind::Leads)
    };
    assert_eq!(references.clone().checked(), Some(references));
    let article = need("Transformer (deep learning architecture)", NeedKind::Leads);
    assert!(article.checked().is_some());
}

#[test]
fn only_a_plan_for_gaps_names_the_gap_each_search_serves() {
    let course = need_schema(None);
    assert_eq!(course["required"], json!(["query", "kind"]));
    assert!(course["properties"].get("gap").is_none());

    let lesson = need_schema(Some(3));
    assert_eq!(lesson["required"], json!(["query", "kind", "gap"]));
    assert_eq!(lesson["properties"]["gap"]["maximum"], json!(2));
    let kinds = lesson["properties"]["kind"]["enum"].as_array().unwrap();
    assert!(kinds.contains(&json!("official_docs")));
    assert!(kinds.contains(&json!("leads")));
    assert!(!kinds.contains(&json!("archive")));
}

#[test]
fn every_planner_may_ask_for_rewordings_and_dates() {
    for schema in [need_schema(None), need_schema(Some(2))] {
        let properties = &schema["properties"];
        let rewording = &properties["alternatives"]["items"];
        assert_eq!(rewording["minLength"], json!(3));
        assert_eq!(rewording["maxLength"], json!(240));
        assert!(
            properties["alternatives"].get("maxItems").is_none(),
            "no fixed count of rewordings"
        );
        assert_eq!(properties["after"]["format"], "date");
        assert_eq!(properties["asOf"]["format"], "date");
    }
}

#[tokio::test]
async fn a_need_takes_its_subject_from_the_gap_it_serves_and_no_host_it_was_not_given() -> Result<()>
{
    let gaps = [
        json!({"claim": "ExecutionContext flows across await points", "missingEvidence": "official docs",
               "kind": "specification", "subject": ".NET", "ownerOriginHints": ["learn.microsoft.com"],
               "needsIndependentSource": true}),
        json!({"claim": "Self-attention cost grows with the square of the sequence length",
               "missingEvidence": "a paper", "kind": "measurement", "subject": "Transformer"}),
    ];
    let llm = scripted(vec![json!({"needs": [
        {"gap": 0, "query": "ExecutionContext flow across await", "kind": "official_docs", "site": "learn.microsoft.com"},
        {"gap": 0, "query": "ExecutionContext AsyncLocal flow analysis", "kind": "web"},
        {"gap": 1, "query": "self-attention quadratic sequence length", "kind": "paper", "subject": "attention"}
    ]})
    .to_string()]);
    let needs = plan_needs(&llm, "async .NET", &gaps, json!([]), &[]).await?;
    let served: Vec<usize> = needs.iter().map(|served| served.gap).collect();
    assert_eq!(served, [0, 0, 1], "each search keeps the gap it serves");
    let routed: Vec<_> = needs
        .iter()
        .map(|GapNeed { need, .. }| (need.kind, need.subject.as_deref(), need.site.as_deref()))
        .collect();
    assert_eq!(
        routed,
        [
            (
                NeedKind::OfficialDocs,
                Some(".NET"),
                Some("learn.microsoft.com")
            ),
            // The independent search keeps no host: owner hints are the
            // planner's to use, never a filter added behind it.
            (NeedKind::Web, Some(".NET"), None),
            // The planner's own subject wins over the gap's.
            (NeedKind::Paper, Some("attention"), None),
        ]
    );
    Ok(())
}

#[tokio::test]
async fn a_gap_asking_for_newer_material_dates_the_searches_that_name_no_date() -> Result<()> {
    let gaps = [
        json!({"claim": "The current stable Python is 3.13.", "missingEvidence": "stale",
                       "needsNewerThan": "2024-10-07"}),
    ];
    let llm = scripted(vec![json!({"needs": [
        {"gap": 0, "query": "Python stable release", "kind": "official_docs"},
        {"gap": 0, "query": "Python release schedule", "kind": "web", "after": "2025-01-01"},
        {"gap": 0, "query": "python.org downloads page", "kind": "web",
         "identifier": "https://www.python.org/downloads/", "asOf": "2024-10-07"}
    ]})
    .to_string()]);
    let needs = plan_needs(&llm, "Python", &gaps, json!([]), &[]).await?;
    let dates: Vec<_> = needs
        .iter()
        .map(|GapNeed { need, .. }| (need.after.as_deref(), need.as_of.as_deref()))
        .collect();
    assert_eq!(
        dates,
        [
            (Some("2024-10-07"), None),
            // The planner's own date wins, and a dated read stays one.
            (Some("2025-01-01"), None),
            (None, Some("2024-10-07")),
        ]
    );
    Ok(())
}

/// A web service that records every search and says which owner it
/// resolved, as the source finder does.
struct Recording {
    web: MockWebService,
    searches: Mutex<Vec<WebSearchInput>>,
}

#[async_trait::async_trait]
impl WebServiceTrait for Recording {
    async fn search_web(&self, input: &WebSearchInput) -> Result<WebSearchOutput> {
        self.searches.lock().unwrap().push(input.clone());
        let mut output = self.web.search_web(input).await?;
        output.owner = Some(ResolvedOwnerDto {
            domain: "docs.python.org".into(),
            resolved_by: Some("wikidata".into()),
        });
        Ok(output)
    }
    async fn fetch_url_content(&self, url: &str) -> Result<FetchUrlContentOutput> {
        self.web.fetch_url_content(url).await
    }
    fn validate_url(&self, url: &str) -> Result<()> {
        self.web.validate_url(url)
    }
}

#[tokio::test]
async fn a_search_reaches_the_finder_with_every_planned_field() -> Result<()> {
    let web = Recording {
        web: MockWebService::new(),
        searches: Mutex::new(Vec::new()),
    };
    web.web.set_search_results(vec![WebSearchResult {
        title: "What's new".into(),
        url: "https://docs.python.org/3/whatsnew/".into(),
        snippet: String::new(),
        published_date: None,
        source: Some("site_discovery".into()),
    }]);
    let newer = PlannedNeed {
        subject: Some("Python".into()),
        alternatives: vec!["CPython latest version".into()],
        after: Some("2024-10-07".into()),
        ..need("Python stable release", NeedKind::OfficialDocs)
    };
    let found = search_need(&web, &newer).await?;
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].kind, NeedKind::OfficialDocs);
    assert_eq!(
        found[0].owner.as_ref().map(|owner| owner.domain.as_str()),
        Some("docs.python.org")
    );
    let dated = PlannedNeed {
        identifier: Some("https://www.python.org/downloads/".into()),
        as_of: Some("2019-03-01".into()),
        ..need("python.org downloads page", NeedKind::Web)
    };
    search_need(&web, &dated).await?;

    let searches = web.searches.lock().unwrap();
    let first = &searches[0];
    assert_eq!(first.kind.as_deref(), Some("official_docs"));
    assert_eq!(first.alternatives, ["CPython latest version"]);
    assert_eq!(first.after.as_deref(), Some("2024-10-07"));
    assert_eq!(first.as_of, None);
    let last = searches.last().unwrap();
    assert_eq!(last.as_of.as_deref(), Some("2019-03-01"));
    assert_eq!(
        last.identifier.as_deref(),
        Some("https://www.python.org/downloads/")
    );
    Ok(())
}

#[tokio::test]
async fn a_search_for_a_gap_the_planner_was_not_given_is_refused() {
    let gaps = [json!({"claim": "c", "missingEvidence": "m"})];
    let llm = scripted(vec![json!({"needs": [
        {"gap": 1, "query": "some public search", "kind": "web"}
    ]})
    .to_string()]);
    assert!(plan_needs(&llm, "topic", &gaps, json!([]), &[])
        .await
        .is_err());
}

#[tokio::test]
async fn no_gaps_plan_no_searches_without_asking_the_model() -> Result<()> {
    let llm = scripted(Vec::new());
    assert!(plan_needs(&llm, "topic", &[], json!([]), &[])
        .await?
        .is_empty());
    assert!(llm.prompts.lock().unwrap().is_empty());
    Ok(())
}
