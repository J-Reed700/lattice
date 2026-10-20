//! Claim evidence is paged by relevance (claim evidence pages design): the
//! section's citations and the lesson's sources first, rings outward for a
//! claim still unsupported, each page ending where its rankings' scores drop,
//! and never a passage off the claim's subject.
use super::*;
use std::collections::HashSet;
use std::sync::Mutex;

const CLAIM: &str = "Mineral alpha shows property beta under pressure.";
/// Shares one content term with the claim, so it ranks below the others.
const MARKER: &str = "Laboratory records confirm the beta behaviour of the sample.";

/// The ring fixtures' claim: none of its words is a function word or in
/// [`FILLER`].
const QUARTZ: &str = "Quartz zeta resists acid etching.";
/// Filler with none of the claims' terms, thirty characters a unit.
const FILLER: &str = "Survey logs are kept on file. ";

/// A small window, so a page holds a few passages. Supports a claim only
/// when it is shown the marker passage, contradicts it when shown its
/// `contrary` text, and records what each check saw, in the order read.
struct PagingModel {
    inner: Model,
    window: usize,
    checks: Mutex<Vec<Vec<String>>>,
    /// The claim each check was asked about.
    claims: Mutex<Vec<String>>,
    contrary: Option<&'static str>,
}

impl PagingModel {
    fn new(window: usize) -> Self {
        Self {
            inner: Model::new(),
            window,
            checks: Mutex::new(Vec::new()),
            claims: Mutex::new(Vec::new()),
            contrary: None,
        }
    }

    async fn respond(&self, prompt: &str) -> Result<String> {
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
            let contrary = self
                .contrary
                .and_then(|contrary| passages.iter().position(|text| text.contains(contrary)));
            self.checks.lock().unwrap().push(passages);
            return Ok(match (found, contrary) {
                (Some(index), _) => format!(
                    "supported\nReason: The records state it.\nSource passage: passage-{index}"
                ),
                (None, Some(index)) => format!(
                    "contradicted\nReason: The trials state otherwise.\nSource passage: passage-{index}"
                ),
                (None, None) => {
                    "unsupported\nReason: No passage states this.\nSource passage: none".into()
                }
            });
        }
        self.inner.respond(prompt).await
    }
}

