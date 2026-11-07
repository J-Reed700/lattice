//! Reference planning and gathering before drafting; the References finding.
//! Fixtures here are shared with the review-scope tests.
use crate::{
    dto::*,
    lessons::course_generation_tests::{outline, request, ScriptedModel},
    outline_draft::{
        repair_saved, DraftRunRegistry, LearningOutlineIssueKind, LearningOutlineReviewStatus,
        RepairLearningOutlineRequestDto,
    },
    outline_linking,
    outline_progress::{OutlineProgress, OutlineRunRegistry, OutlineStage},
    outline_references::{self, PlannedSubtopic, ReferencePlan},
    repository::LearningRepository,
    service,
};
use crate::features::web::mocks::MockWebService;
use lattice_core::application::contracts::web::{
    FetchUrlContentOutput, WebSearchInput, WebSearchOutput, WebSearchResult,
};
use lattice_core::application::ports::WebServiceTrait;
use lattice_core::shared::error::{AppError, Result};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

pub(super) const ORIGINAL: &str =
    "A randomized comparison isolates the intervention from other influences.";
pub(super) const ADDITIONAL: &str =
    "Random assignment limits confounding when treatment groups are compared.";
pub(super) const SECOND: &str =
    "Observed differences require an uncertainty estimate before causal interpretation.";

pub(super) struct Page {
    pub url: String,
    pub text: String,
    pub truncated: bool,
}

/// A web fixture: every query returns the routed pages (or every page when no
/// route is set); search fails when there are no pages at all. Snippets are
/// poison so a test can prove they never become evidence.
pub(super) struct ResearchWeb {
    inner: MockWebService,
    pages: Vec<Page>,
    routes: HashMap<String, Vec<usize>>,
    /// Exact results by (query, kind, page); once any is set, a search with no
    /// entry finds nothing.
    served: HashMap<(String, String, usize), Vec<String>>,
    pub queries: Mutex<Vec<String>>,
    /// The page each search asked for, in search order.
    pub asked_pages: Mutex<Vec<usize>>,
    /// The kind each search asked for, in search order.
    pub kinds: Mutex<Vec<Option<String>>>,
    pub fetches: Mutex<Vec<String>>,
}
impl ResearchWeb {
    pub fn new(passages: &[&str]) -> Self {
        Self::with_pages(
            passages
                .iter()
                .enumerate()
                .map(|(i, text)| Page {
                    url: format!("https://example.org/reference-{i}"),
                    text: (*text).into(),
                    truncated: false,
                })
                .collect(),
        )
    }
    pub fn with_pages(pages: Vec<Page>) -> Self {
        Self {
            inner: MockWebService::new(),
            pages,
            routes: HashMap::new(),
            served: HashMap::new(),
            queries: Mutex::new(vec![]),
            asked_pages: Mutex::new(vec![]),
            kinds: Mutex::new(vec![]),
            fetches: Mutex::new(vec![]),
        }
    }
    pub fn route(mut self, query: &str, pages: &[usize]) -> Self {
        self.routes.insert(query.into(), pages.to_vec());
        self
    }
    /// The addresses one search returns; a page that is not among the fixture's
    /// pages fails when fetched.
    pub fn serve(mut self, query: &str, kind: &str, page: usize, urls: &[String]) -> Self {
        self.served
            .insert((query.into(), kind.into(), page), urls.to_vec());
        self
    }
    /// Every search made, as (query, kind, page), in order.
    pub fn searches(&self) -> Vec<(String, String, usize)> {
        let queries = self.queries.lock().unwrap();
        let kinds = self.kinds.lock().unwrap();
        let pages = self.asked_pages.lock().unwrap();
        (0..queries.len())
            .map(|i| {
                (
                    queries[i].clone(),
                    kinds[i].clone().unwrap_or_default(),
                    pages[i],
                )
            })
            .collect()
    }
}
fn search_result(url: &str) -> WebSearchResult {
    WebSearchResult {
        title: "Research reference".into(),
        url: url.into(),
        snippet: "SEARCH_SNIPPET_MUST_NOT_BE_EVIDENCE".into(),
        published_date: None,
        source: None,
    }
}
#[async_trait::async_trait]
impl WebServiceTrait for ResearchWeb {
    async fn search_web(&self, input: &WebSearchInput) -> Result<WebSearchOutput> {
        self.queries.lock().unwrap().push(input.query.clone());
        self.kinds.lock().unwrap().push(input.kind.clone());
        self.asked_pages.lock().unwrap().push(input.page);
        if !self.served.is_empty() {
            let key = (
                input.query.clone(),
                input.kind.clone().unwrap_or_default(),
                input.page,
            );
            let results = self
                .served
                .get(&key)
                .into_iter()
                .flatten()
                .map(|url| search_result(url))
                .collect();
            self.inner.set_search_results(results);
            let first = WebSearchInput {
                page: 1,
                offset: 0,
                ..input.clone()
            };
            return self.inner.search_web(&first).await;
        }
        if self.pages.is_empty() {
            return Err(AppError::ServiceNotAvailable(
                "Search fixture unavailable".into(),
            ));
        }
        let indices: Vec<usize> = match self.routes.get(&input.query) {
            Some(indices) => indices.clone(),
            None => (0..self.pages.len()).collect(),
        };
        let results: Vec<_> = indices
            .iter()
            .filter_map(|i| self.pages.get(*i))
            .map(|page| search_result(&page.url))
            .collect();
        self.inner.set_search_results(results);
        self.inner.search_web(input).await
    }
    async fn fetch_reference_content(&self, url: &str) -> Result<FetchUrlContentOutput> {
        self.fetches.lock().unwrap().push(url.into());
        let page = self
            .pages
            .iter()
            .find(|page| page.url == url)
            .ok_or_else(|| AppError::Network(format!("fixture has no page {url}")))?;
        Ok(FetchUrlContentOutput {
            url: page.url.clone(),
            title: Some("Research reference".into()),
            content: page.text.clone(),
            content_truncated: page.truncated,
            word_count: page.text.split_whitespace().count(),
            fetch_time_ms: 1.,
            content_type: None,
            from_cache: false,
            read_via: Default::default(),
        })
    }
    async fn fetch_url_content(&self, _: &str) -> Result<FetchUrlContentOutput> {
        panic!("Research must use complete reference captures, not chat previews")
    }
    fn validate_url(&self, _: &str) -> Result<()> {
        Ok(())
    }
}

