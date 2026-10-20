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
    // A later pass of a job that has measured what an answer costs.
    evidence_checks::check_with_citations(
        &model,
        &references,
        &inventory(&[CLAIM.into()]),
        &[],
        &measured(),
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

/// Unit 0 cites a related passage that does not state the claim.
fn citing_the_wrong_passage(references: &ReferenceCollection<'_>) -> Result<Citations> {
    let quote = "Mineral alpha notes 3: property beta was measured, pressure varied.";
    Ok(Citations::new(
        HashMap::from([(0, references.cited_passages(&[quote])?)]),
        HashMap::from([(0, "related-3".to_owned())]),
    ))
}

/// A lesson section citing the related passage, as source 0.
fn wrongly_cited() -> Value {
    json!({"blocks":[{
        "kind":"explanation","title":"Alpha","body":CLAIM,"rubric":[],
        "quote":"Mineral alpha notes 3: property beta was measured, pressure varied.",
        "sourceIndex":0
    }],"questions":[]})
}

/// The claim's finding when pass A reads the wrong passage and pass B finds
/// the marker's.
async fn supported_elsewhere(references: &ReferenceCollection<'_>) -> Result<Finding> {
    let model = PagingModel::new(32_000);
    let results = evidence_checks::check_with_citations(
        &model,
        references,
        &inventory(&[CLAIM.into()]),
        &[],
        &ClaimChecks::default(),
        &citing_the_wrong_passage(references)?,
    )
    .await?;
    Ok(results[0].2.clone())
}

#[tokio::test]
async fn a_claim_supported_elsewhere_has_its_citation_repaired() -> Result<()> {
    let sources = marker_collection();
    let references = ReferenceCollection::lexical(&sources)?;
    let finding = supported_elsewhere(&references).await?;
    assert_eq!(finding.verdict, ClaimVerdict::Supported);
    assert_eq!(finding.two_pass.pass, Some(Pass::Corroboration));
    assert_eq!(finding.two_pass.page, Some(1));
    let found = finding.two_pass.supported_by.clone().unwrap();
    assert_eq!(found.source_id, "marker");
    assert_eq!(found.cited, vec!["related-3".to_owned()]);

    // The lesson numbers both sources: related-3 as 0, the marker's as 1.
    let source = |id: &str| {
        sources
            .iter()
            .find(|source| source.id == id)
            .unwrap()
            .clone()
    };
    let mut numbered = vec![source("related-3"), source("marker")];
    let mut candidate = wrongly_cited();
    let mut tried = HashSet::new();
    let repairs = repair::cite_where_supported(
        &mut candidate,
        std::slice::from_ref(&finding),
        &mut numbered,
        &references,
        &mut tried,
    )?;
    assert_eq!(repairs.len(), 1);
    let block = &candidate["blocks"][0];
    assert_eq!(block["sourceIndex"], 1);
    assert_eq!(
        numbered.len(),
        2,
        "the marker's source was numbered already"
    );
    let quote = block["quote"].as_str().unwrap();
    assert!(source("marker").excerpt.contains(quote));
    assert_eq!(
        quote, "Mineral alpha notes 99: property was logged.",
        "the run of the found passage that best matches the claim"
    );
    let lattice_core::domain::learning::claim_review::ClaimEvent::CitationRepaired {
        cited,
        source_version_id,
        added_source,
        ..
    } = &repairs[0].1
    else {
        panic!("{:?}", repairs[0].1);
    };
    assert_eq!(cited, &vec!["related-3".to_owned()]);
    assert_eq!(source_version_id, "marker");
    assert!(!added_source);
    // The next pass reads it as the section's own citation: pass A's page
    // holds the passage pass B found.
    let next = section_citations(&candidate, &references)?;
    assert!(next
        .of(0)
        .iter()
        .any(|passage| passage.source_id == "marker" && passage.text.contains(MARKER)));
    assert_eq!(next.source_at(1), Some("marker"));
    // Written once: the same citation is not written again.
    assert!(repair::cite_where_supported(
        &mut candidate,
        std::slice::from_ref(&finding),
        &mut numbered,
        &references,
        &mut tried,
    )?
    .is_empty());
    Ok(())
}

#[tokio::test]
async fn a_source_the_lesson_does_not_number_is_added_and_cited() -> Result<()> {
    let sources = marker_collection();
    let references = ReferenceCollection::lexical(&sources)?;
    let finding = supported_elsewhere(&references).await?;
    // The writer was given related-3 only.
    let related = sources
        .iter()
        .find(|source| source.id == "related-3")
        .unwrap();
    let mut numbered = vec![related.clone()];
    let mut candidate = wrongly_cited();
    let repairs = repair::cite_where_supported(
        &mut candidate,
        &[finding],
        &mut numbered,
        &references,
        &mut HashSet::new(),
    )?;
    // The marker's source joins the lesson's sources, with its title and
    // URL as the writer's are, and the section cites it.
    assert_eq!(numbered.len(), 2);
    assert_eq!(numbered[1].id, "marker");
    assert_eq!(numbered[1].title, "Reference marker");
    assert_eq!(candidate["blocks"][0]["sourceIndex"], 1);
    assert!(matches!(
        repairs[0].1,
        lattice_core::domain::learning::claim_review::ClaimEvent::CitationRepaired {
            added_source: true,
            ..
        }
    ));
    // A lesson schema's source indices then allow the new source.
    let mut schema = json!({"properties":{"blocks":{"items":{"properties":{
        "sourceIndex":{"type":"integer","minimum":0,"maximum":0}}}}}});
    repair::number_sources(&mut schema, numbered.len());
    assert_eq!(
        schema["properties"]["blocks"]["items"]["properties"]["sourceIndex"]["maximum"],
        1
    );
    Ok(())
}

#[test]
fn the_repaired_quote_is_the_run_of_sentences_that_best_matches_the_claim() {
    let filler: String = (0..40)
        .map(|line| format!("Archive line {line} is filed away. "))
        .collect();
    let stated = "Mineral alpha shows property beta under pressure in the lab.";
    let passage = format!("{filler}{stated} A closing remark follows.");
    let quote = repair::quotation(CLAIM, &passage, &passage).unwrap();
    assert_eq!(quote, stated, "the matching sentence near the end, alone");
    assert!(quote.chars().count() <= crate::features::learning::generation::MAX_QUOTE_CHARS);
    // Two runs that match as well: the earliest.
    let twice = format!("{stated} Then a pause comes here. {stated}");
    let quote = repair::quotation(CLAIM, &twice, &twice).unwrap();
    assert_eq!(twice.find(quote), Some(0));
    // No run of whole sentences short enough to cite: none.
    let endless = format!("{} pressure", "mineral alpha ".repeat(200));
    assert_eq!(repair::quotation(CLAIM, &endless, &endless), None);
}

#[tokio::test]
async fn the_first_request_carries_pass_a_only_and_its_answers_size_pass_b() -> Result<()> {
    use lattice_core::application::ports::llm_port::Pace;
    let references = ReferenceCollection::lexical(&marker_collection())?;
    // 130 output tokens an answer, reading ten times faster than writing: a
    // page reads 1,300 tokens, two related passages.
    let pace = Pace {
        prompt_tokens_per_second: 1_000.0,
        output_tokens_per_second: 100.0,
    };
    let model = PagingModel::paced(32_000, pace, 130);
    let statements = [CLAIM.to_owned(), format!("{CLAIM} It holds in the lab.")];
    evidence_checks::check_with_citations(
        &model,
        &references,
        &inventory(&statements),
        &[],
        &ClaimChecks::default(),
        &citing(&references, MARKER)?,
    )
    .await?;
    let sizes = model.request_sizes.lock().unwrap().clone();
    let checks = model.checks.lock().unwrap().clone();
    assert_eq!(sizes[0], 2, "both claims' pass A, together");
    assert!(checks[..2].iter().all(|check| read_marker(check)));
    // Every later request reads pass B pages sized by the measured answers,
    // not by the most an answer could be.
    let budget = page_budget(Some(pace), Some(130.0)).unwrap();
    assert_eq!(budget, 1_300);
    assert!(checks.len() > 2);
    for check in &checks[2..] {
        assert!(!read_marker(check));
        let tokens: usize = check.iter().map(|text| bank_tokens(&model, text)).sum();
        assert!(tokens <= budget, "{tokens} > {budget}");
        assert!(!check.is_empty());
    }
    Ok(())
}

#[tokio::test]
async fn without_pass_a_the_first_claim_reads_a_page_bounded_by_the_window_alone() -> Result<()> {
    use lattice_core::application::ports::llm_port::Pace;
    let sources: Vec<_> = (0..12).map(related).collect();
    let references = ReferenceCollection::lexical(&sources)?;
    let pace = Pace {
        prompt_tokens_per_second: 1_000.0,
        output_tokens_per_second: 100.0,
    };
    let model = PagingModel::paced(32_000, pace, 130);
    let statements = [
        CLAIM.to_owned(),
        "Mineral alpha shows property beta under heat.".to_owned(),
        "Mineral alpha shows property beta when cold.".to_owned(),
    ];
    // No section cites anything: there is no pass A to measure by.
    evidence_checks::check(
        &model,
        &references,
        &inventory(&statements),
        &[],
        &ClaimChecks::default(),
    )
    .await?;
    let sizes = model.request_sizes.lock().unwrap().clone();
    let checks = model.checks.lock().unwrap().clone();
    assert_eq!(sizes[0], 1, "the first claim alone");
    assert_eq!(checks[0].len(), 12, "its page bounded by the window alone");
    let budget = page_budget(Some(pace), Some(130.0)).unwrap();
    for check in &checks[1..] {
        let tokens: usize = check.iter().map(|text| bank_tokens(&model, text)).sum();
        assert!(tokens <= budget, "{tokens} > {budget}");
    }
    assert!(checks.len() > 1);
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
