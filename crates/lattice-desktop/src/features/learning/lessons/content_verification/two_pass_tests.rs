//! A claim's two passes (course evidence index §5.1): pass A against what
//! its own section cites, pass B across the course, packed into one
//! request, and what they decide together.
use super::evidence_paging_tests::{
    inventory, marker_collection, reference, related, PagingModel, CLAIM, CONTRARY, MARKER,
};
use super::*;
use crate::features::learning::lesson_drafts;
use std::sync::atomic::Ordering;

/// Unit 0 cites the passage holding `quote`.
fn citing(references: &ReferenceCollection<'_>, quote: &str) -> Result<Citations> {
    Ok(Citations::new(
        HashMap::from([(0, references.cited_passages(&[quote])?)]),
        HashMap::new(),
    ))
}

/// Whether a comparison read the marker's passage.
fn read_marker(check: &[String]) -> bool {
    check.iter().any(|text| text.contains(MARKER))
}

fn claim() -> Claim {
    Claim {
        quote: CLAIM.into(),
        statement: CLAIM.into(),
    }
}

#[tokio::test]
async fn pass_a_reads_only_what_its_section_cites_keyed_without_a_selection() -> Result<()> {
    let pool = crate::features::learning::tests::pool().await?;
    let (repo, job, lesson) = batch_scheduler_tests::setup(&pool).await?;
    let references = ReferenceCollection::lexical(&marker_collection())?;
    let model = PagingModel::new(6_000);
    let citations = citing(&references, MARKER)?;
    lesson_drafts::run(&repo, &job, &lesson, async {
        let results = evidence_checks::check_with_citations(
            &model,
            &references,
            &inventory(&[CLAIM.into()]),
            &[],
            &ClaimChecks::default(),
            &citations,
        )
        .await?;
        let finding = &results[0].2;
        assert_eq!(finding.verdict, ClaimVerdict::Supported);
        assert_eq!(finding.two_pass.pass, Some(Pass::Attribution));
        assert!(finding.two_pass.supported_by.is_none());
        // Pass A compared the cited passage and nothing else.
        let checks = model.checks.lock().unwrap().clone();
        let attribution: Vec<_> = checks.iter().filter(|check| read_marker(check)).collect();
        assert_eq!(attribution.len(), 1);
        assert_eq!(attribution[0].len(), citations.of(0).len());
        // Its receipt is keyed by the claim and those passages alone: no
        // selection, no ranking.
        let mut page = citations.of(0).to_vec();
        canonicalize_evidence(&mut page);
        let key = claim_receipt_key(&model, &ClaimChecks::key(0, &claim(), &page));
        assert!(lesson_drafts::checkpoint(&key).await?.is_some());
        Ok(())
    })
    .await
}

#[tokio::test]
async fn pass_b_never_shows_what_pass_a_shows() -> Result<()> {
    let references = ReferenceCollection::lexical(&marker_collection())?;
    // A window large enough for the whole course on one page.
    let model = PagingModel::new(32_000);
    let citations = citing(&references, MARKER)?;
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
    assert_eq!(checks.len(), 2, "pass A and pass B's first page");
    let corroboration: Vec<_> = checks.iter().filter(|check| !read_marker(check)).collect();
    assert_eq!(corroboration.len(), 1);
    assert_eq!(
        corroboration[0].len(),
        12,
        "every related source, and not the cited one"
    );
    Ok(())
}

#[tokio::test]
async fn a_claims_two_comparisons_share_one_request() -> Result<()> {
    let references = ReferenceCollection::lexical(&marker_collection())?;
    let model = PagingModel::new(32_000);
    let citations = citing(&references, MARKER)?;
    evidence_checks::check_with_citations(
        &model,
        &references,
        &inventory(&[CLAIM.into()]),
        &[],
        &ClaimChecks::default(),
        &citations,
    )
    .await?;
    assert_eq!(model.checks.lock().unwrap().len(), 2);
    assert_eq!(
        model.requests.load(Ordering::SeqCst),
        1,
        "one prompt, two answers"
    );
    Ok(())
}

/// Unit 0 cites a related passage that does not state the claim; the
/// lesson numbers that source 0 and the marker's 1.
fn citing_the_wrong_passage(references: &ReferenceCollection<'_>) -> Result<Citations> {
    let quote = "Mineral alpha notes 3: property beta was measured, pressure varied.";
    Ok(Citations::new(
        HashMap::from([(0, references.cited_passages(&[quote])?)]),
        HashMap::from([(0, "related-3".to_owned()), (1, "marker".to_owned())]),
    ))
}