/// Attach one citation at a JSON pointer.
pub(super) fn cite_at(candidate: &mut Value, path: &str, index: usize, quote: &str) {
    let field = candidate.pointer_mut(path).unwrap();
    field["sourceIndex"] = json!(index);
    field["quote"] = json!(quote);
}
pub(super) fn candidate() -> Value {
    serde_json::from_str(&outline(4, 3).0).unwrap()
}
/// A review with nothing open and nothing new.
pub(super) fn clean() -> Value {
    json!({"newFindings":[]})
}
pub(super) fn plan_finding(path: &str, claim: &str, reason: &str) -> Value {
    json!({"path":path,"kind":"plan","claim":claim,"reason":reason})
}
pub(super) fn factual_finding(path: &str, claim: &str, reason: &str) -> Value {
    json!({"path":path,"kind":"factual","claim":claim,"reason":reason})
}
/// A review of a draft with no open findings that raises `items`.
pub(super) fn findings(items: Vec<Value>) -> Value {
    json!({"newFindings":items})
}
/// A review giving each open finding its verdict, then raising `items`.
pub(super) fn verdicts(verdicts: &[(&str, &str)], items: Vec<Value>) -> Value {
    json!({"verdicts":verdicts.iter().map(|(id, status)| json!({"id":id,"status":status,"reason":format!("Judged against the current text at the finding's path: {status}.")})).collect::<Vec<_>>(),"newFindings":items})
}
/// One plan finding on a whole module.
pub(super) fn issue(index: usize) -> Value {
    findings(vec![plan_finding(
        &format!("/modules/{index}"),
        "The comparison needs stronger evidence.",
        "The project brief does not say how the comparison is measured. Name the measurement.",
    )])
}
/// A strict claim-check reply in the fixture's protocol: label first, then
/// the reason and the passage identifier the verdict rests on.
pub(super) fn judged(label: &str, passage: Option<usize>) -> Value {
    Value::String(format!(
        "{label}\nReason: Every factual part of the claim was compared with the passages.\nSource passage: {}",
        passage.map_or("none".to_owned(), |i| format!("passage-{i}"))
    ))
}
/// A scripted model: JSON replies are serialized, string replies (a claim
/// check's labelled lines) are sent as written.
pub(super) fn model(outputs: Vec<Value>) -> ScriptedModel {
    ScriptedModel {
        outputs: Mutex::new(
            outputs
                .into_iter()
                .map(|v| match v {
                    Value::String(text) => text,
                    other => other.to_string(),
                })
                .collect(),
        ),
        prompts: Mutex::new(vec![]),
    }
}
pub(super) fn plan_output(subtopics: &[(&str, &[&str])]) -> Value {
    json!({"subtopics":subtopics.iter().map(|(title, queries)| json!({"title":title,"needs":queries.iter().map(|query| json!({"query":query,"kind":"official_docs"})).collect::<Vec<_>>()})).collect::<Vec<_>>()})
}
/// A planned search for `kind` of source.
fn planned(
    query: &str,
    kind: lattice_core::domain::sourcing::NeedKind,
) -> crate::references::web_research::PlannedNeed {
    crate::references::web_research::PlannedNeed {
        kind,
        ..crate::references::web_research::PlannedNeed::web(query.into())
    }
}
/// A planned search for papers.
fn paper(query: &str) -> crate::references::web_research::PlannedNeed {
    planned(query, lattice_core::domain::sourcing::NeedKind::Paper)
}
pub(super) fn select_all(count: usize) -> Value {
    json!({"decisions":(0..count).map(|i| json!({"id":format!("result-{i}"),"useForGap":true,"reason":"A primary reference for this subtopic."})).collect::<Vec<_>>()})
}
/// The evidence-linking replies for one run: each listed item path gets the
/// candidate whose passage is (or contains) the given text, every other item
/// `none`. The replies are built from the prompts, so a test does not depend
/// on how many batches retrieval produced.
pub(super) fn links(picks: &[(&str, &str)]) -> Value {
    json!({"@links":picks.iter().map(|(path, quote)| (path.to_string(), json!(quote))).collect::<serde_json::Map<_, _>>()})
}
/// An evidence-linking run that links nothing.
pub(super) fn no_links() -> Value {
    links(&[])
}
/// The model's prompts other than evidence-linking batches, whose number
/// depends on retrieval rather than on the protocol under test.
pub(super) fn calls(llm: &ScriptedModel) -> Vec<String> {
    llm.prompts
        .lock()
        .unwrap()
        .iter()
        .filter(|p| !p.starts_with(outline_linking::LINK_SYSTEM))
        .cloned()
        .collect()
}
/// The evidence-linking prompts, in order.
pub(super) fn linking_prompts(llm: &ScriptedModel) -> Vec<String> {
    llm.prompts
        .lock()
        .unwrap()
        .iter()
        .filter(|p| p.starts_with(outline_linking::LINK_SYSTEM))
        .cloned()
        .collect()
}
pub(super) fn original_source() -> LearningSourceDto {
    LearningSourceDto {
        id: uuid::Uuid::new_v4().to_string(),
        title: "Brief reference overview".into(),
        excerpt: ORIGINAL.into(),
        url: Some("https://example.org/overview".into()),
        acquired_at: 0,
    }
}
pub(super) fn references(
    sources: &[LearningSourceDto],
) -> Vec<crate::sources::InitialReference> {
    sources
        .iter()
        .map(|s| crate::sources::InitialReference {
            source_id: s.id.clone(),
            origin: s.url.clone().unwrap(),
            captured: crate::source_library::CapturedLearningSource {
                title: s.title.clone(),
                publisher: None,
                requested_url: s.url.clone(),
                resolved_url: s.url.clone(),
                text: s.excerpt.clone(),
                truncated: false,
                extraction_version: "complete_test_capture".into(),
                signals: crate::references::trust::capture::not_extracted(
                    "test fixture",
                ),
            },
            found: false,
        })
        .collect()
}
pub(super) fn payload(prompt: &str) -> Value {
    serde_json::from_str(prompt.rsplit("\n\n").next().unwrap()).unwrap()
}
async fn generate_online(
    repo: &LearningRepository,
    model: &ScriptedModel,
    web: &ResearchWeb,
    sources: Vec<LearningSourceDto>,
) -> Result<LearningProgramDto> {
    let captured = references(&sources);
    let drafts = DraftRunRegistry::new();
    service::generate_with_references_and_progress(
        repo,
        &drafts,
        model,
        request(LearningCourseDepth::Course),
        sources,
        &captured,
        &OutlineProgress::default(),
        Some(web),
    )
    .await
}

