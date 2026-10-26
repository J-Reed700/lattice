//! Pass B reads the course page by page (course evidence index §5.2–§5.4):
//! one fused list per reach, consecutive pages sized by the budget and the
//! window, ending at the last passage on the claim's subject, then web
//! research.
use super::*;
use std::collections::HashSet;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

pub(super) const CLAIM: &str = "Mineral alpha shows property beta under pressure.";
/// States the claim. It ends a passage that mentions fewer of the claim's
/// words than the rest, so it ranks last on the claim's subject.
pub(super) const MARKER: &str = "Laboratory records confirm the beta behaviour of the sample.";
/// Says otherwise about the claim.
pub(super) const CONTRARY: &str = "Field reports find that the beta behaviour stops once loaded.";

/// A small window, so a page holds a few passages. Supports a claim when it
/// is shown the marker passage, contradicts it when shown the contrary one,
/// and records what each comparison saw and each request it served.
pub(super) struct PagingModel {
    inner: Model,
    window: usize,
    pub(super) checks: Mutex<Vec<Vec<String>>>,
    /// The claim each comparison was asked about.
    pub(super) claims: Mutex<Vec<String>>,
    /// Requests carrying claim checks.
    pub(super) requests: AtomicUsize,
    /// How many comparisons each of those requests carried, in order.
    pub(super) request_sizes: Mutex<Vec<usize>>,
    /// The pace the backend reports, if any.
    pace: Option<lattice_core::application::ports::llm_port::Pace>,
    /// Output tokens it reports per answer.
    answer_tokens: u64,
    /// A host whose pages it judges a vendor's institutional documentation
    /// when asked for a source's standing.
    institutional_host: Option<&'static str>,
    /// Standing judgements it was asked for on that host.
    pub(super) standing_requests: AtomicUsize,
}

impl PagingModel {
    pub(super) fn new(window: usize) -> Self {
        Self {
            inner: Model::new(),
            window,
            checks: Mutex::new(Vec::new()),
            claims: Mutex::new(Vec::new()),
            requests: AtomicUsize::new(0),
            request_sizes: Mutex::new(Vec::new()),
            pace: None,
            answer_tokens: 0,
            institutional_host: None,
            standing_requests: AtomicUsize::new(0),
        }
    }

    /// Judges pages on `host` a vendor's institutional documentation.
    pub(super) fn judging_institutional(mut self, host: &'static str) -> Self {
        self.institutional_host = Some(host);
        self
    }

    /// A backend that reports its pace and `answer_tokens` per answer, so a
    /// job measures its page budget.
    pub(super) fn paced(
        window: usize,
        pace: lattice_core::application::ports::llm_port::Pace,
        answer_tokens: u64,
    ) -> Self {
        Self {
            pace: Some(pace),
            answer_tokens,
            ..Self::new(window)
        }
    }

    async fn respond(&self, prompt: &str) -> Result<String> {
        if let Some(host) = self.institutional_host {
            if prompt.contains("Source signals and content evidence") && prompt.contains(host) {
                self.standing_requests.fetch_add(1, Ordering::SeqCst);
                return Ok(serde_json::json!({
                    "standing": "institutional",
                    "publisher": { "name": "Example Vendor", "kind": "vendor" },
                    "editorial": "editorial",
                    "nature": "documentation",
                    "flags": [],
                    "reasons": [{ "code": "publisher_metadata", "text": "The vendor's own documentation." }],
                })
                .to_string());
            }
        }
        if let Some(rest) = prompt.strip_prefix("Source passages:\n") {
            let (shown, claim) = rest.split_once("\n\nClaim: ").unwrap_or_default();
            self.claims
                .lock()
                .unwrap()
                .push(claim.split("\n\n").next().unwrap_or_default().to_owned());
            let passages: Vec<String> = shown
                .split("[passage-")
                .skip(1)
                .map(|part| {
                    part.split_once('\n')
                        .map_or("", |(_, text)| text)
                        .to_owned()
                })
                .collect();
            let found = passages.iter().position(|text| text.contains(MARKER));
            let against = passages.iter().position(|text| text.contains(CONTRARY));
            self.checks.lock().unwrap().push(passages);
            return Ok(match (found, against) {
                (Some(index), _) => format!(
                    "supported\nReason: The records state it.\nSource passage: passage-{index}"
                ),
                (None, Some(index)) => format!(
                    "contradicted\nReason: The reports say otherwise.\nSource passage: passage-{index}"
                ),
                _ => "unsupported\nReason: No passage states this.\nSource passage: none".into(),
            });
        }
        self.inner.respond(prompt).await
    }
}