#[tokio::test]
async fn a_claim_supported_elsewhere_has_its_citation_repaired() -> Result<()> {
    let sources = marker_collection();
    let references = ReferenceCollection::lexical(&sources)?;
    let model = PagingModel::new(32_000);
    let citations = citing_the_wrong_passage(&references)?;
    let results = evidence_checks::check_with_citations(
        &model,
        &references,
        &inventory(&[CLAIM.into()]),
        &[],
        &ClaimChecks::default(),
        &citations,
    )
    .await?;
    let finding = results[0].2.clone();
    assert_eq!(finding.verdict, ClaimVerdict::Supported);
    assert_eq!(finding.two_pass.pass, Some(Pass::Corroboration));
    assert_eq!(finding.two_pass.page, Some(1));
    let found = finding.two_pass.supported_by.clone().unwrap();
    assert_eq!(found.source_id, "marker");
    assert_eq!(found.cited, vec!["related-3".to_owned()]);

    let mut candidate = json!({"blocks":[{
        "kind":"explanation","title":"Alpha","body":CLAIM,"rubric":[],
        "quote":"Mineral alpha notes 3: property beta was measured, pressure varied.",
        "sourceIndex":0
    }],"questions":[]});
    let mut tried = HashSet::new();
    assert!(repair::cite_where_supported(
        &mut candidate,
        std::slice::from_ref(&finding),
        &citations,
        &references,
        &mut tried,
    )?);
    let block = &candidate["blocks"][0];
    assert_eq!(block["sourceIndex"], 1);
    let quote = block["quote"].as_str().unwrap();
    let marker = sources.iter().find(|source| source.id == "marker").unwrap();
    assert!(marker.excerpt.contains(quote));
    // The next pass reads it as the section's own citation: pass A's page
    // holds the passage pass B found.
    let next = section_citations(&candidate, &references)?;
    assert!(next
        .of(0)
        .iter()
        .any(|passage| passage.source_id == "marker" && passage.text.contains(MARKER)));
    assert_eq!(next.source_at(1), Some("marker"));
    // Written once: the same citation is not written again.
    assert!(!repair::cite_where_supported(
        &mut candidate,
        std::slice::from_ref(&finding),
        &citations,
        &references,
        &mut tried,
    )?);
    Ok(())
}

#[test]
fn a_source_the_lesson_does_not_number_keeps_its_citation() -> Result<()> {
    let sources = marker_collection();
    let references = ReferenceCollection::lexical(&sources)?;
    let marker = sources.iter().find(|source| source.id == "marker").unwrap();
    let mut finding = Finding {
        unit: 0,
        quote: CLAIM.into(),
        statement: CLAIM.into(),
        verdict: ClaimVerdict::Supported,
        reason: "The records state it.".into(),
        evidence: references.source_passages("marker", &[(0, marker.excerpt.len())])?,
        supporting_quote: Some(marker.excerpt.clone()),
        resolution: None,
        trust: None,
        two_pass: Default::default(),
    };
    finding.two_pass.supported_by = Some(SupportedElsewhere {
        source_id: "marker".into(),
        start_byte: 0,
        end_byte: marker.excerpt.len(),
        cited: vec!["related-3".into()],
    });
    let citations = Citations::new(HashMap::new(), HashMap::from([(0, "related-3".to_owned())]));
    let mut candidate = json!({"blocks":[{"kind":"explanation","title":"Alpha","body":CLAIM,"rubric":[],"quote":"x","sourceIndex":0}],"questions":[]});
    let before = candidate.clone();
    assert!(!repair::cite_where_supported(
        &mut candidate,
        &[finding],
        &citations,
        &references,
        &mut HashSet::new(),
    )?);
    assert_eq!(candidate, before);
    Ok(())
}

#[tokio::test]
async fn a_contradiction_pass_b_finds_beside_pass_a_support_reaches_the_gate() -> Result<()> {
    let mut sources = marker_collection();
    sources.push(reference(
        "contrary".into(),
        format!("Mineral alpha notes 98: property beta was measured, pressure varied. {CONTRARY}"),
    ));
    let references = ReferenceCollection::lexical(&sources)?;
    let model = PagingModel::new(32_000);
    let citations = citing(&references, MARKER)?;
    let results = evidence_checks::check_with_citations(
        &model,
        &references,
        &inventory(&[CLAIM.into()]),
        &[],
        &ClaimChecks::default(),
        &citations,
    )
    .await?;
    let finding = &results[0].2;
    // The citation supports the claim; the course says otherwise beside it.
    assert_eq!(finding.verdict, ClaimVerdict::Supported);
    assert_eq!(finding.two_pass.pass, Some(Pass::Attribution));
    let contrary = &finding.two_pass.contrary;
    assert_eq!(contrary.len(), 1);
    assert_eq!(contrary[0].passage.source_id, "contrary");
    assert!(contrary[0].quote.contains(CONTRARY));
    // It travels with the finding into the gate (`provenance_gate::assess`),
    // which levels it beside the support: see the gate's own tests.
    assert!(finding
        .evidence
        .iter()
        .all(|passage| passage.source_id == "marker"));
    Ok(())
}