const GOAL: &str = "Design randomized experiments, measure their outcomes, and interpret the results with honest uncertainty";

#[tokio::test]
async fn the_plan_guard_drops_queries_that_copy_the_goal_and_fails_only_when_none_remain(
) -> Result<()> {
    let mut request = request(LearningCourseDepth::Focused);
    request.goal = GOAL.into();
    let progress = OutlineProgress::default();
    // One copied query is dropped; the public one survives.
    let llm =
        model(vec![plan_output(&[
        (
            "Randomized comparison",
            &[
                "design randomized experiments measure their outcomes and interpret the results",
                "randomized controlled trial design documentation",
            ],
        ),
        ("Uncertainty", &["confidence interval interpretation textbook"]),
    ])]);
    let plan = outline_references::plan(&llm, &request, &[], &progress).await?;
    assert_eq!(plan.query_count(), 2);
    assert_eq!(
        plan.subtopics[0]
            .needs
            .iter()
            .map(|need| (need.query.as_str(), need.kind))
            .collect::<Vec<_>>(),
        [(
            "randomized controlled trial design documentation",
            lattice_core::domain::sourcing::NeedKind::OfficialDocs
        )]
    );
    for subtopic in &plan.subtopics {
        for need in &subtopic.needs {
            assert!(!outline_references::copies_private_text(
                &need.query,
                &[GOAL, &request.prior_knowledge]
            ));
        }
    }
    // Every query copied the goal: nothing usable remains, so it is an error.
    let llm = model(vec![plan_output(&[(
        "Randomized comparison",
        &["Design randomized experiments, measure their outcomes, and interpret the results"],
    )])]);
    assert!(outline_references::plan(&llm, &request, &[], &progress)
        .await
        .is_err());
    // No queries and no sources is malformed; no queries with sources is a
    // planner that found the catalog sufficient.
    let empty = plan_output(&[("Randomized comparison", &[])]);
    let llm = model(vec![empty.clone()]);
    assert!(outline_references::plan(&llm, &request, &[], &progress)
        .await
        .is_err());
    let llm = model(vec![empty]);
    let plan = outline_references::plan(&llm, &request, &[original_source()], &progress).await?;
    assert_eq!(plan.query_count(), 0);
    assert_eq!(
        plan.subtopic_titles(),
        vec!["Randomized comparison".to_owned()]
    );
    Ok(())
}