#[async_trait::async_trait]
impl LLMPort for PagingModel {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        let prompts = fixture_prompts(request);
        let comparisons = prompts
            .iter()
            .filter(|prompt| prompt.starts_with("Source passages:\n"))
            .count();
        if comparisons > 0 {
            self.requests.fetch_add(1, Ordering::SeqCst);
            self.request_sizes.lock().unwrap().push(comparisons);
        }
        let mut replies = Vec::new();
        for prompt in prompts {
            replies.push(self.respond(&prompt).await?);
        }
        let mut response = fixture_completion(request, replies);
        response.output_tokens = self.answer_tokens * comparisons as u64;
        Ok(response)
    }
    fn pace(&self) -> Option<lattice_core::application::ports::llm_port::Pace> {
        self.pace
    }
    fn model_name(&self) -> &str {
        "paging-fixture"
    }
    fn count_tokens(&self, text: &str) -> usize {
        text.len() / 4
    }
    fn max_context_tokens(&self) -> usize {
        self.window
    }
    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
}

pub(super) fn reference(id: String, excerpt: String) -> LearningSourceDto {
    LearningSourceDto {
        title: format!("Reference {id}"),
        id,
        url: None,
        excerpt,
        acquired_at: 1,
    }
}

/// A passage about the claim's subject that does not state the claim, then
/// filler on nothing the claim names.
pub(super) fn related_text(index: usize) -> String {
    format!(
        "Mineral alpha notes {index}: property beta was measured, pressure varied. {}",
        "Field measurements continue. ".repeat(70)
    )
}

pub(super) fn related(index: usize) -> LearningSourceDto {
    reference(format!("related-{index}"), related_text(index))
}

/// Twelve related sources and the marker's, whose passage mentions three of
/// the claim's words where the others mention five: on the subject, last.
/// Its own lines are its alone, so any stretch of it names it.
pub(super) fn marker_collection() -> Vec<LearningSourceDto> {
    let mut sources: Vec<_> = (0..12).map(related).collect();
    let lines: String = (0..40)
        .map(|line| format!("Marker archive line {line} is filed. "))
        .collect();
    sources.push(reference(
        "marker".into(),
        format!("Mineral alpha notes 99: property was logged. {lines}{MARKER}"),
    ));
    sources
}

/// Pass B read to its lists' end whatever the pages cost, as the
/// page-recall eval's ceiling reads it: the walk these tests watch.
pub(super) const LIST_END: Paging = Paging {
    reaches: REACHES,
    depth: Depth::ListEnd,
};

pub(super) fn inventory(statements: &[String]) -> Inventory {
    Inventory {
        units: vec![UnitClaims {
            index: 0,
            claims: statements
                .iter()
                .map(|statement| Claim {
                    quote: statement.clone(),
                    statement: statement.clone(),
                })
                .collect(),
            non_factual_reason: String::new(),
        }],
    }
}

#[tokio::test]
async fn a_claim_supported_only_past_its_first_page_is_found_on_a_later_one() -> Result<()> {
    let references = ReferenceCollection::lexical(&marker_collection())?;
    let model = PagingModel::new(6_000);
    let first = evidence_for(&model, CLAIM, 0, &references, &[]).await?;
    assert!(!first.is_empty());
    assert!(
        first.iter().all(|passage| !passage.text.contains(MARKER)),
        "the fixture must put the support past page 1"
    );

    let (results, activity) =
        crate::features::learning::lesson_progress::observed(evidence_checks::check_with_paging(
            &model,
            &references,
            &inventory(&[CLAIM.into()]),
            &[],
            &ClaimChecks::default(),
            (&Citations::default(), LIST_END),
        ))
        .await;
    let results = results?;

    let finding = &results[0].2;
    assert_eq!(finding.verdict, ClaimVerdict::Supported);
    assert_eq!(finding.two_pass.pass, Some(Pass::Corroboration));
    assert!(finding.two_pass.page.is_some_and(|page| page > 1));
    assert_eq!(finding.two_pass.reach, Some(Reach::Course));
    // The section cites nothing, so the support is a citation it lacks.
    let found = finding.two_pass.supported_by.as_ref().unwrap();
    assert_eq!(found.source_id, "marker");
    assert!(found.cited.is_empty());
    assert_eq!(
        (
            activity.checks_completed,
            activity.checks_total,
            activity.checks_unresolved
        ),
        (1, 1, 0),
        "The later page's support resolves the claim without a second check"
    );
    assert_eq!(activity.deeper_evidence, None);
    assert!(finding.evidence.iter().any(|p| p.text.contains(MARKER)));
    let checks = model.checks.lock().unwrap();
    assert!(checks.len() >= 2, "page 1, then at least one more");
    // No page repeats a passage an earlier page showed.
    let shown: Vec<&String> = checks.iter().flatten().collect();
    let unique: HashSet<&String> = shown.iter().copied().collect();
    assert_eq!(shown.len(), unique.len());
    Ok(())
}

