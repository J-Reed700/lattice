//! Claim evidence is paged: a share of a batch call first, deeper pages only
//! for a claim still unsupported, while a page shares a content term.
use super::*;
use std::collections::HashSet;
use std::sync::Mutex;

const CLAIM: &str = "Mineral alpha shows property beta under pressure.";
/// Shares one content term with the claim, so it ranks below the others.
const MARKER: &str = "Laboratory records confirm the beta behaviour of the sample.";

/// A small window, so a page holds a few passages. Supports a claim only
/// when it is shown the marker passage, and records what each check saw.
struct PagingModel {
    inner: Model,
    window: usize,
    checks: Mutex<Vec<Vec<String>>>,
    /// The claim each check was asked about.
    claims: Mutex<Vec<String>>,
}

impl PagingModel {
    fn new(window: usize) -> Self {
        Self {
            inner: Model::new(),
            window,
            checks: Mutex::new(Vec::new()),
            claims: Mutex::new(Vec::new()),
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
            self.checks.lock().unwrap().push(passages);
            return Ok(match found {
                Some(index) => format!(
                    "supported\nReason: The records state it.\nSource passage: passage-{index}"
                ),
                None => "unsupported\nReason: No passage states this.\nSource passage: none".into(),
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
async fn a_claim_backed_only_past_its_first_page_is_found_by_deepening() -> Result<()> {
    // The marker passage shares one term with the claim, so it ranks last.
    let mut sources: Vec<_> = (0..8).map(related).collect();
    sources.push(reference(
        "marker".into(),
        format!("{} {MARKER}", "Archive index entry. ".repeat(80)),
    ));
    let references = ReferenceCollection::lexical(&sources)?;
    let model = PagingModel::new(6_000);
    let first = evidence_for(&model, CLAIM, 0, &references, &[], 1).await?;
    assert!(!first.is_empty());
    assert!(
        first.iter().all(|passage| !passage.text.contains(MARKER)),
        "the fixture must put the support past page 1"
    );

    let (results, activity) =
        crate::features::learning::lesson_progress::observed(evidence_checks::check(
            &model,
            &references,
            &inventory(&[CLAIM.into()]),
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
    assert_eq!(activity.deeper_evidence, None);
    assert!(results[0]
        .2
        .evidence
        .iter()
        .any(|p| p.text.contains(MARKER)));
    let checks = model.checks.lock().unwrap();
    assert!(checks.len() >= 2, "page 1, then at least one deeper page");
    // No page repeats a passage an earlier page showed.
    let shown: Vec<&String> = checks.iter().flatten().collect();
    let unique: HashSet<&String> = shown.iter().copied().collect();
    assert_eq!(shown.len(), unique.len());
    Ok(())
}

#[tokio::test]
async fn a_claim_with_no_content_term_past_its_first_page_stops_there() -> Result<()> {
    // Three passages share content terms; the rest share only "under".
    let mut sources: Vec<_> = (0..3).map(related).collect();
    sources.extend((0..10).map(|index| {
        reference(
            format!("function-{index}"),
            format!(
                "Item {index} sits under the shelf. {}",
                "Unrelated filler text. ".repeat(80)
            ),
        )
    }));
    let references = ReferenceCollection::lexical(&sources)?;
    let model = PagingModel::new(6_000);
    let first = evidence_for(&model, CLAIM, 0, &references, &[], 1).await?;
    let share = claim_share(&model, CLAIM, 1);
    let (next, _) = references
        .verification_page(CLAIM, &first, |page| {
            within_share(&model, share, page.iter())
        })
        .await?;
    assert!(!next.is_empty(), "the fixture must leave a second page");
    assert!(next
        .iter()
        .all(|p| !references.mentions_subject_of(CLAIM, &p.text)));

    let results = evidence_checks::check(
        &model,
        &references,
        &inventory(&[CLAIM.into()]),
        &[],
        &ClaimChecks::default(),
    )
    .await?;

    assert_eq!(results[0].2.verdict, ClaimVerdict::Unsupported);
    assert_eq!(
        model.checks.lock().unwrap().len(),
        1,
        "only page 1 was checked"
    );
    Ok(())
}

#[tokio::test]
async fn each_claim_in_a_batch_call_stays_within_its_share() -> Result<()> {
    let sources: Vec<_> = (0..40).map(related).collect();
    let references = ReferenceCollection::lexical(&sources)?;
    let model = PagingModel::new(32_000);
    let statements: Vec<String> = (0..8)
        .map(|index| format!("Mineral alpha shows property beta in sample {index}."))
        .collect();
    let alone = evidence_for(&model, &statements[0], 0, &references, &[], 1).await?;
    for statement in &statements {
        let share = claim_share(&model, statement, 8);
        let evidence = evidence_for(&model, statement, 0, &references, &[], 8).await?;
        assert!(!evidence.is_empty());
        assert!(within_share(&model, share, evidence.iter()));
        assert!(
            evidence.len() < alone.len(),
            "a shared call gives each claim less"
        );
    }
    // Every batch request the check sends fits the window.
    evidence_checks::check(
        &model,
        &references,
        &inventory(&statements),
        &[],
        &ClaimChecks::default(),
    )
    .await?;
    assert!(model.checks.lock().unwrap().len() >= statements.len());
    Ok(())
}

#[tokio::test]
async fn deepening_stops_after_a_round_that_settles_nothing() -> Result<()> {
    // Plenty of deeper pages mention the claim's subject, but none settles it.
    let sources: Vec<_> = (0..20).map(related).collect();
    let references = ReferenceCollection::lexical(&sources)?;
    let model = PagingModel::new(6_000);
    let (results, activity) =
        crate::features::learning::lesson_progress::observed(evidence_checks::check(
            &model,
            &references,
            &inventory(&[CLAIM.into()]),
            &[],
            &ClaimChecks::default(),
        ))
        .await;
    let results = results?;
    assert_eq!(results[0].2.verdict, ClaimVerdict::Unsupported);
    // Page 1, then one deepening round that settled nothing, then stop.
    assert_eq!(model.checks.lock().unwrap().len(), 2);
    assert_eq!(
        (activity.checks_completed, activity.checks_unresolved),
        (1, 1),
        "A claim unsupported on two pages needs attention once"
    );
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
    // Retrieval alone ranks the support past page 1.
    let retrieved = evidence_for(&model, CLAIM, 0, &references, &[], 1).await?;
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
async fn cited_passages_keep_citation_order_within_the_share() -> Result<()> {
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
    let share = claim_share(&model, CLAIM, 1);
    assert!(
        !within_share(&model, share, cited.iter()),
        "the fixture must overflow"
    );

    let citations = Citations::new(HashMap::from([(0, cited.clone())]), HashMap::new());
    let page = first_page(&model, CLAIM, 0, &references, &[], 1, &citations).await?;

    assert!(within_share(&model, share, page.iter()));
    let taken: Vec<_> = page.iter().filter(|p| p.retrieval_kind == CITED).collect();
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
async fn research_found_for_a_claim_leads_its_full_first_page_and_is_rechecked() -> Result<()> {
    let sources: Vec<_> = (0..12).map(related).collect();
    let references = ReferenceCollection::lexical(&sources)?;
    let model = PagingModel::new(6_000);
    let claims = inventory(&[CLAIM.into()]);
    let mut retained = ClaimChecks::default();
    let first = evidence_checks::check(&model, &references, &claims, &[], &retained).await?;
    assert_eq!(first[0].2.verdict, ClaimVerdict::Unsupported);
    let share = claim_share(&model, CLAIM, 1);
    let first_page = first[0].2.evidence.clone();
    let findings: Vec<_> = first.iter().map(|result| result.2.clone()).collect();
    for (_, key, finding, _) in first {
        retained.record(key, &finding);
    }
    let checks_before = model.checks.lock().unwrap().len();

    // Research saves a source that states the claim, in a passage longer
    // than any room the first page leaves.
    let mut expanded_sources = sources.clone();
    let archive = "Archive index entry. ".repeat(80);
    expanded_sources.push(reference(
        "marker".into(),
        format!("{archive} {MARKER} {archive}"),
    ));
    let expanded = ReferenceCollection::lexical(&expanded_sources)?;
    assert!(
        !within_share(
            &model,
            share,
            first_page
                .iter()
                .chain(&expanded.cited_passages(&[MARKER])?)
        ),
        "the fixture's first page has no room left for the new passage"
    );
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
    assert!(within_share(&model, share, second[0].2.evidence.iter()));
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
    let course_wide = evidence_for(&model, CLAIM, 0, &references, &[], 1).await?;
    assert_eq!(
        from_lesson(&course_wide),
        None,
        "course-wide ranking puts the lesson's source past page 1"
    );
    // Another section of the lesson cites that source.
    let citations = Citations::new(
        HashMap::from([(
            1,
            references.cited_passages(&["Archive index entry. Archive index entry."])?,
        )]),
        HashMap::new(),
    );
    let page = first_page(&model, CLAIM, 0, &references, &[], 1, &citations).await?;
    assert!(
        from_lesson(&page).is_some(),
        "the lesson's source is retrieved before the course's"
    );
    assert!(page.len() > 1, "the course fills the room left");
    assert!(within_share(
        &model,
        claim_share(&model, CLAIM, 1),
        page.iter()
    ));
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
    let evidence = first_page(&model, CLAIM, 0, &references, &[], 1, &citations).await?;
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
