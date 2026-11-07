//! Planned searches are typed needs: each states its kind, takes a subject
//! and a newer-than date only from the gap it serves, keeps to a host only
//! when the planner named one (in `site`, or as `site:` operators in the
//! query, which become one search per site), and reaches the source finder
//! with every field it was planned with.

use std::sync::Mutex;

use lattice_core::application::contracts::web::{
    FetchUrlContentOutput, ResolvedOwnerDto, WebSearchInput, WebSearchOutput, WebSearchResult,
};
use lattice_core::application::ports::WebServiceTrait;
use lattice_core::domain::sourcing::NeedKind;
use lattice_core::shared::error::{AppError, Result};
use serde_json::json;

use super::web_research::{
    need_schema, plan_needs, search_need, with_breadth, Candidate, GapNeed, NeedSearch,
    PlannedNeed, Room,
};
use crate::lessons::course_generation_tests::ScriptedModel;
use lattice_core::application::ports::MockWebService;

fn scripted(outputs: Vec<String>) -> ScriptedModel {
    ScriptedModel {
        outputs: Mutex::new(outputs.into()),
        prompts: Mutex::new(Vec::new()),
    }
}

/// A planned need that stands for one search, as that search.
trait One {
    fn one(self) -> Option<PlannedNeed>;
}