/// Twelve related sources, then ten that share only a function word with
/// the claim.
fn subject_then_shelf() -> Vec<LearningSourceDto> {
    let mut sources: Vec<_> = (0..12).map(related).collect();
    sources.extend((0..10).map(|index| {
        reference(
            format!("function-{index}"),
            format!(
                "Item {index} sits under the shelf. {}",
                "Unrelated filler text. ".repeat(80)
            ),
        )
    }));
    sources
}

#[tokio::test]
async fn the_list_ends_at_its_last_passage_on_the_subject_and_the_claim_then_goes_to_research(
) -> Result<()> {
    let references = ReferenceCollection::lexical(&subject_then_shelf())?;
    let subject = references.subject_of(CLAIM).await?;
    let list = references
        .claim_list(
            CLAIM,
            &crate::features::learning::reference_collection::Scope::All,
            &[],
        )
        .await?;
    assert_eq!(
        list.passages.len(),
        12,
        "one widened passage per related source"
    );
    assert!(list
        .passages
        .last()
        .is_some_and(|passage| subject.mentioned_in(&passage.text)));
    assert!(list
        .passages
        .iter()
        .all(|passage| passage.source_id.starts_with("related-")));

    let model = PagingModel::new(6_000);
    let results = evidence_checks::check_with_paging(
        &model,
        &references,
        &inventory(&[CLAIM.into()]),
        &[],
        &ClaimChecks::default(),
        (&Citations::default(), LIST_END),
    )
    .await?;

    let finding = &results[0].2;
    assert_eq!(finding.verdict, ClaimVerdict::Unsupported);
    assert!(
        finding.wants_research(),
        "past the list's end: web research"
    );
    let checks = model.checks.lock().unwrap();
    assert!(checks.len() >= 2, "the list takes more than one page");
    // Every page was read, up to the list's end and no further, and no round
    // that settled nothing stopped the walk early.
    let read: Vec<&String> = checks.iter().flatten().collect();
    assert_eq!(read.len(), 12);
    assert!(read.iter().all(|text| !text.contains("shelf")));
    Ok(())
}

#[tokio::test]
async fn a_course_that_cannot_tell_its_passages_apart_is_read_whole() -> Result<()> {
    // Every passage holds every word of the claim: none is more on the
    // subject than another, so the whole ranking is on it.
    let sources: Vec<_> = (0..3)
        .map(|index| {
            reference(
                format!("same-{index}"),
                "Mineral alpha shows property beta under pressure.".into(),
            )
        })
        .collect();
    let references = ReferenceCollection::lexical(&sources)?;
    let all = crate::features::learning::reference_collection::Scope::All;
    let list = references.claim_list(CLAIM, &all, &[]).await?;
    assert_eq!(list.passages.len(), 3);
    // A claim none of whose words is in the course has nothing to read.
    let invented = "OrderWidget CartFactory SkuHelper.";
    assert!(references
        .claim_list(invented, &all, &[])
        .await?
        .passages
        .is_empty());
    Ok(())
}

#[tokio::test]
async fn a_counter_query_joins_the_ranking_only_for_a_claim_with_a_value() -> Result<()> {
    let references = ReferenceCollection::lexical(&marker_collection())?;
    let all = crate::features::learning::reference_collection::Scope::All;
    assert!(!references.claim_list(CLAIM, &all, &[]).await?.counter);
    let valued = "Mineral alpha shows property beta under 7 units of pressure.";
    assert!(references.claim_list(valued, &all, &[]).await?.counter);
    Ok(())
}

#[test]
fn a_page_never_exceeds_its_budget_while_it_fits_the_window() {
    let model = PagingModel::new(6_000);
    let passage = |index: usize, chars: usize| EvidencePassage {
        source_id: format!("source-{index}"),
        text: format!("{index} {}", "y".repeat(chars)),
        start_byte: 0,
        end_byte: chars,
        retrieval_kind: "lexical".into(),
        score: 1.0 / (61.0 + index as f64),
    };
    let list: Vec<_> = (0..12)
        .map(|index| passage(index, 2_000 + 100 * index))
        .collect();
    let tokens = |page: &[EvidencePassage]| -> usize {
        page.iter().map(|p| bank_tokens(&model, &p.text)).sum()
    };
    let budget = tokens(&list[..3]) + 10;
    let (page, passed_over) = evidence_checks::page_of(&model, CLAIM, &list, &[], Some(budget));
    assert_eq!(passed_over, 0);
    assert_eq!(page.len(), 3, "the budget's slice, a passage never split");
    assert!(tokens(&page) <= budget);
    assert!(fits_window(&model, CLAIM, &page));
    assert!(
        tokens(&list[..4]) > budget,
        "the next passage would exceed it"
    );
    // Consecutive: the next page starts where this one stopped.
    let (next, _) = evidence_checks::page_of(&model, CLAIM, &list, &page, Some(budget));
    assert_eq!(
        next.first().map(|p| &p.source_id),
        list.get(3).map(|p| &p.source_id)
    );
    // Without a measured pace, the window alone bounds the page.
    let (windowed, _) = evidence_checks::page_of(&model, CLAIM, &list, &[], None);
    assert!(fits_window(&model, CLAIM, &windowed));
    assert!(windowed.len() < list.len());
    assert!(!fits_window(&model, CLAIM, &list[..windowed.len() + 1]));
    // A first passage over the budget is still read, alone.
    let (alone, _) = evidence_checks::page_of(&model, CLAIM, &list, &[], Some(1));
    assert_eq!(alone.len(), 1);
}