#[tokio::test]
async fn gather_runs_one_selection_per_subtopic_and_dedupes_by_url_and_content() -> Result<()> {
    let web = ResearchWeb::with_pages(vec![
        Page {
            url: "https://example.org/a".into(),
            text: ADDITIONAL.repeat(3),
            truncated: false,
        },
        Page {
            url: "https://mirror.example.org/a-copy".into(),
            text: ADDITIONAL.repeat(3),
            truncated: false,
        },
        Page {
            url: "https://example.org/truncated".into(),
            text: SECOND.repeat(3),
            truncated: true,
        },
        Page {
            url: "https://example.org/c".into(),
            text: SECOND.repeat(3),
            truncated: false,
        },
        Page {
            url: "https://example.org/overview".into(),
            text: ORIGINAL.into(),
            truncated: false,
        },
    ])
    .route("q1", &[0, 1, 2, 4])
    .route("q2", &[1, 0])
    .route("q3", &[3, 4])
    .route("Random assignment", &[0])
    .route("Uncertainty", &[3]);
    let plan = ReferencePlan {
        subtopics: vec![
            PlannedSubtopic {
                title: "Random assignment".into(),
                needs: vec![paper("q1"), paper("q2")],
            },
            PlannedSubtopic {
                title: "Uncertainty".into(),
                needs: vec![paper("q3")],
            },
            PlannedSubtopic {
                title: "Already covered".into(),
                needs: vec![],
            },
        ],
    };
    // Subtopic 1 sees three new URLs (a, mirror, truncated; overview exists);
    // subtopic 2 sees one (c). Both selections approve everything.
    let llm = model(vec![select_all(3), select_all(1)]);
    let existing = vec![original_source()];
    let found =
        outline_references::gather(&llm, &web, &plan, &existing, &OutlineProgress::default())
            .await?;
    assert_eq!(
        web.queries.lock().unwrap().as_slice(),
        [
            "q1",
            "q2",
            "q1",
            "Random assignment",
            "q3",
            "q3",
            "Uncertainty"
        ]
    );
    assert_eq!(
        web.kinds.lock().unwrap().as_slice(),
        [
            Some("paper".to_owned()),
            Some("paper".to_owned()),
            Some("web".to_owned()),
            Some("encyclopedia".to_owned()),
            Some("paper".to_owned()),
            Some("web".to_owned()),
            Some("encyclopedia".to_owned())
        ],
        "each planned search goes out with the kind its plan gave it, then the web and encyclopedia searches"
    );
    assert_eq!(found.searches, 7);
    assert_eq!(llm.prompts.lock().unwrap().len(), 2);
    let selection = payload(&llm.prompts.lock().unwrap()[0]);
    assert_eq!(selection["topic"], "Random assignment");
    assert_eq!(selection["results"].as_array().unwrap().len(), 3);
    assert!(selection["results"]
        .as_array()
        .unwrap()
        .iter()
        .all(|r| r["url"] != "https://example.org/overview"));
    // Mirror dropped by content hash, truncated page dropped by validation.
    let urls: Vec<_> = found
        .sources
        .iter()
        .map(|s| s.url.clone().unwrap())
        .collect();
    assert_eq!(urls, ["https://example.org/a", "https://example.org/c"]);
    assert_eq!(found.references.len(), 2);
    assert_eq!(found.references[0].captured.text, ADDITIONAL.repeat(3));
    assert_eq!(found.unavailable, 1);
    let fetches = web.fetches.lock().unwrap();
    assert_eq!(fetches.len(), 4);
    assert!(!fetches.iter().any(|u| u == "https://example.org/overview"));
    Ok(())
}

#[tokio::test]
async fn a_subtopic_planned_only_as_papers_is_also_searched_on_the_web_and_in_an_encyclopedia(
) -> Result<()> {
    let web = ResearchWeb::new(&[ADDITIONAL]);
    let plan = ReferencePlan {
        subtopics: vec![PlannedSubtopic {
            title: "Random assignment".into(),
            needs: vec![paper("q1"), paper("q2")],
        }],
    };
    let llm = model(vec![select_all(1)]);
    let found =
        outline_references::gather(&llm, &web, &plan, &[], &OutlineProgress::default()).await?;
    assert_eq!(
        web.queries.lock().unwrap().as_slice(),
        ["q1", "q2", "q1", "Random assignment"],
        "the planned searches, then the first query on the web, then the title"
    );
    assert_eq!(
        web.kinds.lock().unwrap().as_slice(),
        [
            Some("paper".to_owned()),
            Some("paper".to_owned()),
            Some("web".to_owned()),
            Some("encyclopedia".to_owned())
        ]
    );
    assert_eq!(found.searches, 4);
    Ok(())
}

#[tokio::test]
async fn a_subtopic_whose_needs_already_include_web_and_encyclopedia_is_searched_as_planned(
) -> Result<()> {
    use lattice_core::domain::sourcing::NeedKind;
    let web = ResearchWeb::new(&[ADDITIONAL]);
    let plan = ReferencePlan {
        subtopics: vec![PlannedSubtopic {
            title: "Random assignment".into(),
            needs: vec![
                paper("q1"),
                planned("q2", NeedKind::Web),
                planned("q3", NeedKind::Encyclopedia),
            ],
        }],
    };
    let llm = model(vec![select_all(1)]);
    let found =
        outline_references::gather(&llm, &web, &plan, &[], &OutlineProgress::default()).await?;
    assert_eq!(web.queries.lock().unwrap().as_slice(), ["q1", "q2", "q3"]);
    assert_eq!(
        web.kinds.lock().unwrap().as_slice(),
        [
            Some("paper".to_owned()),
            Some("web".to_owned()),
            Some("encyclopedia".to_owned())
        ]
    );
    assert_eq!(found.searches, 3);
    Ok(())
}