#[tokio::test]
async fn pass_a_receipt_survives_a_change_in_the_course_ranking() -> Result<()> {
    let pool = crate::features::learning::tests::pool().await?;
    let (repo, job, lesson) = batch_scheduler_tests::setup(&pool).await?;
    let mut sources = marker_collection();
    let model = PagingModel::new(32_000);
    lesson_drafts::run(&repo, &job, &lesson, async {
        let references = ReferenceCollection::lexical(&sources)?;
        let citations = citing(&references, MARKER)?;
        evidence_checks::check_with_citations(
            &model,
            &references,
            &inventory(&[CLAIM.into()]),
            &[],
            &ClaimChecks::default(),
            &citations,
        )
        .await?;
        assert_eq!(model.checks.lock().unwrap().len(), 2);
        // A new source joins the course and ranks into pass B's list.
        sources.push(related(50));
        let references = ReferenceCollection::lexical(&sources)?;
        let citations = citing(&references, MARKER)?;
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
        let checks = model.checks.lock().unwrap().clone();
        assert_eq!(
            checks.len(),
            3,
            "pass B's first page only is compared again"
        );
        assert!(
            !read_marker(&checks[2]),
            "pass A's comparison was restored from its receipt"
        );
        assert!(checks[2].iter().any(|text| text.contains("notes 50")));
        Ok(())
    })
    .await
}

#[test]
fn either_pass_support_with_the_other_contradicting_carries_the_contrary() {
    let passage = |id: &str, text: &str| EvidencePassage {
        source_id: id.into(),
        text: text.into(),
        start_byte: 0,
        end_byte: text.len(),
        retrieval_kind: "lexical".into(),
        score: 0.0,
    };
    let decided = |verdict: ClaimVerdict, id: &str, text: &str| Finding {
        unit: 0,
        quote: CLAIM.into(),
        statement: CLAIM.into(),
        verdict,
        reason: String::new(),
        evidence: vec![passage(id, text)],
        supporting_quote: Some(text.into()),
        resolution: None,
        trust: None,
        two_pass: Default::default(),
    };
    // The citation says otherwise; the course supports the claim.
    let combined = evidence_checks::combined(
        Some(decided(ClaimVerdict::Contradicted, "cited", CONTRARY)),
        decided(ClaimVerdict::Supported, "found", MARKER),
        (Some(Reach::Course), 2),
        &["cited".to_owned()],
    );
    assert_eq!(combined.verdict, ClaimVerdict::Supported);
    assert_eq!(combined.two_pass.page, Some(2));
    assert_eq!(
        combined.two_pass.supported_by.as_ref().unwrap().source_id,
        "found"
    );
    assert_eq!(combined.two_pass.contrary[0].passage.source_id, "cited");
    // Neither supports: the citation's contradiction stands.
    let combined = evidence_checks::combined(
        Some(decided(ClaimVerdict::Contradicted, "cited", CONTRARY)),
        decided(ClaimVerdict::Unsupported, "found", "nothing"),
        (Some(Reach::Course), 1),
        &["cited".to_owned()],
    );
    assert_eq!(combined.verdict, ClaimVerdict::Contradicted);
    assert_eq!(combined.two_pass.pass, Some(Pass::Attribution));
    // Neither decides: unresolved, with everything both compared.
    let combined = evidence_checks::combined(
        Some(decided(ClaimVerdict::Unsupported, "cited", "nothing cited")),
        decided(ClaimVerdict::Unsupported, "found", "nothing found"),
        (Some(Reach::Course), 1),
        &["cited".to_owned()],
    );
    assert_eq!(combined.verdict, ClaimVerdict::Unsupported);
    assert_eq!(combined.evidence.len(), 2);
}

#[tokio::test]
async fn another_sections_citations_are_not_pass_a() -> Result<()> {
    let references = ReferenceCollection::lexical(&marker_collection())?;
    let model = PagingModel::new(6_000);
    // Section 1 cites the support; the claim belongs to section 0.
    let citations = Citations::new(
        HashMap::from([(1, references.cited_passages(&[MARKER])?)]),
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
    let checks = model.checks.lock().unwrap();
    assert!(
        !read_marker(&checks[0]),
        "no pass A: section 0 cites nothing"
    );
    assert_eq!(results[0].2.two_pass.pass, Some(Pass::Corroboration));
    Ok(())
}