#[tokio::test]
async fn first_page_only_stops_pass_b_after_one_page() -> Result<()> {
    let references = ReferenceCollection::lexical(&marker_collection())?;
    let model = PagingModel::new(6_000);
    let results = evidence_checks::check_with_paging(
        &model,
        &references,
        &inventory(&[CLAIM.into()]),
        &[],
        &ClaimChecks::default(),
        (
            &Citations::default(),
            Paging {
                reaches: REACHES,
                depth: Depth::FirstPage,
            },
        ),
    )
    .await?;
    assert_eq!(results[0].2.verdict, ClaimVerdict::Unsupported);
    assert_eq!(
        model.checks.lock().unwrap().len(),
        1,
        "the first page only, though the support is on a later one"
    );
    Ok(())
}

#[tokio::test]
async fn the_three_orders_read_the_lesson_sources_where_each_says() -> Result<()> {
    let mut sources: Vec<_> = (0..12).map(related).collect();
    let archive = "Archive index entry. ".repeat(80);
    sources.push(reference(
        "lesson-source".into(),
        format!("{archive} Mineral alpha pressure readings were archived. {archive}"),
    ));
    // Two sources alike, so their passages rank equal.
    let twin = "Mineral alpha twin note: property beta under pressure was logged.";
    sources.push(reference("twin-a".into(), twin.into()));
    sources.push(reference("twin-b".into(), twin.into()));
    let references = ReferenceCollection::lexical(&sources)?;
    let model = PagingModel::new(6_000);
    // Another section of the lesson cites the lesson source and twin-b.
    let lesson = references.cited_passages(&["Mineral alpha pressure readings were archived."])?;
    let mut lesson_twin = references.cited_passages(&[twin])?;
    lesson_twin.retain(|passage| passage.source_id == "twin-b");
    let lesson_twin = if lesson_twin.is_empty() {
        references.source_passages("twin-b", &[(0, twin.len())])?
    } else {
        lesson_twin
    };
    let cited = [lesson.clone(), lesson_twin].concat();
    let walk = |order: &'static [Reach]| {
        let cited = cited.clone();
        let references = &references;
        let model = &model;
        async move {
            walk_pages(
                model,
                references,
                (0, CLAIM),
                (Vec::new(), cited),
                order,
                None,
            )
            .await
        }
    };
    let page_of = |pages: &[WalkedPage], source: &str| {
        pages
            .iter()
            .position(|page| page.passages.iter().any(|p| p.source_id == source))
    };
    let order = |name: &str| ORDERS.iter().find(|(n, _)| *n == name).unwrap().1;

    let relevance = walk(order("relevance")).await?;
    assert!(relevance.iter().all(|page| page.reach == Reach::Course));
    assert!(
        page_of(&relevance, "lesson-source").is_some_and(|page| page > 0),
        "by relevance alone the lesson's source is past page 1"
    );

    let lesson_first = walk(order("lesson-sources")).await?;
    assert_eq!(lesson_first[0].reach, Reach::LessonSources);
    assert!(lesson_first[0]
        .passages
        .iter()
        .all(|p| p.source_id == "lesson-source" || p.source_id == "twin-b"));
    assert_eq!(page_of(&lesson_first, "lesson-source"), Some(0));
    assert!(lesson_first
        .iter()
        .skip_while(|page| page.reach == Reach::LessonSources)
        .all(|page| page.reach == Reach::Course
            && page.passages.iter().all(|p| p.source_id != "lesson-source")));

    let ties = walk(order("tie-break")).await?;
    let sequence: Vec<&str> = ties
        .iter()
        .flat_map(|page| page.passages.iter().map(|p| p.source_id.as_str()))
        .collect();
    let at = |source: &str| sequence.iter().position(|id| *id == source).unwrap();
    assert!(
        at("twin-b") < at("twin-a"),
        "equal scores: the lesson's first"
    );
    Ok(())
}