fn url_list(prefix: &str, count: usize) -> Vec<String> {
    (0..count)
        .map(|i| format!("https://example.org/{prefix}-{i}"))
        .collect()
}
fn readable(url: &str) -> Page {
    Page {
        url: url.into(),
        text: ADDITIONAL.repeat(3),
        truncated: false,
    }
}
fn search(query: &str, kind: &str, page: usize) -> (String, String, usize) {
    (query.into(), kind.into(), page)
}
/// A selection that turns down every one of `count` results.
fn reject_all(count: usize) -> Value {
    json!({"decisions":(0..count).map(|i| json!({"id":format!("result-{i}"),"useForGap":false,"reason":"Off topic for this subtopic."})).collect::<Vec<_>>()})
}
fn one_subtopic(title: &str, query: &str) -> ReferencePlan {
    ReferencePlan {
        subtopics: vec![PlannedSubtopic {
            title: title.into(),
            needs: vec![paper(query)],
        }],
    }
}
fn shown_urls(prompt: &str) -> Vec<String> {
    payload(prompt)["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|result| result["url"].as_str().unwrap().to_owned())
        .collect()
}

#[tokio::test]
async fn a_subtopic_whose_first_round_reads_nothing_is_saved_by_a_later_round() -> Result<()> {
    let (cut, gone, good) = (
        "https://example.org/cut".to_owned(),
        "https://example.org/gone".to_owned(),
        "https://example.org/good".to_owned(),
    );
    let web = ResearchWeb::with_pages(vec![
        Page {
            url: cut.clone(),
            text: SECOND.repeat(3),
            truncated: true,
        },
        readable(&good),
    ])
    .serve("q1", "paper", 1, &[cut.clone(), gone.clone()])
    .serve("q1", "community", 1, std::slice::from_ref(&good));
    let plan = one_subtopic("Random assignment", "q1");
    let llm = model(vec![select_all(2), select_all(1)]);
    let found =
        outline_references::gather(&llm, &web, &plan, &[], &OutlineProgress::default()).await?;
    assert_eq!(found.references.len(), 1);
    assert_eq!(found.references[0].origin, good);
    assert_eq!(found.references[0].captured.text, ADDITIONAL.repeat(3));
    let searches = web.searches();
    assert_eq!(
        searches[..3],
        [
            search("q1", "paper", 1),
            search("q1", "web", 1),
            search("Random assignment", "encyclopedia", 1)
        ]
    );
    assert_eq!(searches[3], search("q1", "community", 1));
    Ok(())
}

#[tokio::test]
async fn a_subtopic_whose_first_selection_picks_nothing_is_selected_again_in_a_later_round(
) -> Result<()> {
    let (off, good) = (
        "https://example.org/off-topic".to_owned(),
        "https://example.org/good".to_owned(),
    );
    let web = ResearchWeb::with_pages(vec![readable(&off), readable(&good)])
        .serve("q1", "paper", 1, std::slice::from_ref(&off))
        .serve("q1", "community", 1, std::slice::from_ref(&good));
    let plan = one_subtopic("Random assignment", "q1");
    let llm = model(vec![reject_all(1), select_all(1)]);
    let found =
        outline_references::gather(&llm, &web, &plan, &[], &OutlineProgress::default()).await?;
    assert_eq!(found.references.len(), 1);
    assert_eq!(found.references[0].origin, good);
    assert_eq!(llm.prompts.lock().unwrap().len(), 2);
    assert!(!web.fetches.lock().unwrap().contains(&off));
    Ok(())
}

#[tokio::test]
async fn a_subtopic_saved_in_its_first_round_sends_no_further_searches() -> Result<()> {
    let good = "https://example.org/good".to_owned();
    let web = ResearchWeb::with_pages(vec![readable(&good)])
        .serve("q1", "paper", 1, std::slice::from_ref(&good))
        .serve("q1", "community", 1, &url_list("late", 3));
    let plan = one_subtopic("Random assignment", "q1");
    let llm = model(vec![select_all(1)]);
    let found =
        outline_references::gather(&llm, &web, &plan, &[], &OutlineProgress::default()).await?;
    assert_eq!(found.references.len(), 1);
    assert_eq!(
        web.searches(),
        [
            search("q1", "paper", 1),
            search("q1", "web", 1),
            search("Random assignment", "encyclopedia", 1)
        ]
    );
    assert_eq!(llm.prompts.lock().unwrap().len(), 1);
    Ok(())
}

#[tokio::test]
async fn a_subtopic_that_never_reads_a_page_asks_the_four_rounds_and_stops() -> Result<()> {
    let (paper_hits, web_hits, encyclopedia_hits) = (
        url_list("paper", 12),
        url_list("web", 3),
        url_list("encyclopedia", 12),
    );
    let web = ResearchWeb::with_pages(vec![])
        .serve("q1", "paper", 1, &paper_hits)
        .serve("q1", "web", 1, &web_hits)
        .serve("Random assignment", "encyclopedia", 1, &encyclopedia_hits);
    let plan = one_subtopic("Random assignment", "q1");
    let llm = model(vec![select_all(1)]);
    let found =
        outline_references::gather(&llm, &web, &plan, &[], &OutlineProgress::default()).await?;
    assert!(found.references.is_empty());
    assert_eq!(
        web.searches(),
        [
            // Planned: the plan's need, then the web and encyclopedia.
            search("q1", "paper", 1),
            search("q1", "web", 1),
            search("Random assignment", "encyclopedia", 1),
            // Deeper: page 2 of the two searches whose first page was full.
            search("q1", "paper", 2),
            search("Random assignment", "encyclopedia", 2),
            // Other kinds: the general kind not yet asked, with the first query.
            search("q1", "community", 1),
            // Shorter: the title on the web.
            search("Random assignment", "web", 1),
        ]
    );
    assert_eq!(llm.prompts.lock().unwrap().len(), 1);
    Ok(())
}

#[tokio::test]
async fn a_later_round_shows_the_selector_only_results_it_has_not_shown_before() -> Result<()> {
    let good = "https://example.org/good".to_owned();
    let (first, second) = (
        "https://example.org/first".to_owned(),
        "https://example.org/second".to_owned(),
    );
    let saved_already = original_source();
    let saved_url = saved_already.url.clone().unwrap();
    let web = ResearchWeb::with_pages(vec![readable(&good)])
        .serve("q1", "paper", 1, &[first.clone(), second.clone()])
        .serve(
            "q1",
            "community",
            1,
            &[
                first.clone(),
                saved_url.clone(),
                second.clone(),
                good.clone(),
            ],
        );
    let plan = one_subtopic("Random assignment", "q1");
    let llm = model(vec![select_all(2), select_all(1)]);
    let found = outline_references::gather(
        &llm,
        &web,
        &plan,
        &[saved_already],
        &OutlineProgress::default(),
    )
    .await?;
    let prompts = llm.prompts.lock().unwrap().clone();
    assert_eq!(prompts.len(), 2);
    assert_eq!(shown_urls(&prompts[0]), [first, second]);
    assert_eq!(shown_urls(&prompts[1]), std::slice::from_ref(&good));
    assert_eq!(found.references.len(), 1);
    assert_eq!(found.references[0].origin, good);
    Ok(())
}

#[tokio::test]
async fn only_a_subtopic_with_nothing_saved_gets_later_rounds() -> Result<()> {
    let good = "https://example.org/good".to_owned();
    let web = ResearchWeb::with_pages(vec![readable(&good)]).serve(
        "q3",
        "paper",
        1,
        std::slice::from_ref(&good),
    );
    let plan = ReferencePlan {
        subtopics: vec![
            PlannedSubtopic {
                title: "Random assignment".into(),
                needs: vec![paper("q1")],
            },
            PlannedSubtopic {
                title: "Uncertainty".into(),
                needs: vec![paper("q3")],
            },
        ],
    };
    let llm = model(vec![select_all(1)]);
    let found =
        outline_references::gather(&llm, &web, &plan, &[], &OutlineProgress::default()).await?;
    assert_eq!(found.references.len(), 1);
    assert_eq!(found.references[0].origin, good);
    let searches = web.searches();
    let of = |query: &str, title: &str| -> Vec<(String, String, usize)> {
        searches
            .iter()
            .filter(|(q, _, _)| q == query || q == title)
            .cloned()
            .collect()
    };
    assert_eq!(
        of("q1", "Random assignment"),
        [
            search("q1", "paper", 1),
            search("q1", "web", 1),
            search("Random assignment", "encyclopedia", 1),
            search("q1", "community", 1),
            search("Random assignment", "web", 1),
        ]
    );
    assert_eq!(
        of("q3", "Uncertainty"),
        [
            search("q3", "paper", 1),
            search("q3", "web", 1),
            search("Uncertainty", "encyclopedia", 1),
        ]
    );
    assert_eq!(searches.len(), 8);
    Ok(())
}

#[tokio::test]
async fn references_are_gathered_before_drafting_and_saved_in_full_with_the_program() -> Result<()>
{
    let pool = crate::tests::pool().await?;
    let repo = LearningRepository::new(pool.clone());
    let deep = "Random assignment limits confounding when treatment groups are compared.";
    let full = format!(
        "{}\n\n{deep}\n\n{}",
        "Preface and publishing information.\n".repeat(200),
        "Appendix material.\n".repeat(50)
    );
    let web = ResearchWeb::with_pages(vec![Page {
        url: "https://example.org/full".into(),
        text: full.clone(),
        truncated: false,
    }]);
    let llm = model(vec![
        plan_output(&[("Random assignment", &["random assignment confounding"])]),
        select_all(1),
        candidate(),
        no_links(),
        clean(),
    ]);
    let stages = Arc::new(Mutex::new(vec![]));
    let details = Arc::new(Mutex::new(vec![]));
    let (observed, seen) = (stages.clone(), details.clone());
    let run = OutlineRunRegistry::new().register(None, move |update| {
        observed.lock().unwrap().push(update.stage);
        if let Some(detail) = update.detail {
            seen.lock().unwrap().push(detail);
        }
    })?;
    let drafts = DraftRunRegistry::new();
    let program = run
        .progress
        .run(service::generate_with_references_and_progress(
            &repo,
            &drafts,
            &llm,
            request(LearningCourseDepth::Course),
            vec![],
            &[],
            &run.progress,
            Some(&web),
        ))
        .await?;
    let review = program.outline_review.as_ref().unwrap();
    assert_eq!(review.status, LearningOutlineReviewStatus::Passed);
    assert_eq!(review.references_gathered, 1);
    assert!(review.note.contains("1 references saved"));
    assert_eq!(program.sources.len(), 1);
    assert_eq!(program.sources[0].excerpt.chars().count(), 2400);
    let saved = repo.verification_sources(&program.summary.id).await?;
    assert_eq!(saved.len(), 1);
    assert_eq!(saved[0].excerpt, full.trim());
    assert!(!saved[0].excerpt.contains("SEARCH_SNIPPET"));
    let prompts = calls(&llm);
    assert_eq!(prompts.len(), 4);
    // The plan runs first, the draft retrieves by subtopic and sees the deep passage.
    assert!(prompts[0].starts_with("Plan the reference material"));
    let draft_context = payload(&prompts[2]);
    assert_eq!(draft_context["sourceCatalog"]["totalSources"], 1);
    assert!(prompts[2].contains(deep));
    assert!(!prompts[2].contains("name the sourceIndex of the passage that supports it"));
    assert!(prompts[2].contains("Where a passage directly addresses the specific topic or skill"));
    assert!(!prompts[2].contains("most plan items need no citation"));
    // Linking offers only the saved page's own prose: the preface and appendix
    // lines are page furniture, never candidates. The outcome is in the note.
    assert!(linking_prompts(&llm).iter().all(|prompt| !prompt
        .contains("Preface and publishing information")
        && !prompt.contains("Appendix material")));
    assert!(review
        .note
        .contains("of 24 outline items linked to references"));
    let stages = stages.lock().unwrap().clone();
    let position = |stage: OutlineStage| stages.iter().position(|s| *s == stage).unwrap();
    assert!(position(OutlineStage::GatheringReferences) < position(OutlineStage::Drafting));
    assert!(position(OutlineStage::Drafting) < position(OutlineStage::CheckingCitations));
    assert!(position(OutlineStage::CheckingCitations) < position(OutlineStage::Reviewing));
    assert!(details
        .lock()
        .unwrap()
        .iter()
        .any(|d| d.starts_with("Subtopic 1 of 1")));
    let draft = repo.outline_draft(&program.summary.id).await?.unwrap();
    assert_eq!(
        draft.reference_plan.unwrap().subtopic_titles(),
        vec!["Random assignment".to_owned()]
    );
    Ok(())
}

#[tokio::test]
async fn zero_gathered_pages_is_a_references_finding_that_repair_resolves_by_gathering(
) -> Result<()> {
    let repo = LearningRepository::new(crate::tests::pool().await?);
    let llm = model(vec![
        plan_output(&[("Random assignment", &["random assignment confounding"])]),
        candidate(),
        clean(),
    ]);
    let offline = ResearchWeb::new(&[]);
    let saved = generate_online(&repo, &llm, &offline, vec![]).await?;
    let review = saved.outline_review.as_ref().unwrap();
    assert_eq!(review.status, LearningOutlineReviewStatus::NeedsRepair);
    assert_eq!(review.issues.len(), 1);
    assert_eq!(review.issues[0].kind, LearningOutlineIssueKind::References);
    assert_eq!(review.issues[0].path, "");
    assert!(review.note.contains("no references could be gathered"));
    // The planned searches, then the community and paper searches, then the
    // title: every search fails, so the subtopic is widened to the end.
    assert_eq!(offline.queries.lock().unwrap().len(), 6);
    assert_eq!(llm.prompts.lock().unwrap().len(), 3);
    assert!(saved.sources.is_empty());
    let drafts = DraftRunRegistry::new();
    assert!(service::accept(
        &repo,
        &drafts,
        AcceptLearningProgramRequestDto {
            program_id: saved.summary.id.clone(),
            expected_revision: saved.summary.revision,
            title: saved.summary.title.clone(),
        }
    )
    .await
    .is_err());
    // Repair reuses the saved plan: no new planning call, one selection. The
    // modules were already reviewed and references are not part of that
    // receipt, so the finding is gone without another review call.
    let online = ResearchWeb::new(&[ADDITIONAL]);
    let llm = model(vec![select_all(1)]);
    let repaired = repair_saved(
        &repo,
        &drafts,
        &llm,
        &RepairLearningOutlineRequestDto {
            program_id: saved.summary.id.clone(),
            expected_revision: saved.summary.revision,
        },
        &OutlineProgress::default(),
        Some(&online),
    )
    .await?;
    let review = repaired.outline_review.as_ref().unwrap();
    assert_eq!(review.status, LearningOutlineReviewStatus::Passed);
    assert!(review.issues.is_empty());
    assert_eq!(review.references_gathered, 1);
    assert_eq!(repaired.sources.len(), 1);
    assert_eq!(
        online.queries.lock().unwrap().as_slice(),
        [
            "random assignment confounding",
            "random assignment confounding",
            "Random assignment"
        ]
    );
    assert_eq!(llm.prompts.lock().unwrap().len(), 1);
    service::accept(
        &repo,
        &drafts,
        AcceptLearningProgramRequestDto {
            program_id: repaired.summary.id.clone(),
            expected_revision: repaired.summary.revision,
            title: repaired.summary.title.clone(),
        },
    )
    .await?;
    Ok(())
}

#[tokio::test]
async fn user_sources_are_supplemented_and_the_planner_sees_their_catalog() -> Result<()> {
    let repo = LearningRepository::new(crate::tests::pool().await?);
    let web = ResearchWeb::new(&[ADDITIONAL]);
    let llm = model(vec![
        plan_output(&[("Random assignment", &["random assignment confounding"])]),
        select_all(1),
        candidate(),
        no_links(),
        clean(),
    ]);
    let program = generate_online(&repo, &llm, &web, vec![original_source()]).await?;
    assert_eq!(
        program.outline_review.as_ref().unwrap().status,
        LearningOutlineReviewStatus::Passed
    );
    assert_eq!(program.sources.len(), 2);
    assert_eq!(program.sources[0].excerpt, ORIGINAL);
    assert_eq!(program.sources[1].excerpt, ADDITIONAL);
    let prompts = calls(&llm);
    let plan = payload(&prompts[0]);
    assert_eq!(plan["existingSources"]["totalSources"], 1);
    assert!(plan["existingSources"]
        .to_string()
        .contains("Brief reference overview"));
    assert_eq!(plan["depth"]["maxModules"], 6);
    assert!(prompts[0].contains("never copy the learner's goal"));
    let draft = payload(&prompts[2]);
    assert_eq!(draft["sourceCatalog"]["totalSources"], 2);
    assert_eq!(
        repo.verification_sources(&program.summary.id).await?.len(),
        2
    );
    Ok(())
}

#[tokio::test]
async fn offline_topic_drafts_still_pass_without_a_references_finding() -> Result<()> {
    let repo = LearningRepository::new(crate::tests::pool().await?);
    let llm = model(vec![candidate(), clean()]);
    let drafts = DraftRunRegistry::new();
    let program = service::generate(
        &repo,
        &drafts,
        &llm,
        request(LearningCourseDepth::Course),
        vec![],
    )
    .await?;
    let review = program.outline_review.as_ref().unwrap();
    assert_eq!(review.status, LearningOutlineReviewStatus::Passed);
    assert!(review.issues.is_empty());
    assert_eq!(review.citations_verified, 0);
    assert_eq!(llm.prompts.lock().unwrap().len(), 2);
    Ok(())
}

#[tokio::test]
async fn typography_only_mismatches_are_restored_before_the_author_citation_is_offered(
) -> Result<()> {
    let repo = LearningRepository::new(crate::tests::pool().await?);
    let exact = "The cell’s response changes when experimental conditions differ.";
    let mut source = original_source();
    source.excerpt = exact.into();
    let mut initial = candidate();
    cite_at(
        &mut initial,
        "/modules/0/summary",
        0,
        &exact.replace('’', "'"),
    );
    let llm = model(vec![
        initial,
        links(&[("/modules/0/summary", exact)]),
        clean(),
    ]);
    let sources = vec![source];
    let drafts = DraftRunRegistry::new();
    let program = service::generate_with_references(
        &repo,
        &drafts,
        &llm,
        request(LearningCourseDepth::Course),
        sources.clone(),
        &references(&sources),
    )
    .await?;
    let review = program.outline_review.as_ref().unwrap();
    assert_eq!(review.status, LearningOutlineReviewStatus::Passed);
    assert_eq!(review.citations_verified, 1);
    assert_eq!(review.citations_removed, 0);
    assert_eq!(review.repair_passes, 0);
    assert_eq!(calls(&llm).len(), 2);
    // The chooser is offered the source's own typography, never the author's.
    let offered = linking_prompts(&llm);
    assert!(!offered.is_empty());
    assert!(offered
        .iter()
        .all(|p| !payload(p)["passages"].to_string().contains("cell's")));
    assert!(offered
        .iter()
        .any(|p| payload(p)["passages"].to_string().contains("cell’s")));
    let saved = repo.outline_draft(&program.summary.id).await?.unwrap();
    assert_eq!(saved.candidate["modules"][0]["summary"]["quote"], exact);
    assert_eq!(
        saved
            .citation_receipts
            .values()
            .filter(|r| r.covers)
            .count(),
        1
    );
    Ok(())
}

/// A course plan's searches take the same rewordings and dates as a lesson's,
/// checked the same way.
#[tokio::test]
async fn a_course_plan_keeps_rewordings_and_dates_and_refuses_an_unreadable_date() -> Result<()> {
    let request = request(LearningCourseDepth::Focused);
    let progress = OutlineProgress::default();
    let subtopic = |need: Value| json!({"subtopics":[{"title":"Release cadence","needs":[need]}]});
    let llm = model(vec![subtopic(json!({
        "query": "Python release cycle",
        "kind": "official_docs",
        "alternatives": ["CPython annual release schedule"],
        "after": "2024-01-01"
    }))]);
    let plan = outline_references::plan(&llm, &request, &[], &progress).await?;
    let need = &plan.subtopics[0].needs[0];
    assert_eq!(need.alternatives, ["CPython annual release schedule"]);
    assert_eq!(need.after.as_deref(), Some("2024-01-01"));

    let llm = model(vec![subtopic(json!({
        "query": "Python release cycle",
        "kind": "official_docs",
        "after": "last year"
    }))]);
    assert!(outline_references::plan(&llm, &request, &[], &progress)
        .await
        .is_err());
    Ok(())
}

/// A subtopic's title is the planner's own words; only a blank one is refused.
#[tokio::test]
async fn a_long_subtopic_title_from_the_planner_is_read_not_refused() -> Result<()> {
    let request = request(LearningCourseDepth::Focused);
    let title = "A randomized comparison isolates a treatment effect. "
        .repeat(4)
        .trim()
        .to_owned();
    let llm = model(vec![plan_output(&[(
        title.as_str(),
        &["randomized controlled trial design documentation"],
    )])]);
    let plan = outline_references::plan(&llm, &request, &[], &OutlineProgress::default()).await?;
    assert_eq!(plan.subtopic_titles(), vec![title]);
    Ok(())
}

#[tokio::test]
async fn the_reference_plan_schema_bounds_no_text_but_a_blank_one() -> Result<()> {
    let request = request(LearningCourseDepth::Focused);
    let llm = model(vec![plan_output(&[(
        "Randomized comparison",
        &["randomized controlled trial design documentation"],
    )])]);
    let recording = crate::content_caps_tests::Recording::new(&llm);
    outline_references::plan(&recording, &request, &[], &OutlineProgress::default()).await?;
    let schemas = recording.schemas_naming("subtopics");
    assert!(!schemas.is_empty(), "the plan's schema was sent");
    for schema in &schemas {
        let caps = crate::content_caps_tests::text_caps(schema);
        assert!(caps.is_empty(), "{caps:?}");
    }
    Ok(())
}

/// A course made from a document the user chose and a page the outline's own
/// research found, both short.
async fn a_choice_and_a_find() -> Result<(sqlx::SqlitePool, String)> {
    let pool = crate::tests::pool().await?;
    let repo = LearningRepository::new(pool.clone());
    let web = ResearchWeb::new(&[ADDITIONAL]);
    let llm = model(vec![
        plan_output(&[("Random assignment", &["random assignment confounding"])]),
        select_all(1),
        candidate(),
        no_links(),
        clean(),
    ]);
    let program = generate_online(&repo, &llm, &web, vec![original_source()]).await?;
    Ok((pool, program.summary.id))
}

#[tokio::test]
async fn a_short_source_the_user_chose_for_the_course_is_not_swept() -> Result<()> {
    let (pool, program_id) = a_choice_and_a_find().await?;

    let refused = crate::source_library::LearningSourceLibraryRepository::new(
        pool.clone(),
    )
    .sweep_admission(&program_id)
    .await?;

    let titles: Vec<&str> = refused.iter().map(|page| page.title.as_str()).collect();
    assert!(!titles.contains(&"Brief reference overview"), "{refused:?}");
    let held = LearningRepository::new(pool)
        .verification_sources(&program_id)
        .await?;
    assert!(held
        .iter()
        .any(|source| source.title == "Brief reference overview"));
    Ok(())
}

#[tokio::test]
async fn a_short_page_the_outline_research_found_is_still_swept() -> Result<()> {
    let (pool, program_id) = a_choice_and_a_find().await?;

    let refused = crate::source_library::LearningSourceLibraryRepository::new(
        pool.clone(),
    )
    .sweep_admission(&program_id)
    .await?;

    let found: Vec<_> = refused
        .iter()
        .filter(|page| page.title == "Research reference")
        .collect();
    assert_eq!(found.len(), 1, "{refused:?}");
    assert!(
        found[0].reason.starts_with("too short"),
        "{}",
        found[0].reason
    );
    let held = LearningRepository::new(pool)
        .verification_sources(&program_id)
        .await?;
    assert!(held
        .iter()
        .all(|source| source.title != "Research reference"));
    Ok(())
}