#[async_trait::async_trait]
impl LLMPort for PagingModel {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        let mut replies = Vec::new();
        for prompt in fixture_prompts(request) {
            replies.push(self.respond(&prompt).await?);
        }
        Ok(fixture_completion(request, replies))
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

fn reference(id: String, excerpt: String) -> LearningSourceDto {
    LearningSourceDto {
        title: format!("Reference {id}"),
        id,
        url: None,
        excerpt,
        acquired_at: 1,
    }
}

/// A passage about the claim's subject that does not state the claim.
fn related(index: usize) -> LearningSourceDto {
    reference(
        format!("related-{index}"),
        format!(
            "Mineral alpha notes {index}: property beta was measured, pressure varied. {}",
            "Field measurements continue. ".repeat(70)
        ),
    )
}

/// A source whose `regions` each sit inside one keyword chunk of their own,
/// far enough apart that each widens to a passage of its own: every region
/// is a candidate, ranked by its own words alone.
fn regions(id: &str, regions: &[String]) -> LearningSourceDto {
    // A chunk is 1,200 characters, one every 900: a region of at most 600
    // characters, 300 into a block of 7,200, lies in that block's first chunk
    // alone, and its widened passage never reaches the next region's.
    let pad = |text: &mut String, to: usize| {
        while text.len() < to {
            let missing = to - text.len();
            text.push_str(&FILLER[..missing.min(FILLER.len())]);
        }
    };
    let mut text = String::new();
    for region in regions {
        assert!(region.len() <= 600);
        let block = text.len();
        pad(&mut text, block + 300);
        text.push_str(region);
        pad(&mut text, block + 7_200);
    }
    reference(id.into(), text)
}

/// A region stating the ring claim's terms three times over: it outranks a
/// [`weak`] one by a clear gap. Regions of one kind rank exactly alike.
fn strong(note: usize) -> String {
    format!("Note {note:03}: quartz zeta resists acid etching, quartz zeta resists acid etching, quartz zeta resists acid etching.")
}

fn weak(note: usize) -> String {
    format!("Note {note:03}: quartz zeta resists acid etching.")
}

/// A weak region that also holds the marker the fixture supports.
fn marked(note: usize) -> String {
    format!("Note {note:03}: quartz zeta resists acid etching. {MARKER}")
}

/// A region with none of the claim's terms: what a section cites.
fn cited(note: usize) -> String {
    format!("Note {note:03}: the section cites this record.")
}

fn quartz_inventory() -> Inventory {
    inventory(&[QUARTZ.into()])
}

fn identities(page: &[EvidencePassage]) -> Vec<(String, usize, usize)> {
    page.iter()
        .map(|passage| {
            (
                passage.source_id.clone(),
                passage.start_byte,
                passage.end_byte,
            )
        })
        .collect()
}

fn inventory(statements: &[String]) -> Inventory {
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
async fn a_page_does_not_grow_with_the_window() -> Result<()> {
    let mut sources: Vec<_> = (0..2)
        .map(|index| regions(&format!("strong-{index}"), &[strong(index)]))
        .collect();
    sources.extend((0..4).map(|index| regions(&format!("weak-{index}"), &[weak(10 + index)])));
    let references = ReferenceCollection::lexical(&sources)?;
    let narrow = evidence_for(&PagingModel::new(32_000), QUARTZ, 0, &references, &[]).await?;
    let wide = evidence_for(&PagingModel::new(128_000), QUARTZ, 0, &references, &[]).await?;
    assert_eq!(identities(&narrow), identities(&wide));
    // The page ends at the drop in relevance, not where the room runs out.
    assert_eq!(narrow.len(), 2);
    assert!(narrow
        .iter()
        .all(|passage| passage.source_id.starts_with("strong-")));
    Ok(())
}

#[tokio::test]
async fn no_page_holds_an_off_subject_passage_even_with_room() -> Result<()> {
    // The other sources share only "quartz", which is commoner across the
    // collection than the claim's other terms.
    let mut sources = vec![regions("weak", &[weak(1)])];
    sources.extend((0..5).map(|index| {
        regions(
            &format!("off-{index}"),
            &[format!(
                "Note {:03}: quartz crystals were catalogued.",
                10 + index
            )],
        )
    }));
    let references = ReferenceCollection::lexical(&sources)?;
    assert!(!references.mentions_subject_of(QUARTZ, "Note 010: quartz crystals were catalogued."));
    let model = PagingModel::new(32_000);
    let results = evidence_checks::check(
        &model,
        &references,
        &quartz_inventory(),
        &[],
        &ClaimChecks::default(),
    )
    .await?;
    assert_eq!(results[0].2.verdict, ClaimVerdict::Unsupported);
    let checks = model.checks.lock().unwrap();
    assert_eq!(
        checks.len(),
        1,
        "nothing on the subject is left after page 1"
    );
    assert_eq!(checks[0].len(), 1);
    assert!(checks
        .iter()
        .flatten()
        .all(|text| !text.contains("catalogued")));
    Ok(())
}

#[tokio::test]
async fn page_one_holds_the_section_citation_and_the_lesson_sources_above_the_cut() -> Result<()> {
    // Source A holds the section's citation and a weak passage; source B,
    // which another section cites, a strong one. Six more sources, uncited,
    // are as strong as B.
    let mut sources = vec![
        regions("cited-a", &[cited(1), weak(2)]),
        regions("cited-b", &[strong(3)]),
    ];
    sources.extend((0..6).map(|index| regions(&format!("other-{index}"), &[strong(100 + index)])));
    let references = ReferenceCollection::lexical(&sources)?;
    let section = references.cited_passages(&["Note 001: the section cites this record."])?;
    let elsewhere = references.cited_passages(&["Note 003: quartz zeta"])?;
    let citations = Citations::new(
        HashMap::from([(0, section), (1, elsewhere)]),
        HashMap::new(),
    );
    let model = PagingModel::new(6_000);

    let page = first_page(&model, QUARTZ, 0, &references, &[], &citations).await?;
    assert_eq!(page.len(), 2, "{page:?}");
    assert!(page
        .iter()
        .any(|p| p.retrieval_kind == CITED && p.text.contains("the section cites this record")));
    assert!(page
        .iter()
        .any(|p| p.source_id == "cited-b" && p.text.contains("Note 003")));
    // A's weak passage lies below the cut and waits; the uncited sources
    // wait for ring 2.
    assert!(page.iter().all(|p| !p.text.contains("Note 002")));
    assert!(page.iter().all(|p| !p.source_id.starts_with("other-")));

    evidence_checks::check_with_citations(
        &model,
        &references,
        &quartz_inventory(),
        &[],
        &ClaimChecks::default(),
        &citations,
    )
    .await?;
    let checks = model.checks.lock().unwrap();
    assert_eq!(checks[0].len(), 2);
    assert!(checks[0][0].contains("the section cites this record"));
    assert!(checks[0][1].contains("Note 003"));
    Ok(())
}

#[tokio::test]
async fn rings_run_to_exhaustion_before_the_deeper_pages() -> Result<()> {
    // Three strong sources; one weak source, below the cut on page 1; and a
    // source whose strong passage is on page 1 and whose marked one, as weak
    // as the weak source's, only a deeper page reaches.
    let mut sources: Vec<_> = (0..3)
        .map(|index| regions(&format!("strong-{index}"), &[strong(index)]))
        .collect();
    sources.push(regions("weak", &[weak(10)]));
    sources.push(regions("deep", &[strong(20), marked(21)]));
    let references = ReferenceCollection::lexical(&sources)?;
    let model = PagingModel::new(32_000);
    let (results, activity) =
        crate::features::learning::lesson_progress::observed(evidence_checks::check(
            &model,
            &references,
            &quartz_inventory(),
            &[],
            &ClaimChecks::default(),
        ))
        .await;
    let results = results?;

    assert_eq!(results[0].2.verdict, ClaimVerdict::Supported);
    assert_eq!(
        (
            activity.checks_completed,
            activity.checks_total,
            activity.checks_unresolved
        ),
        (1, 1, 0),
        "The deeper page's support resolves the claim without a second check"
    );
    let checks = model.checks.lock().unwrap().clone();
    assert_eq!(checks.len(), 3, "page 1, the rest of ring 2, a deeper page");
    assert_eq!(checks[0].len(), 4);
    assert!(checks[0]
        .iter()
        .all(|text| !text.contains("Note 010") && !text.contains(MARKER)));
    assert_eq!(checks[1].len(), 1);
    assert!(
        checks[1][0].contains("Note 010"),
        "ring 2 ends before the deeper pages"
    );
    assert_eq!(checks[2].len(), 1);
    assert!(checks[2][0].contains(MARKER));
    // No page repeats a passage an earlier page showed.
    let shown: Vec<&String> = checks.iter().flatten().collect();
    let unique: HashSet<&String> = shown.iter().copied().collect();
    assert_eq!(shown.len(), unique.len());

    // With nothing on the subject left after ring 2, the walk ends there.
    sources.pop();
    let references = ReferenceCollection::lexical(&sources)?;
    let model = PagingModel::new(32_000);
    let results = evidence_checks::check(
        &model,
        &references,
        &quartz_inventory(),
        &[],
        &ClaimChecks::default(),
    )
    .await?;
    assert_eq!(results[0].2.verdict, ClaimVerdict::Unsupported);
    assert_eq!(model.checks.lock().unwrap().len(), 2);
    Ok(())
}

#[tokio::test]
async fn the_group_stop_applies_only_on_the_deeper_pages() -> Result<()> {
    // The lesson cites a strong and a weak source; the support is the best
    // passage of a source it does not cite.
    let sources = vec![
        regions("cited-strong", &[strong(1)]),
        regions("cited-weak", &[weak(2)]),
        regions("uncited", &[marked(3)]),
    ];
    let references = ReferenceCollection::lexical(&sources)?;
    let elsewhere =
        references.cited_passages(&["Note 001: quartz zeta", "Note 002: quartz zeta"])?;
    assert_eq!(elsewhere.len(), 2);
    let citations = Citations::new(HashMap::from([(1, elsewhere)]), HashMap::new());
    let model = PagingModel::new(32_000);
    let results = evidence_checks::check_with_citations(
        &model,
        &references,
        &quartz_inventory(),
        &[],
        &ClaimChecks::default(),
        &citations,
    )
    .await?;
    let checks = model.checks.lock().unwrap();
    // Page 1 is the strong cited source; the weak one, below the cut, is
    // page 2, a round that settles nothing; ring 2 still runs.
    assert_eq!(checks.len(), 3);
    assert!(checks[0].iter().all(|text| text.contains("Note 001")));
    assert!(checks[1].iter().all(|text| text.contains("Note 002")));
    assert!(checks[2][0].contains(MARKER));
    assert_eq!(results[0].2.verdict, ClaimVerdict::Supported);
    Ok(())
}

#[tokio::test]
async fn a_counter_page_finds_another_value_of_the_subject_from_ring_two_on() -> Result<()> {
    const COUNTED: &str = "Quartz zeta resists acid etching for 40 hours.";
    const CONTRARY: &str = "etching for 12 hours";
    // The support query ranks the passage that repeats the claim's number
    // first; the contrary passage lies in the same source, so ring 2 never
    // takes it, and an uncited weak source is ring 2's second page.
    let sources = vec![
        regions(
            "repeats",
            &[
                "Note 001: quartz zeta resists acid etching for 40 hours, quartz zeta resists acid etching for 40 hours.".into(),
                format!("Note 002: quartz zeta resists acid {CONTRARY}."),
            ],
        ),
        regions(
            "other",
            &["Note 003: quartz zeta resists acid etching for 40 hours.".into()],
        ),
    ];
    let references = ReferenceCollection::lexical(&sources)?;
    let page = evidence_for(&PagingModel::new(32_000), COUNTED, 0, &references, &[]).await?;
    assert!(page
        .iter()
        .all(|p| p.retrieval_kind != citations::COUNTER && !p.text.contains(CONTRARY)));

    let model = PagingModel {
        contrary: Some(CONTRARY),
        ..PagingModel::new(32_000)
    };
    let results = evidence_checks::check(
        &model,
        &references,
        &inventory(&[COUNTED.into()]),
        &[],
        &ClaimChecks::default(),
    )
    .await?;
    assert_eq!(results[0].2.verdict, ClaimVerdict::Contradicted);
    let checks = model.checks.lock().unwrap();
    assert_eq!(checks.len(), 2);
    assert!(checks[0].iter().all(|text| !text.contains(CONTRARY)));
    assert!(checks[1].iter().any(|text| text.contains(CONTRARY)));
    assert!(results[0]
        .2
        .evidence
        .iter()
        .any(|p| p.retrieval_kind == citations::COUNTER && p.text.contains(CONTRARY)));
    Ok(())
}

#[test]
fn a_term_in_no_passage_neither_counts_nor_lowers_the_bar() {
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
    assert!(references.mentions_subject_of(claim, "A note about alpha."));
    assert!(!references.mentions_subject_of(claim, "A note about beta."));
    assert!(!references.mentions_subject_of(claim, "OrderWidget CartFactory SkuHelper"));
}

fn marker_collection() -> Vec<LearningSourceDto> {
    let mut sources: Vec<_> = (0..12).map(related).collect();
    sources.push(reference(
        "marker".into(),
        format!("{} {MARKER}", "Archive index entry. ".repeat(80)),
    ));
    sources
}

#[tokio::test]
async fn a_claim_whose_section_cites_its_support_is_settled_on_page_one() -> Result<()> {
    let sources = marker_collection();
    let references = ReferenceCollection::lexical(&sources)?;
    let model = PagingModel::new(6_000);
    // Retrieval alone never shows the support: it shares no distinctive
    // term with the claim.
    let retrieved = evidence_for(&model, CLAIM, 0, &references, &[]).await?;
    assert!(retrieved.iter().all(|p| !p.text.contains(MARKER)));
    let citations = Citations::new(
        HashMap::from([(0, references.cited_passages(&[MARKER])?)]),
        HashMap::new(),
    );

    let results = evidence_checks::check_with_citations(
        &model,
        &references,
        &inventory(&[CLAIM.into()]),
        &[],
        &ClaimChecks::default(),
        &citations,
    )
    .await?;

    assert_eq!(results[0].2.verdict, ClaimVerdict::Supported);
    let checks = model.checks.lock().unwrap();
    assert_eq!(checks.len(), 1, "settled on page 1");
    assert!(
        checks[0][0].contains(MARKER),
        "the cited passage comes first"
    );
    Ok(())
}

#[tokio::test]
async fn another_sections_citations_are_not_added() -> Result<()> {
    let sources = marker_collection();
    let references = ReferenceCollection::lexical(&sources)?;
    let model = PagingModel::new(6_000);
    // Section 1 cites the support; the claim belongs to section 0.
    let citations = Citations::new(
        HashMap::from([(1, references.cited_passages(&[MARKER])?)]),
        HashMap::new(),
    );
    evidence_checks::check_with_citations(
        &model,
        &references,
        &inventory(&[CLAIM.into()]),
        &[],
        &ClaimChecks::default(),
        &citations,
    )
    .await?;
    let checks = model.checks.lock().unwrap();
    assert!(checks[0].iter().all(|text| !text.contains(MARKER)));
    Ok(())
}

#[tokio::test]
async fn cited_passages_keep_citation_order_within_the_window() -> Result<()> {
    let sources: Vec<_> = (0..12).map(related).collect();
    let references = ReferenceCollection::lexical(&sources)?;
    let model = PagingModel::new(6_000);
    let quotes: Vec<String> = (0..12)
        .rev()
        .map(|i| format!("Mineral alpha notes {i}: property beta was measured, pressure varied."))
        .collect();
    let quotes: Vec<&str> = quotes.iter().map(String::as_str).collect();
    let cited = references.cited_passages(&quotes)?;
    assert_eq!(cited.len(), 12);
    assert!(
        !fits_window(&model, CLAIM, &cited),
        "the fixture must overflow"
    );

    let citations = Citations::new(HashMap::from([(0, cited.clone())]), HashMap::new());
    let page = first_page(&model, CLAIM, 0, &references, &[], &citations).await?;

    let taken: Vec<_> = page.iter().filter(|p| p.retrieval_kind == CITED).collect();
    assert!(fits_window(&model, CLAIM, taken.iter().copied()));
    assert!(!taken.is_empty() && taken.len() < cited.len());
    // The first cited passages in citation order, the rest left for page 2.
    let expected: HashSet<_> = cited[..taken.len()].iter().map(|p| &p.source_id).collect();
    assert!(taken.iter().all(|p| expected.contains(&p.source_id)));
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
async fn research_found_for_a_claim_above_the_cut_leads_its_page_and_is_rechecked() -> Result<()> {
    let sources: Vec<_> = (0..3)
        .map(|index| regions(&format!("strong-{index}"), &[strong(index)]))
        .collect();
    let references = ReferenceCollection::lexical(&sources)?;
    let model = PagingModel::new(32_000);
    let claims = quartz_inventory();
    let mut retained = ClaimChecks::default();
    let first = evidence_checks::check(&model, &references, &claims, &[], &retained).await?;
    assert_eq!(first[0].2.verdict, ClaimVerdict::Unsupported);
    let findings: Vec<_> = first.iter().map(|result| result.2.clone()).collect();
    for (_, key, finding, _) in first {
        retained.record(key, &finding);
    }
    let checks_before = model.checks.lock().unwrap().len();

    // Research saves a source with a strong passage that states the claim
    // and a weak one, below the cut.
    let mut expanded_sources = sources.clone();
    expanded_sources.push(regions(
        "research",
        &[format!("{} {MARKER}", strong(10)), weak(11)],
    ));
    let expanded = ReferenceCollection::lexical(&expanded_sources)?;
    evidence_selection::record_research_scope(&expanded, &findings, &retained).await?;
    let second = evidence_checks::check(&model, &expanded, &claims, &[], &retained).await?;

    assert_eq!(second[0].2.verdict, ClaimVerdict::Supported);
    let checks = model.checks.lock().unwrap();
    let recheck = checks
        .get(checks_before)
        .expect("new evidence is checked again, not reused");
    assert!(
        recheck[0].contains(MARKER),
        "what research found comes first"
    );
    assert!(recheck.iter().all(|text| !text.contains("Note 011")));
    // The earlier page follows.
    assert_eq!(recheck.len(), 4);
    Ok(())
}

#[tokio::test]
async fn a_claim_sees_the_lesson_sources_before_the_rest_of_the_course() -> Result<()> {
    let mut sources: Vec<_> = (0..12).map(related).collect();
    let archive = "Archive index entry. ".repeat(80);
    sources.push(reference(
        "lesson-source".into(),
        format!("{archive} Mineral alpha pressure readings were archived. {archive}"),
    ));
    let references = ReferenceCollection::lexical(&sources)?;
    let model = PagingModel::new(6_000);
    let from_lesson = |page: &[EvidencePassage]| {
        page.iter()
            .position(|passage| passage.source_id == "lesson-source")
    };
    let course_wide = evidence_for(&model, CLAIM, 0, &references, &[]).await?;
    assert_eq!(
        from_lesson(&course_wide),
        None,
        "course-wide, the lesson's source ranks below the cut"
    );
    // Another section of the lesson cites that source.
    let citations = Citations::new(
        HashMap::from([(
            1,
            references.cited_passages(&["Archive index entry. Archive index entry."])?,
        )]),
        HashMap::new(),
    );
    let page = first_page(&model, CLAIM, 0, &references, &[], &citations).await?;
    assert!(
        from_lesson(&page).is_some(),
        "the lesson's source is retrieved before the course's"
    );
    assert_eq!(page.len(), 1, "the rest of the course waits for ring 2");
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
    let evidence = first_page(&model, CLAIM, 0, &references, &[], &citations).await?;
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
        assert_eq!(model.checks.lock().unwrap().len(), 1, "checked again");
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
    // Ten sources that rank alike: no gap ends page 1 before the window.
    let sources: Vec<_> = (0..10).map(related).collect();
    let references = ReferenceCollection::lexical(&sources)?;
    let model = DenseModel {
        paging: PagingModel::new(6_000),
        refused: Default::default(),
    };
    let first = evidence_for(&model, CLAIM, 0, &references, &[]).await?;
    let results = evidence_checks::check(
        &model,
        &references,
        &inventory(&[CLAIM.into()]),
        &[],
        &ClaimChecks::default(),
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
    assert!(checks.len() >= 2, "page 1, then a deeper page");
    assert!(cut.iter().any(|passage| saw(&checks[1], &passage.text)));
    // No page repeats a passage an earlier page showed.
    let shown: Vec<&String> = checks.iter().flatten().collect();
    let unique: HashSet<&String> = shown.iter().copied().collect();
    assert_eq!(shown.len(), unique.len());
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
        branch: None,
        branch_score: 0.0,
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
    // The judge reads the cited passage first, the researched one last, and
    // the least needed retrieved passage at the centre.
    assert_eq!(reading_order(&page), vec![1, 3, 2, 4, 0]);
}

#[test]
fn the_judge_reads_cited_passages_at_the_edges_and_the_key_stays_canonical() {
    let mut page = vec![
        ranked_passage(CITED, 0.0, "first cited"),
        ranked_passage("lexical", 0.3, "third retrieved"),
        ranked_passage(CITED, 0.0, "second cited"),
        ranked_passage("lexical", 0.5, "first retrieved"),
        ranked_passage("lexical", 0.4, "second retrieved"),
    ];
    let reading = reading_order(&page);
    assert_eq!(reading, vec![0, 3, 1, 4, 2]);
    let read: Vec<&str> = reading.iter().map(|at| page[*at].text.as_str()).collect();
    assert_eq!(
        read,
        [
            "first cited",
            "first retrieved",
            "third retrieved",
            "second retrieved",
            "second cited"
        ]
    );
    // The comparison is keyed on the canonical page, whatever order the
    // judge read it in.
    let claim = Claim {
        quote: CLAIM.into(),
        statement: CLAIM.into(),
    };
    let mut read_page: Vec<_> = reading.iter().map(|at| page[*at].clone()).collect();
    canonicalize_evidence(&mut page);
    canonicalize_evidence(&mut read_page);
    assert_eq!(
        ClaimChecks::key(0, &claim, &read_page),
        ClaimChecks::key(0, &claim, &page)
    );
}

#[tokio::test]
async fn the_judge_receives_the_page_in_reading_order() -> Result<()> {
    let mut sources = vec![regions("cited", &[cited(1), cited(2)])];
    sources.extend((0..3).map(|index| regions(&format!("strong-{index}"), &[strong(10 + index)])));
    let references = ReferenceCollection::lexical(&sources)?;
    let section = references.cited_passages(&[
        "Note 001: the section cites this record.",
        "Note 002: the section cites this record.",
    ])?;
    assert_eq!(section.len(), 2);
    let citations = Citations::new(HashMap::from([(0, section)]), HashMap::new());
    let model = PagingModel::new(32_000);
    evidence_checks::check_with_citations(
        &model,
        &references,
        &quartz_inventory(),
        &[],
        &ClaimChecks::default(),
        &citations,
    )
    .await?;
    let checks = model.checks.lock().unwrap();
    assert_eq!(checks[0].len(), 5);
    assert!(checks[0][0].contains("Note 001"), "{:?}", checks[0]);
    assert!(checks[0][4].contains("Note 002"), "{:?}", checks[0]);
    Ok(())
}

#[test]
fn branch_scores_are_no_part_of_a_comparison_or_its_selection() {
    let passage = ranked_passage("lexical_fallback", 1.0 / 61.0, "a ranked passage");
    let rescored = EvidencePassage {
        branch: Some(crate::features::learning::reference_collection::Branch::Keyword),
        branch_score: 7.5,
        ..passage.clone()
    };
    let claim = Claim {
        quote: CLAIM.into(),
        statement: CLAIM.into(),
    };
    let key = ClaimChecks::key(0, &claim, std::slice::from_ref(&passage));
    assert_eq!(
        ClaimChecks::key(0, &claim, std::slice::from_ref(&rescored)),
        key
    );
    let model = PagingModel::new(6_000);
    assert_eq!(
        claim_receipt_key(&model, &key),
        claim_receipt_key(
            &model,
            &ClaimChecks::key(0, &claim, std::slice::from_ref(&rescored))
        )
    );
    let saved = |page: &[EvidencePassage]| {
        serde_json::to_string(&evidence_selection::selection_of(&[], page)).unwrap()
    };
    assert_eq!(
        saved(std::slice::from_ref(&passage)),
        saved(std::slice::from_ref(&rescored))
    );
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