#[tokio::test]
async fn a_term_in_no_passage_neither_counts_nor_lowers_the_bar() {
    let one = |id: &str, text: &str| reference(id.into(), text.into());
    let mut sources = vec![
        one("a1", "Alpha decays slowly."),
        one("a2", "Alpha is measured hourly."),
    ];
    sources.extend((0..5).map(|i| one(&format!("b{i}"), &format!("Beta reading {i}."))));
    let references = ReferenceCollection::lexical(&sources).unwrap();
    // Three invented names, in no passage, would pull the median to zero and
    // leave only themselves distinctive; excluded, "alpha" is distinctive.
    let claim = "OrderWidget CartFactory SkuHelper keep alpha above beta.";
    let subject = references.subject_of(claim).await.unwrap();
    assert!(subject.mentioned_in("A note about alpha."));
    assert!(!subject.mentioned_in("A note about beta."));
    assert!(!subject.mentioned_in("OrderWidget CartFactory SkuHelper"));
}

#[tokio::test]
async fn research_found_for_a_claim_leads_its_first_page_and_its_walk() -> Result<()> {
    let sources: Vec<_> = (0..12).map(related).collect();
    let references = ReferenceCollection::lexical(&sources)?;
    let model = PagingModel::new(6_000);
    let claims = inventory(&[CLAIM.into()]);
    let mut retained = ClaimChecks::default();
    let first = evidence_checks::check(&model, &references, &claims, &[], &retained).await?;
    assert_eq!(first[0].2.verdict, ClaimVerdict::Unsupported);
    let findings: Vec<_> = first.iter().map(|result| result.2.clone()).collect();
    for (_, key, finding, _) in first {
        retained.record(key, &finding);
    }
    let checks_before = model.checks.lock().unwrap().len();

    // Research saves a source that states the claim.
    let mut expanded_sources = sources.clone();
    let archive = "Archive index entry. ".repeat(80);
    expanded_sources.push(reference(
        "marker".into(),
        format!("{archive} Mineral alpha property records: {MARKER} {archive}"),
    ));
    let expanded = ReferenceCollection::lexical(&expanded_sources)?;
    evidence_selection::record_research_scope(&expanded, &findings, &retained).await?;
    let second = evidence_checks::check(&model, &expanded, &claims, &[], &retained).await?;

    assert_eq!(second[0].2.verdict, ClaimVerdict::Supported);
    assert_eq!(second[0].2.two_pass.reach, Some(Reach::Researched));
    let checks = model.checks.lock().unwrap();
    let recheck = checks
        .get(checks_before)
        .expect("new evidence is checked again, not reused");
    assert!(
        recheck[0].contains(MARKER),
        "what research found is read first"
    );
    assert!(fits_window(&model, CLAIM, &second[0].2.evidence));
    Ok(())
}

/// A judge that fails every check: with an outage, or with an unusable
/// answer.
struct BrokenJudge {
    inner: Model,
    outage: bool,
}

#[async_trait::async_trait]
impl LLMPort for BrokenJudge {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        let (_, user) = crate::features::learning::lessons::structured_call::task_messages(request);
        if user.contains("Claim: ") || user.contains("\"sourcePassages\"") {
            if self.outage {
                return Err(AppError::ServiceNotAvailable("503: loading model".into()));
            }
            return Ok(CompletionResponse::from_text("no verdict here"));
        }
        self.inner.complete(request).await
    }
    fn model_name(&self) -> &str {
        "broken-judge"
    }
    fn max_context_tokens(&self) -> usize {
        32_000
    }
    fn count_tokens(&self, text: &str) -> usize {
        text.len() / 4
    }
    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
}

#[tokio::test]
async fn an_unusable_check_leaves_the_claim_with_its_cause_and_an_outage_defers_it() -> Result<()> {
    use lattice_core::shared::runtime::jobs::JobFailure;
    let sources: Vec<_> = (0..3).map(related).collect();
    let references = ReferenceCollection::lexical(&sources)?;
    let references = &references;
    let check = |llm: BrokenJudge| async move {
        evidence_checks::check(
            &llm,
            references,
            &inventory(&[CLAIM.into()]),
            &[],
            &ClaimChecks::default(),
        )
        .await
    };
    // Deterministic: the claim is left unverified with its cause, which the
    // claim ledger queues for the user as "could not judge" (§4), never a
    // job failure.
    let results = check(BrokenJudge {
        inner: Model::new(),
        outage: false,
    })
    .await?;
    assert_eq!(results[0].2.verdict, ClaimVerdict::Unverified);
    assert_eq!(results[0].2.statement, CLAIM);
    assert!(
        results[0].2.reason.contains("unusable"),
        "{}",
        results[0].2.reason
    );
    // Transient: the outage fails the check and the job defers it.
    let error = check(BrokenJudge {
        inner: Model::new(),
        outage: true,
    })
    .await
    .err()
    .unwrap();
    assert!(JobFailure::is_transient(&error), "{error}");
    Ok(())
}