impl One for PlannedNeed {
    fn one(self) -> Option<PlannedNeed> {
        self.searches().map(|searches| {
            assert_eq!(searches.len(), 1, "one search: {searches:?}");
            searches.into_iter().next().unwrap()
        })
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
    .one()
    .unwrap();
    assert_eq!(trimmed.query, "requests timeout");
    assert_eq!(trimmed.subject, None);
    assert_eq!(trimmed.ecosystem.as_deref(), Some("pypi"));

    assert_eq!(need("   ", NeedKind::Web).one(), None, "a blank query");
    assert_eq!(
        need("an archived page", NeedKind::Archive).one(),
        None,
        "an archived copy starts from an address"
    );
    let unknown_registry = PlannedNeed {
        ecosystem: Some("cpan".into()),
        ..need("Moose", NeedKind::Package)
    };
    assert_eq!(unknown_registry.one(), None);
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
    .one()
    .unwrap();
    assert_eq!(dated.identifier.as_deref(), Some(page));
    assert_eq!(dated.alternatives, ["request size limit"]);
    assert_eq!(dated.as_of.as_deref(), Some("2019-03-01"));

    let refused = |need: PlannedNeed| need.one().is_none();
    let base = || need("upload limit", NeedKind::Web);
    assert!(refused(PlannedNeed {
        alternatives: vec!["ok rewording".into(), "  ".into()],
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
    assert_eq!(references.clone().one(), Some(references));
    let article = need("Transformer (deep learning architecture)", NeedKind::Leads);
    assert!(article.one().is_some());
}

#[test]
fn a_site_operator_in_the_query_is_the_search_scope() {
    let scoped = need(
        "site:docs.anthropic.com tool use input schema",
        NeedKind::OfficialDocs,
    )
    .one()
    .unwrap();
    assert_eq!(scoped.query, "tool use input schema");
    assert_eq!(scoped.site.as_deref(), Some("docs.anthropic.com"));
}

#[test]
fn a_query_naming_several_sites_is_one_search_per_site() {
    let searches = PlannedNeed {
        alternatives: vec!["site:platform.openai.com tool result error loop".into()],
        ..need(
            "site:platform.openai.com OR site:docs.anthropic.com function calling error handling",
            NeedKind::OfficialDocs,
        )
    }
    .searches()
    .unwrap();
    let scoped: Vec<_> = searches
        .iter()
        .map(|search| {
            (
                search.query.as_str(),
                search.site.as_deref(),
                search.alternatives.as_slice(),
            )
        })
        .collect();
    let rewording = ["tool result error loop".to_owned()];
    assert_eq!(
        scoped,
        [
            (
                "function calling error handling",
                Some("platform.openai.com"),
                rewording.as_slice()
            ),
            (
                "function calling error handling",
                Some("docs.anthropic.com"),
                rewording.as_slice()
            ),
        ]
    );
}

#[test]
fn a_search_covers_the_addresses_under_its_site() {
    let manual = PlannedNeed {
        site: Some("docs.example.org/manual".into()),
        ..need("random assignment", NeedKind::Web)
    };
    for inside in [
        "https://docs.example.org/manual",
        "https://docs.example.org/manual/chapter.html",
        "https://archive.docs.example.org/manual/chapter",
    ] {
        assert!(manual.covers(inside), "{inside}");
    }
    for outside in [
        "https://en.wikipedia.org/wiki/Manual",
        "https://docs.example.org.attacker.test/manual",
        "https://fakedocs.example.org/manual",
        "https://docs.example.org/manuals",
        "https://docs.example.org/other",
        "not a URL",
    ] {
        assert!(!manual.covers(outside), "{outside}");
    }
    assert!(need("random assignment", NeedKind::Web).covers("https://anywhere.test/"));
}

#[test]
fn a_site_the_planner_set_is_the_scope_and_is_written_as_an_address() {
    let set = PlannedNeed {
        site: Some(" https://Learn.Microsoft.com/en-us/dotnet/ ".into()),
        ..need(
            "site:stackoverflow.com Span<T> async",
            NeedKind::OfficialDocs,
        )
    }
    .one()
    .unwrap();
    assert_eq!(set.query, "Span<T> async");
    assert_eq!(
        set.site.as_deref(),
        Some("learn.microsoft.com/en-us/dotnet")
    );

    let not_an_address = PlannedNeed {
        site: Some("the official docs".into()),
        ..need("Span<T> async", NeedKind::OfficialDocs)
    };
    assert_eq!(not_an_address.searches(), None);
    assert_eq!(
        need("site:react.dev", NeedKind::Web).searches(),
        None,
        "no terms are left to search once the site is lifted out"
    );
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
        assert_eq!(rewording["minLength"], json!(1));
        assert!(rewording.get("maxLength").is_none());
        assert!(
            properties["alternatives"].get("maxItems").is_none(),
            "no fixed count of rewordings"
        );
        assert_eq!(properties["after"]["format"], "date");
        assert_eq!(properties["asOf"]["format"], "date");
    }
}

#[test]
fn a_planned_search_schema_bounds_no_text_but_a_blank_one() {
    for (what, schema) in [
        ("a course plan", need_schema(None)),
        ("a lesson plan", need_schema(Some(2))),
    ] {
        let caps = crate::content_caps_tests::text_caps(&schema);
        assert!(caps.is_empty(), "{what}: {caps:?}");
    }
}

#[test]
fn a_search_is_refused_for_being_blank_never_for_its_length() {
    let long = "how a planner words a search ".repeat(20);
    assert!(long.chars().count() > 240);
    for (what, query) in [
        ("a query under three characters", "ab".to_owned()),
        ("a query past 240 characters", long.clone()),
    ] {
        let planned = need(&query, NeedKind::Web).one();
        assert!(planned.is_some(), "{what}");
    }
    let reworded = PlannedNeed {
        alternatives: vec!["ab".into(), long.trim().into()],
        ..need("upload limit", NeedKind::Web)
    }
    .one();
    assert!(reworded.is_some(), "rewordings of any length");
    assert_eq!(
        need("   ", NeedKind::Web).one(),
        None,
        "a blank query says nothing"
    );
    let blank_rewording = PlannedNeed {
        alternatives: vec!["  ".into()],
        ..need("upload limit", NeedKind::Web)
    };
    assert_eq!(
        blank_rewording.one(),
        None,
        "a blank rewording says nothing"
    );
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
    let found = search_need(&web, &newer, Room::for_all()).await?;
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
    search_need(&web, &dated, Room::for_all()).await?;

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

/// A scholarly catalog: every page is full of results not seen before.
struct Endless {
    web: MockWebService,
    pages: Mutex<usize>,
}

#[async_trait::async_trait]
impl WebServiceTrait for Endless {
    async fn search_web(&self, input: &WebSearchInput) -> Result<WebSearchOutput> {
        *self.pages.lock().unwrap() += 1;
        let mut output = self.web.search_web(input).await?;
        output.results = (0..input.max_results)
            .map(|i| WebSearchResult {
                title: format!("Paper {} {i}", input.page),
                url: format!("https://doi.org/10.1000/{}.{i}", input.page),
                snippet: String::new(),
                published_date: None,
                source: Some("crossref".into()),
            })
            .collect();
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
async fn a_paged_search_of_a_catalog_that_never_runs_out_stops_when_its_room_is_full() -> Result<()>
{
    let web = Endless {
        web: MockWebService::new(),
        pages: Mutex::new(0),
    };
    let cost = |_: &Candidate| 1;
    let room = Room {
        tokens: 30,
        cost: &cost,
    };
    let tonal = need("tonal centre", NeedKind::Paper);
    let mut search = NeedSearch::new(&tonal);
    let mut found = Vec::new();
    while !search.is_done() {
        found.extend(search.next_page(&web, room).await?);
    }
    // Twelve a page: the third page fills the room and is the last asked for.
    assert_eq!(found.len(), 30);
    assert_eq!(*web.pages.lock().unwrap(), 3);
    assert_eq!(found[29].result.url, "https://doi.org/10.1000/3.5");
    Ok(())
}

/// A catalog that answers every page it is asked for with a full page of
/// results not seen before, and fails from page `fail_from` on so that a
/// search which keeps paging still ends.
struct Full {
    web: MockWebService,
    asked: Mutex<Vec<usize>>,
    fail_from: usize,
}

#[async_trait::async_trait]
impl WebServiceTrait for Full {
    async fn search_web(&self, input: &WebSearchInput) -> Result<WebSearchOutput> {
        self.asked.lock().unwrap().push(input.page);
        if input.page >= self.fail_from {
            return Err(AppError::ServiceNotAvailable("No further page".into()));
        }
        let mut output = self.web.search_web(input).await?;
        output.results = (0..input.max_results)
            .map(|i| WebSearchResult {
                title: format!("Paper {} {i}", input.page),
                url: format!("https://doi.org/10.1000/{}.{i}", input.page),
                snippet: String::new(),
                published_date: None,
                source: Some("crossref".into()),
            })
            .collect();
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
async fn a_search_asks_for_the_first_page_only() -> Result<()> {
    let web = Full {
        web: MockWebService::new(),
        asked: Mutex::new(Vec::new()),
        fail_from: 4,
    };
    let found = search_need(
        &web,
        &need("tonal centre", NeedKind::Paper),
        Room::for_all(),
    )
    .await?;
    assert_eq!(*web.asked.lock().unwrap(), [1]);
    assert_eq!(found.len(), 12);
    Ok(())
}

#[tokio::test]
async fn a_need_search_asks_for_the_next_page_each_time_until_a_page_adds_nothing() -> Result<()> {
    let web = Full {
        web: MockWebService::new(),
        asked: Mutex::new(Vec::new()),
        fail_from: 10,
    };
    let tonal = need("tonal centre", NeedKind::Paper);
    let mut search = NeedSearch::new(&tonal);
    let first = search.next_page(&web, Room::for_all()).await?;
    assert_eq!(*web.asked.lock().unwrap(), [1]);
    assert!(!search.is_done());
    let second = search.next_page(&web, Room::for_all()).await?;
    assert_eq!(*web.asked.lock().unwrap(), [1, 2]);
    assert_eq!(first.len(), 12);
    assert_eq!(second.len(), 12);
    assert_eq!(second[0].result.url, "https://doi.org/10.1000/2.0");
    Ok(())
}

#[tokio::test]
async fn a_need_search_is_done_after_a_short_page_and_asks_for_no_more() -> Result<()> {
    let web = Recording {
        web: MockWebService::new(),
        searches: Mutex::new(Vec::new()),
    };
    web.web.set_search_results(
        (0..5)
            .map(|i| WebSearchResult {
                title: format!("Article {i}"),
                url: format!("https://example.org/article-{i}"),
                snippet: String::new(),
                published_date: None,
                source: None,
            })
            .collect(),
    );
    let tonal = need("tonal centre", NeedKind::Web);
    let mut search = NeedSearch::new(&tonal);
    assert_eq!(search.next_page(&web, Room::for_all()).await?.len(), 5);
    assert!(search.is_done());
    assert!(search.next_page(&web, Room::for_all()).await?.is_empty());
    assert_eq!(web.searches.lock().unwrap().len(), 1);
    Ok(())
}

#[tokio::test]
async fn a_need_search_whose_later_page_fails_ends_without_an_error() -> Result<()> {
    let web = Full {
        web: MockWebService::new(),
        asked: Mutex::new(Vec::new()),
        fail_from: 2,
    };
    let tonal = need("tonal centre", NeedKind::Paper);
    let mut search = NeedSearch::new(&tonal);
    assert_eq!(search.next_page(&web, Room::for_all()).await?.len(), 12);
    assert!(search.next_page(&web, Room::for_all()).await?.is_empty());
    assert!(search.is_done());
    assert!(search.next_page(&web, Room::for_all()).await?.is_empty());
    assert_eq!(*web.asked.lock().unwrap(), [1, 2]);
    Ok(())
}

#[tokio::test]
async fn a_need_search_whose_first_page_fails_returns_the_error() {
    let web = Full {
        web: MockWebService::new(),
        asked: Mutex::new(Vec::new()),
        fail_from: 1,
    };
    let tonal = need("tonal centre", NeedKind::Paper);
    assert!(search_need(&web, &tonal, Room::for_all()).await.is_err());
    assert!(NeedSearch::new(&tonal)
        .next_page(&web, Room::for_all())
        .await
        .is_err());
}

#[test]
fn a_plan_of_papers_is_searched_on_the_web_and_in_an_encyclopedia_too() {
    let planned = [
        need("q1", NeedKind::Paper),
        need("q2", NeedKind::OfficialDocs),
    ];
    let searched = with_breadth(&planned, "  Random assignment ");
    assert_eq!(searched[..2], planned);
    assert_eq!(searched.len(), 4);
    assert_eq!(searched[2], need("q1", NeedKind::Web));
    assert_eq!(
        searched[3],
        need("Random assignment", NeedKind::Encyclopedia)
    );
}

#[test]
fn a_plan_keeps_the_web_and_encyclopedia_searches_it_has_and_adds_only_the_missing_kind() {
    let both = [
        need("q1", NeedKind::Paper),
        need("q2", NeedKind::Web),
        need("q3", NeedKind::Encyclopedia),
    ];
    assert_eq!(with_breadth(&both, "Title"), both);

    let web_only = [need("q1", NeedKind::Web)];
    assert_eq!(
        with_breadth(&web_only, "Title"),
        [
            need("q1", NeedKind::Web),
            need("Title", NeedKind::Encyclopedia)
        ]
    );
    let encyclopedia_only = [need("q1", NeedKind::Encyclopedia)];
    assert_eq!(
        with_breadth(&encyclopedia_only, "Title"),
        [
            need("q1", NeedKind::Encyclopedia),
            need("q1", NeedKind::Web)
        ]
    );
}

#[test]
fn a_subtopic_the_planner_covered_with_existing_sources_has_no_searches() {
    assert!(with_breadth(&[], "Title").is_empty());
    let papers = [need("q1", NeedKind::Paper)];
    assert_eq!(
        with_breadth(&papers, "  "),
        [need("q1", NeedKind::Paper), need("q1", NeedKind::Web)],
        "a blank title is no search"
    );
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
