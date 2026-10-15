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
}

impl PagingModel {
    fn new(window: usize) -> Self {
        Self {
            inner: Model::new(),
            window,
            checks: Mutex::new(Vec::new()),
        }
    }

    async fn respond(&self, prompt: &str) -> Result<String> {
        if let Some(rest) = prompt.strip_prefix("Source passages:\n") {
            let shown = rest.split("\n\nClaim: ").next().unwrap_or_default();
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

    let results = evidence_checks::check(
        &model,
        &references,
        &inventory(&[CLAIM.into()]),
        &[],
        &ClaimChecks::default(),
    )
    .await?;

    assert_eq!(results[0].2.verdict, ClaimVerdict::Supported);
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
    let results = evidence_checks::check(
        &model,
        &references,
        &inventory(&[CLAIM.into()]),
        &[],
        &ClaimChecks::default(),
    )
    .await?;
    assert_eq!(results[0].2.verdict, ClaimVerdict::Unsupported);
    // Page 1, then one deepening round that settled nothing, then stop.
    assert_eq!(model.checks.lock().unwrap().len(), 2);
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