#[tokio::test]
async fn the_checker_reads_a_source_index_as_its_title() -> Result<()> {
    let sources = marker_collection();
    let references = ReferenceCollection::lexical(&sources)?;
    let model = PagingModel::new(6_000);
    let citations = Citations::new(
        HashMap::from([(0, references.cited_passages(&[MARKER])?)]),
        HashMap::from([(0, "marker".to_owned())]),
    );
    let statement = format!("Source 0 reports that {CLAIM}");
    let results = evidence_checks::check_with_citations(
        &model,
        &references,
        &inventory(std::slice::from_ref(&statement)),
        &[],
        &ClaimChecks::default(),
        &citations,
    )
    .await?;
    assert_eq!(
        results[0].2.statement, statement,
        "the finding keeps the lesson's words"
    );
    let claims = model.claims.lock().unwrap();
    assert!(
        claims[0].starts_with("Source \"Reference marker\" reports that"),
        "{}",
        claims[0]
    );
    assert!(claims.iter().all(|claim| !claim.contains("Source 0")));
    Ok(())
}

#[test]
fn only_a_known_source_index_is_labelled() -> Result<()> {
    let sources = marker_collection();
    let references = ReferenceCollection::lexical(&sources)?;
    let citations = Citations::new(HashMap::new(), HashMap::from([(1, "related-1".to_owned())]));
    assert_eq!(
        citations.label(
            "As source 1 notes, Source 7 and Resource 1 differ; see Source 1.",
            &references
        ),
        "As source \"Reference related-1\" notes, Source 7 and Resource 1 differ; see Source \"Reference related-1\"."
    );
    Ok(())
}

#[tokio::test]
async fn a_verdict_the_challenge_overturned_is_checked_again_under_the_trust_policy() -> Result<()>
{
    use crate::features::learning::lesson_drafts;
    let pool = crate::features::learning::tests::pool().await?;
    let (repo, job, lesson) = batch_scheduler_tests::setup(&pool).await?;
    let sources = marker_collection();
    let references = ReferenceCollection::lexical(&sources)?;
    let model = PagingModel::new(6_000);
    let citations = Citations::new(
        HashMap::from([(0, references.cited_passages(&[MARKER])?)]),
        HashMap::new(),
    );
    let claims = inventory(&[CLAIM.into()]);
    // Pass A's page: the passage the section cites.
    let mut evidence = citations.of(0).to_vec();
    canonicalize_evidence(&mut evidence);
    // The same comparison, keyed as the evidence challenge keyed it, where
    // the challenge had overturned its approval.
    let comparison = ClaimChecks::key(0, &claims.units[0].claims[0], &evidence);
    let earlier = format!(
        "claim-v1:{}",
        digest(
            &json!({"policy": "lesson-evidence-v20", "model": model.model_name(),
                "context": model.max_context_tokens(), "comparison": comparison,
                "challenge": "challenge-contrary-evidence-v1"})
            .to_string()
        )
    );
    // A receipt names the trust policy that weighs its verdict.
    let current = claim_receipt_key(&model, &comparison);
    assert_ne!(current, earlier);
    assert_eq!(
        current,
        format!(
            "claim-v1:{}",
            digest(
                &json!({"policy": POLICY, "model": model.model_name(),
                    "context": model.max_context_tokens(), "comparison": comparison,
                    "trust": lattice_core::domain::provenance::TRUST_POLICY_VERSION})
                .to_string()
            )
        )
    );
    lesson_drafts::run(&repo, &job, &lesson, async {
        lesson_drafts::record_checkpoint(
            &earlier,
            serde_json::to_value(ClaimReceipt {
                verdict: ClaimVerdict::Unsupported,
                reason: "Independent review raised unresolved evidence questions.".into(),
                supporting_quote: None,
                interpretation_policy: Some(
                    lattice_core::application::services::claim_verification::STRICT_INTERPRETATION_POLICY
                        .into(),
                ),
                shown: None,
            })?,
        )
        .await?;
        let results = evidence_checks::check_with_citations(
            &model,
            &references,
            &claims,
            &[],
            &ClaimChecks::default(),
            &citations,
        )
        .await?;
        assert_eq!(results[0].2.verdict, ClaimVerdict::Supported);
        assert_eq!(
            results[0].2.two_pass.pass,
            Some(Pass::Attribution),
            "the citation supports it"
        );
        assert_eq!(
            model.checks.lock().unwrap().len(),
            2,
            "checked again, with pass B's first page beside it"
        );
        assert!(
            lesson_drafts::checkpoint(&current).await?.is_some(),
            "the new verdict is saved under the trust policy's key"
        );
        Ok(())
    })
    .await
}

/// A backend whose tokenizer counts the fixture's text half again denser than
/// the calibrated estimate: a page the estimate fits is refused by the exact
/// count, with its numbers, as the scheduler refuses it.
struct DenseModel {
    paging: PagingModel,
    refused: std::sync::atomic::AtomicUsize,
}

#[async_trait::async_trait]
impl LLMPort for DenseModel {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        use lattice_core::application::ports::llm_port::CompletionInput;
        let estimate: usize = request
            .input
            .iter()
            .map(|item| match item {
                CompletionInput::Message { content, .. } => self.count_tokens(content),
                _ => 0,
            })
            .sum();
        let exact = estimate * 3 / 2;
        let window = self.max_context_tokens();
        if exact >= window {
            self.refused
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            return Err(crate::features::llm::scheduler::rejection::window_refusal(
                self.model_name(),
                crate::features::llm::scheduler::rejection::RequestSize {
                    prompt_tokens: exact,
                    counted_exactly: true,
                    ..Default::default()
                },
                window,
            ));
        }
        self.paging.complete(request).await
    }
    fn model_name(&self) -> &str {
        "dense-fixture"
    }
    fn count_tokens(&self, text: &str) -> usize {
        self.paging.count_tokens(text)
    }
    fn max_context_tokens(&self) -> usize {
        self.paging.max_context_tokens()
    }
    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
}

#[tokio::test]
async fn a_page_counted_past_the_window_is_sent_again_with_fewer_passages() -> Result<()> {
    let sources: Vec<_> = (0..14).map(related).collect();
    let references = ReferenceCollection::lexical(&sources)?;
    let model = DenseModel {
        paging: PagingModel::new(6_000),
        refused: Default::default(),
    };
    let first = evidence_for(&model, CLAIM, 0, &references, &[]).await?;
    let results = evidence_checks::check_with_paging(
        &model,
        &references,
        &inventory(&[CLAIM.into()]),
        &[],
        &ClaimChecks::default(),
        (&Citations::default(), LIST_END),
    )
    .await?;

    assert!(model.refused.load(std::sync::atomic::Ordering::SeqCst) >= 1);
    let checks = model.paging.checks.lock().unwrap();
    let saw = |check: &[String], text: &str| check.iter().any(|seen| seen.contains(text));
    // The refused page was judged, on fewer passages: the claim has a verdict.
    assert_eq!(results[0].2.verdict, ClaimVerdict::Unsupported);
    assert!(
        checks[0].len() < first.len(),
        "page 1 was sent again, shorter"
    );
    // The finding names only the passages the judge read.
    assert!(!results[0].2.evidence.is_empty());
    assert!(results[0]
        .2
        .evidence
        .iter()
        .all(|passage| saw(&checks[0], &passage.text)));
    // A passage cut for size was not shown, so the next page carries it.
    let cut: Vec<_> = first
        .iter()
        .filter(|passage| !saw(&checks[0], &passage.text))
        .collect();
    assert!(!cut.is_empty());
    assert!(checks.len() >= 2, "page 1, then the next page of the list");
    assert!(cut.iter().any(|passage| saw(&checks[1], &passage.text)));
    // No page repeats a passage an earlier page showed.
    let shown: Vec<&String> = checks.iter().flatten().collect();
    let unique: HashSet<&String> = shown.iter().copied().collect();
    assert_eq!(shown.len(), unique.len());
    Ok(())
}

/// [`DenseModel`] with its tokenizer reachable, as llama-server's is: it
/// counts every text half again denser than the calibrated estimate, and
/// counts what it is sent the same way before it refuses a prompt that
/// fills the window.
struct CountingDenseModel {
    paging: PagingModel,
    refused: AtomicUsize,
    /// The backend's count of each request it served.
    sent: Mutex<Vec<usize>>,
}

impl CountingDenseModel {
    fn new(window: usize) -> Self {
        Self {
            paging: PagingModel::new(window),
            refused: AtomicUsize::new(0),
            sent: Mutex::new(Vec::new()),
        }
    }

    fn exact(&self, text: &str) -> usize {
        self.paging.count_tokens(text) * 3 / 2
    }
}

#[async_trait::async_trait]
impl LLMPort for CountingDenseModel {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        use lattice_core::application::ports::llm_port::CompletionInput;
        let text = request
            .input
            .iter()
            .filter_map(|item| match item {
                CompletionInput::Message { content, .. } => Some(content.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n");
        let counted = self.exact(&text);
        let window = self.max_context_tokens();
        if counted >= window {
            self.refused.fetch_add(1, Ordering::SeqCst);
            return Err(crate::features::llm::scheduler::rejection::window_refusal(
                self.model_name(),
                crate::features::llm::scheduler::rejection::RequestSize {
                    prompt_tokens: counted,
                    counted_exactly: true,
                    ..Default::default()
                },
                window,
            ));
        }
        self.sent.lock().unwrap().push(counted);
        self.paging.complete(request).await
    }
    fn model_name(&self) -> &str {
        "counting-dense-fixture"
    }
    fn count_tokens(&self, text: &str) -> usize {
        self.paging.count_tokens(text)
    }
    async fn count_tokens_exact(&self, text: &str) -> Result<usize> {
        Ok(self.exact(text))
    }
    fn counts_tokens_exactly(&self) -> bool {
        true
    }
    fn max_context_tokens(&self) -> usize {
        self.paging.max_context_tokens()
    }
    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
}

/// The single-claim judge's checker.
fn strict_checker(
    model: &dyn LLMPort,
) -> lattice_core::application::services::claim_verification::ClaimChecker<'_> {
    lattice_core::application::services::claim_verification::ClaimChecker::new(
        model,
        lattice_core::application::ports::llm_port::SamplingOverride::deterministic(),
        512,
        lattice_core::application::services::claim_verification::CheckPolicy::Strict,
    )
}

/// The course eval of 2026-10-21 sized its claim checks by the estimate,
/// which its pages ran past by thousands of tokens: nearly every check was
/// refused and asked again. A check sized by the count of what it sends is
/// never refused, and keeps its answer's room.
#[tokio::test]
async fn a_claim_check_is_sized_by_the_count_of_what_it_sends() -> Result<()> {
    use lattice_core::application::services::claim_verification::ANSWER_ROOM_TOKENS;
    // A window that holds the check's instructions, the answer's room and
    // a few passages, by the exact count.
    let probe = CountingDenseModel::new(1_000_000);
    let instructions = strict_checker(&probe)
        .located_prompt_tokens_measured(CLAIM, &[])
        .await;
    let model = CountingDenseModel::new(instructions + ANSWER_ROOM_TOKENS + 2_000);
    let window = model.max_context_tokens();
    // As many passages as the estimate says fit.
    let mut passages: Vec<String> = Vec::new();
    loop {
        let mut grown = passages.clone();
        grown.push(format!("{} {}", grown.len(), "y".repeat(1_200)));
        if !strict_checker(&model).located_fits(CLAIM, &grown) {
            break;
        }
        passages = grown;
    }
    assert!(passages.len() > 1);
    assert!(
        strict_checker(&model)
            .located_prompt_tokens_measured(CLAIM, &passages)
            .await
            + ANSWER_ROOM_TOKENS
            > window,
        "the estimate undercounts the page"
    );

    let ranking: Vec<usize> = (0..passages.len()).collect();
    let judged = judge_for_publication(&model, CLAIM, &passages, &ranking).await;

    assert_eq!(
        model.refused.load(Ordering::SeqCst),
        0,
        "no check was refused for its size"
    );
    let shown = shown_of(&passages, &judged.shown);
    assert!(!shown.is_empty() && shown.len() < passages.len());
    let sent = model.sent.lock().unwrap().clone();
    let first = *sent.first().expect("the check was sent");
    assert_eq!(
        first,
        strict_checker(&model)
            .located_prompt_tokens_measured(CLAIM, &shown)
            .await,
        "the count before sending is the backend's count of what was sent"
    );
    assert!(
        first + ANSWER_ROOM_TOKENS <= window,
        "the answer keeps its room"
    );
    Ok(())
}

fn ranked_passage(kind: &str, score: f64, text: &str) -> EvidencePassage {
    EvidencePassage {
        source_id: format!("source-{text}"),
        text: text.into(),
        start_byte: 0,
        end_byte: text.len(),
        retrieval_kind: kind.into(),
        score,
    }
}

#[test]
fn a_page_loses_its_least_needed_passages_not_its_last() {
    // In page order, as the judge reads it; cited and researched passages
    // carry no score.
    let page = vec![
        ranked_passage(evidence_selection::RESEARCHED, 0.0, "researched"),
        ranked_passage(CITED, 0.0, "cited"),
        ranked_passage("lexical", 1.0 / 63.0, "third-ranked"),
        ranked_passage("lexical", 1.0 / 61.0, "top-ranked"),
        ranked_passage("lexical", 1.0 / 62.0, "second-ranked"),
    ];
    assert_eq!(ranking(&page), vec![1, 0, 3, 4, 2]);
}

#[test]
fn a_restored_receipt_names_only_the_passages_its_judge_was_shown() {
    let page = vec![
        ranked_passage("lexical", 1.0 / 61.0, "shown"),
        ranked_passage("lexical", 1.0 / 62.0, "cut for size"),
    ];
    let claim = Claim {
        quote: CLAIM.into(),
        statement: CLAIM.into(),
    };
    let finding = ClaimReceipt {
        verdict: ClaimVerdict::Supported,
        reason: "The passage states it.".into(),
        supporting_quote: Some("shown".into()),
        interpretation_policy: Some(
            lattice_core::application::services::claim_verification::STRICT_INTERPRETATION_POLICY
                .into(),
        ),
        shown: Some(vec![0]),
    }
    .finding(0, &claim, &page)
    .unwrap();
    assert_eq!(finding.evidence.len(), 1);
    assert_eq!(finding.evidence[0].text, "shown");
}
