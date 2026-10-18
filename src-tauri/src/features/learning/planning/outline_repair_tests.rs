//! Repair by finding kind: evidence closes a supported factual finding, a
//! contradiction is rewritten with its passage, the second attempt removes,
//! module budgets exhaust, plan repair never adds citations, and targeted
//! research saves pages that the judge then reads.
use super::outline_reference_tests::*;
use crate::features::learning::{
    dto::*,
    lessons::course_generation_tests::request,
    outline_draft::{
        repair_saved, DraftRunRegistry, LearningOutlineIssueKind, LearningOutlineReviewStatus,
        RepairLearningOutlineRequestDto,
    },
    outline_linking,
    outline_progress::OutlineProgress,
    outline_repair::{CORRECT, NARROW, PLAN_SMALLEST, REMOVE},
    repository::LearningRepository,
    service,
};
use lattice_core::application::services::claim_verification::ClaimVerdict;
use lattice_core::shared::error::Result;
use serde_json::{json, Value};

fn with_summary(text: &str) -> Value {
    let mut initial = candidate();
    initial["modules"][0]["summary"]["text"] = json!(text);
    initial
}

fn module_with_summary(candidate: &Value, text: &str) -> Value {
    let mut module = candidate["modules"][0].clone();
    module["summary"]["text"] = json!(text);
    json!({"module":module})
}

fn module_with_objective(candidate: &Value, lesson: usize, objective: &str) -> Value {
    let mut module = candidate["modules"][0].clone();
    module["lessons"][lesson]["objective"] = json!(objective);
    module
}

async fn generate_with(
    repo: &LearningRepository,
    llm: &crate::features::learning::lessons::course_generation_tests::ScriptedModel,
    sources: Vec<LearningSourceDto>,
) -> Result<LearningProgramDto> {
    service::generate_with_references(
        repo,
        &DraftRunRegistry::new(),
        llm,
        request(LearningCourseDepth::Course),
        sources.clone(),
        &references(&sources),
    )
    .await
}

#[tokio::test]
async fn a_supported_factual_finding_is_closed_by_attached_evidence_without_a_rewrite() -> Result<()>
{
    let repo = LearningRepository::new(crate::features::learning::tests::pool().await?);
    let mut source = original_source();
    source.excerpt = ADDITIONAL.into();
    let sources = vec![source.clone()];
    // Linking declines the summary's candidate; the factual repair then
    // attaches the passage the strict judge supports. The module's reviewed
    // text is unchanged by that citation, so no further review call is made.
    let llm = model(vec![
        with_summary(ADDITIONAL),
        no_links(),
        findings(vec![factual_finding(
            "/modules/0/summary",
            ADDITIONAL,
            "This assertion needs evidence from the saved references.",
        )]),
        judged("supported", Some(0)),
    ]);
    let program = generate_with(&repo, &llm, sources).await?;
    let review = program.outline_review.as_ref().unwrap();
    assert_eq!(
        review.status,
        LearningOutlineReviewStatus::Passed,
        "{}",
        review.note
    );
    assert_eq!(review.repair_passes, 1);
    assert_eq!(review.citations_verified, 1);
    assert!(review.issues.is_empty());
    assert!(review.note.contains("1 of 24 outline items linked"));
    let prompts = calls(&llm);
    // The strict judgment is the only check: no challenge call follows it.
    assert_eq!(prompts.len(), 3);
    assert!(prompts[2].starts_with("Source passages:"));
    assert!(prompts[2].contains(&format!("Claim: {ADDITIONAL}")));
    assert!(!prompts
        .iter()
        .any(|p| p.starts_with("Repair a saved curriculum module")));
    let saved = repo.outline_draft(&program.summary.id).await?.unwrap();
    assert_eq!(saved.candidate["modules"][0]["summary"]["sourceIndex"], 0);
    assert_eq!(
        saved.candidate["modules"][0]["summary"]["quote"],
        ADDITIONAL
    );
    assert_eq!(
        saved
            .citation_receipts
            .values()
            .filter(|r| r.covers)
            .count(),
        1
    );
    let receipt = saved.claim_receipts.values().next().unwrap();
    assert_eq!(receipt.verdict, ClaimVerdict::Supported);
    assert_eq!(receipt.claim, ADDITIONAL);
    assert_eq!(receipt.path, "/modules/0/summary");
    assert_eq!(receipt.source_id.as_deref(), Some(source.id.as_str()));
    Ok(())
}

#[tokio::test]
async fn a_contradicted_factual_finding_is_rewritten_with_the_contradicting_passage() -> Result<()>
{
    let repo = LearningRepository::new(crate::features::learning::tests::pool().await?);
    let wrong = "Random assignment increases confounding when treatment groups are compared.";
    let mut source = original_source();
    source.excerpt = ADDITIONAL.into();
    let sources = vec![source.clone()];
    let initial = with_summary(wrong);
    // The corrected summary is new text, so it is offered for linking again
    // after the rewrite; nothing is chosen either time.
    let llm = model(vec![
        initial.clone(),
        no_links(),
        findings(vec![factual_finding(
            "/modules/0/summary",
            wrong,
            "This states the opposite of what the saved reference says.",
        )]),
        judged("contradicted", Some(0)),
        module_with_summary(&initial, ADDITIONAL),
        no_links(),
        verdicts(&[("finding-1", "resolved")], vec![]),
    ]);
    let program = generate_with(&repo, &llm, sources).await?;
    let review = program.outline_review.as_ref().unwrap();
    assert_eq!(
        review.status,
        LearningOutlineReviewStatus::Passed,
        "{}",
        review.note
    );
    assert!(review.issues.is_empty());
    let prompts = calls(&llm);
    assert_eq!(prompts.len(), 5);
    assert!(prompts[3].starts_with("Repair a saved curriculum module"));
    let repair = payload(&prompts[3]);
    assert_eq!(repair["findings"][0]["id"], "finding-1");
    assert_eq!(repair["findings"][0]["instruction"], CORRECT);
    assert_eq!(
        repair["findings"][0]["contradictingPassage"]["sourceIndex"],
        0
    );
    assert_eq!(
        repair["findings"][0]["contradictingPassage"]["quote"],
        ADDITIONAL
    );
    assert!(repair["findings"][0]["reason"]
        .as_str()
        .unwrap()
        .starts_with("Contradicted by a saved reference"));
    let saved = repo.outline_draft(&program.summary.id).await?.unwrap();
    let receipt = saved.claim_receipts.values().next().unwrap();
    assert_eq!(receipt.verdict, ClaimVerdict::Contradicted);
    assert_eq!(receipt.supporting_quote.as_deref(), Some(ADDITIONAL));
    assert_eq!(receipt.source_id.as_deref(), Some(source.id.as_str()));
    assert_eq!(saved.candidate["modules"][0]["summary"]["text"], ADDITIONAL);
    // The rewrite changed the text, so the rewrite's item carries no citation.
    assert_eq!(
        saved.candidate["modules"][0]["summary"]["sourceIndex"],
        Value::Null
    );
    Ok(())
}

#[tokio::test]
async fn an_unsupported_assertion_is_narrowed_then_removed_then_left_for_the_user() -> Result<()> {
    let repo = LearningRepository::new(crate::features::learning::tests::pool().await?);
    let claim = "Random assignment isolates the intervention and eliminates every source of bias in a comparison.";
    let sources = vec![original_source()];
    let initial = with_summary(claim);
    let narrowed = "Random assignment isolates the intervention and reduces most sources of bias in a comparison.";
    let removed = "Random assignment isolates the intervention in a comparison.";
    let llm = model(vec![
        initial.clone(),
        no_links(),
        findings(vec![factual_finding(
            "/modules/0/summary",
            claim,
            "No reference supports that every source of bias is eliminated.",
        )]),
        judged("unsupported", None),
        module_with_summary(&initial, narrowed),
        no_links(),
        json!({"verdicts":[{"id":"finding-1","status":"unresolved","reason":"The summary still overstates what random assignment guarantees."}],"newFindings":[]}),
        module_with_summary(&initial, removed),
        no_links(),
        json!({"verdicts":[{"id":"finding-1","status":"unresolved","reason":"The summary still presents the guarantee as a fact."}],"newFindings":[]}),
    ]);
    let program = generate_with(&repo, &llm, sources).await?;
    let review = program.outline_review.as_ref().unwrap();
    assert_eq!(
        review.status,
        LearningOutlineReviewStatus::NeedsRepair,
        "{}",
        review.note
    );
    assert_eq!(review.repair_passes, 2);
    assert_eq!(review.issues.len(), 1);
    let issue = &review.issues[0];
    assert_eq!(issue.kind, LearningOutlineIssueKind::Factual);
    assert_eq!(issue.attempts, 2);
    assert!(issue.exhausted);
    assert_eq!(issue.claim, removed);
    assert!(review.note.starts_with("1 finding remain"));
    assert!(review
        .note
        .contains("was not supported by the saved references"));
    let prompts = calls(&llm);
    assert_eq!(prompts.len(), 7);
    assert!(prompts[2].starts_with("Source passages:"));
    assert_eq!(payload(&prompts[3])["findings"][0]["instruction"], NARROW);
    assert!(payload(&prompts[3])["findings"][0]["reason"]
        .as_str()
        .unwrap()
        .starts_with("Not supported by the saved references"));
    // The second attempt neither researches nor judges again.
    assert!(prompts[5].starts_with("Repair a saved curriculum module"));
    assert_eq!(payload(&prompts[5])["findings"][0]["instruction"], REMOVE);
    assert_eq!(
        prompts
            .iter()
            .filter(|p| p.starts_with("Source passages:"))
            .count(),
        1
    );
    Ok(())
}

#[tokio::test]
async fn a_module_that_keeps_spawning_findings_runs_out_of_rewrites() -> Result<()> {
    let repo = LearningRepository::new(crate::features::learning::tests::pool().await?);
    let initial = candidate();
    let first = module_with_objective(&initial, 0, "Measure the first comparison.");
    let mut second = first.clone();
    second["lessons"][1]["objective"] = json!("Measure the second comparison.");
    let mut third = second.clone();
    third["lessons"][2]["objective"] = json!("Measure the third comparison.");
    let objective = "Design a controlled experiment and explain its limitations.";
    let llm = model(vec![
        initial,
        findings(vec![plan_finding(
            "/modules/0/lessons/0",
            objective,
            "Name the measurement for this lesson.",
        )]),
        json!({"module":first}),
        verdicts(
            &[("finding-1", "resolved")],
            vec![plan_finding(
                "/modules/0/lessons/1",
                objective,
                "Name the measurement for this lesson too.",
            )],
        ),
        json!({"module":second}),
        verdicts(
            &[("finding-2", "resolved")],
            vec![plan_finding(
                "/modules/0/lessons/2",
                objective,
                "And for this lesson.",
            )],
        ),
        json!({"module":third}),
        verdicts(
            &[("finding-3", "resolved")],
            vec![plan_finding(
                "/modules/0/summary",
                "Build a skill and apply it to an experiment.",
                "The summary no longer matches the rewritten lessons.",
            )],
        ),
    ]);
    let program = service::generate(
        &repo,
        &DraftRunRegistry::new(),
        &llm,
        request(LearningCourseDepth::Course),
        vec![],
    )
    .await?;
    let review = program.outline_review.as_ref().unwrap();
    // Out of rewrites, the summary finding is advice that does not block.
    assert_eq!(
        review.status,
        LearningOutlineReviewStatus::Passed,
        "{}",
        review.note
    );
    assert_eq!(review.repair_passes, 3);
    assert_eq!(review.issues.len(), 1);
    assert_eq!(review.issues[0].id, "finding-4");
    assert_eq!(review.issues[0].attempts, 0);
    assert!(review.issues[0].exhausted);
    assert!(review.note.starts_with("Outline checks passed."));
    assert!(review.note.contains("Module 1 · Summary:"));
    assert_eq!(llm.prompts.lock().unwrap().len(), 8);
    Ok(())
}

#[tokio::test]
async fn plan_repair_never_adds_a_citation() -> Result<()> {
    let repo = LearningRepository::new(crate::features::learning::tests::pool().await?);
    let sources = vec![original_source()];
    let initial = candidate();
    let mut fixed = initial["modules"][0].clone();
    fixed["project"]["brief"] =
        json!("Measure the comparison with an observable outcome and record its limitations.");
    // The rewrite also cites the saved passage, exactly, on an unchanged item.
    fixed["summary"]["sourceIndex"] = json!(0);
    fixed["summary"]["quote"] = json!(ORIGINAL);
    let llm = model(vec![
        initial,
        no_links(),
        issue(0),
        json!({"module":fixed}),
        verdicts(&[("finding-1", "resolved")], vec![]),
    ]);
    let program = generate_with(&repo, &llm, sources).await?;
    let review = program.outline_review.as_ref().unwrap();
    assert_eq!(
        review.status,
        LearningOutlineReviewStatus::Passed,
        "{}",
        review.note
    );
    assert_eq!(review.citations_verified, 0);
    let prompts = calls(&llm);
    assert_eq!(prompts.len(), 4);
    assert_eq!(payload(&prompts[2])["findings"][0]["kind"], "plan");
    // The rewrite changed no item text, so no item was offered for linking
    // again: every linking call precedes the first review.
    {
        let all = llm.prompts.lock().unwrap();
        let last_link = all
            .iter()
            .rposition(|p| p.starts_with(outline_linking::LINK_SYSTEM))
            .unwrap();
        let first_review = all
            .iter()
            .position(|p| p.starts_with("Review instructional quality."))
            .unwrap();
        assert!(last_link < first_review);
    }
    let saved = repo.outline_draft(&program.summary.id).await?.unwrap();
    assert_eq!(
        saved.candidate["modules"][0]["summary"]["sourceIndex"],
        Value::Null
    );
    assert_eq!(saved.candidate["modules"][0]["summary"]["quote"], "");
    assert!(saved.candidate["modules"][0]["project"]["brief"]
        .as_str()
        .unwrap()
        .starts_with("Measure the comparison"));
    assert!(saved.citation_receipts.values().all(|r| !r.covers));
    Ok(())
}

#[tokio::test]
async fn second_plan_attempt_asks_for_the_smallest_edit() -> Result<()> {
    let repo = LearningRepository::new(crate::features::learning::tests::pool().await?);
    let initial = candidate();
    let llm = model(vec![
        initial.clone(),
        issue(0),
        json!({"module":module_with_objective(&initial, 0, "Measure the first comparison.")}),
        json!({"verdicts":[{"id":"finding-1","status":"unresolved","reason":"The project brief still names no measurement."}],"newFindings":[]}),
        json!({"module":module_with_objective(&initial, 0, "Measure the comparison with a stated outcome.")}),
        verdicts(&[("finding-1", "resolved")], vec![]),
    ]);
    let program = service::generate(
        &repo,
        &DraftRunRegistry::new(),
        &llm,
        request(LearningCourseDepth::Course),
        vec![],
    )
    .await?;
    assert_eq!(
        program.outline_review.as_ref().unwrap().status,
        LearningOutlineReviewStatus::Passed
    );
    let prompts = llm.prompts.lock().unwrap();
    assert_eq!(prompts.len(), 6);
    assert_ne!(
        payload(&prompts[2])["findings"][0]["instruction"],
        PLAN_SMALLEST
    );
    assert_eq!(
        payload(&prompts[4])["findings"][0]["instruction"],
        PLAN_SMALLEST
    );
    Ok(())
}

#[tokio::test]
async fn targeted_research_saves_pages_the_judge_then_reads_and_repair_tells_the_planner_what_was_searched(
) -> Result<()> {
    let repo = LearningRepository::new(crate::features::learning::tests::pool().await?);
    let claim = "Random assignment isolates the intervention and limits confounding when groups are compared.";
    let reason = "No saved reference states this; it needs a primary source.";
    // First run: the one page the search finds is irrelevant, the judge finds
    // the claim unsupported, and the narrowing rewrite comes back unchanged,
    // so the operation stops with the finding and its search on record.
    let irrelevant = ResearchWeb::new(&[SECOND]);
    let initial = with_summary(claim);
    let llm = model(vec![
        plan_output(&[("Random assignment", &[])]),
        initial.clone(),
        no_links(),
        findings(vec![factual_finding("/modules/0/summary", claim, reason)]),
        json!({"queries":["random assignment confounding primary source"]}),
        select_all(1),
        judged("unsupported", None),
        json!({"module":initial["modules"][0]}),
    ]);
    let sources = vec![original_source()];
    let drafts = DraftRunRegistry::new();
    let saved = service::generate_with_references_and_progress(
        &repo,
        &drafts,
        &llm,
        request(LearningCourseDepth::Course),
        sources.clone(),
        &references(&sources),
        &OutlineProgress::default(),
        Some(&irrelevant),
    )
    .await?;
    let review = saved.outline_review.as_ref().unwrap();
    assert_eq!(
        review.status,
        LearningOutlineReviewStatus::NeedsRepair,
        "{}",
        review.note
    );
    assert!(review.note.contains("no usable correction"));
    assert_eq!(review.issues.len(), 1);
    assert_eq!(review.issues[0].attempts, 1);
    assert_eq!(
        review.issues[0].searched,
        vec!["random assignment confounding primary source".to_owned()]
    );
    assert_eq!(review.references_gathered, 1);
    assert_eq!(saved.sources.len(), 2);
    let prompts = calls(&llm);
    assert_eq!(prompts.len(), 7);
    assert!(prompts[3].starts_with("Plan focused reference searches"));
    let planned = payload(&prompts[3]);
    assert_eq!(planned["topic"], "Experimental design module 0");
    assert_eq!(planned["gaps"][0]["claim"], claim);
    assert_eq!(planned["gaps"][0]["missingEvidence"], reason);
    assert_eq!(planned["alreadySearched"], json!([]));
    let selection = payload(&prompts[4]);
    assert_eq!(selection["topic"], "Experimental design module 0");
    assert_eq!(selection["gaps"][0]["missingEvidence"], reason);
    // The judge saw the page captured a moment earlier, never a snippet.
    assert!(prompts[5].starts_with("Source passages:"));
    assert!(!prompts[5].contains("SEARCH_SNIPPET"));
    assert_eq!(
        irrelevant.queries.lock().unwrap().as_slice(),
        ["random assignment confounding primary source"]
    );
    // Repair: attempts start afresh, the planner is told what was searched,
    // the new page supports the claim and the finding closes with evidence.
    // The page gathered in the first run offers new candidates, so linking
    // runs again first; the module receipts still hold because references
    // are not part of them, so the review stays incremental. Evidence closes
    // the finding in code, the module text is unchanged, so the receipt
    // still matches and no further review call is made.
    let relevant = ResearchWeb::with_pages(vec![Page {
        url: "https://example.org/primary-source".into(),
        text: format!("{claim} This holds because treatment and control differ only by chance."),
        truncated: false,
    }]);
    let llm = model(vec![
        no_links(),
        json!({"verdicts":[{"id":"finding-1","status":"unresolved","reason":"The summary still asserts the guarantee without support."}],"newFindings":[]}),
        json!({"queries":["random assignment confounding textbook"]}),
        select_all(1),
        judged("supported", Some(0)),
    ]);
    let repaired = repair_saved(
        &repo,
        &drafts,
        &llm,
        &RepairLearningOutlineRequestDto {
            program_id: saved.summary.id.clone(),
            expected_revision: saved.summary.revision,
        },
        &OutlineProgress::default(),
        Some(&relevant),
    )
    .await?;
    let review = repaired.outline_review.as_ref().unwrap();
    assert_eq!(
        review.status,
        LearningOutlineReviewStatus::Passed,
        "{}",
        review.note
    );
    assert!(review.issues.is_empty());
    assert_eq!(repaired.sources.len(), 3);
    let prompts = calls(&llm);
    assert_eq!(prompts.len(), 5);
    assert_eq!(payload(&prompts[0])["openFindings"][0]["attempts"], 0);
    assert_eq!(
        payload(&prompts[0])["reviewScope"]["mode"],
        "changed_modules"
    );
    assert_eq!(
        payload(&prompts[0])["reviewScope"]["moduleIndices"],
        json!([0])
    );
    assert_eq!(
        payload(&prompts[1])["alreadySearched"],
        json!(["random assignment confounding primary source"])
    );
    assert_eq!(
        relevant.queries.lock().unwrap().as_slice(),
        ["random assignment confounding textbook"]
    );
    let draft = repo.outline_draft(&repaired.summary.id).await?.unwrap();
    assert_eq!(
        draft.research_log["finding-1"],
        vec![
            "random assignment confounding primary source".to_owned(),
            "random assignment confounding textbook".to_owned()
        ]
    );
    assert_eq!(draft.candidate["modules"][0]["summary"]["sourceIndex"], 2);
    assert!(draft.candidate["modules"][0]["summary"]["quote"]
        .as_str()
        .unwrap()
        .starts_with(claim));
    let versions = repo.verification_sources(&repaired.summary.id).await?;
    assert_eq!(versions.len(), 3);
    assert!(versions
        .iter()
        .all(|s| !s.excerpt.contains("SEARCH_SNIPPET")));
    Ok(())
}

#[tokio::test]
async fn an_unchanged_last_attempt_leaves_a_plan_finding_as_an_advisory_and_passes() -> Result<()> {
    let repo = LearningRepository::new(crate::features::learning::tests::pool().await?);
    let initial = candidate();
    let first = module_with_objective(&initial, 0, "Measure the first comparison.");
    let llm = model(vec![
        initial.clone(),
        issue(0),
        json!({"module":first.clone()}),
        json!({"verdicts":[{"id":"finding-1","status":"unresolved","reason":"The project brief still names no measurement."}],"newFindings":[]}),
        // The last attempt returns the module unchanged.
        json!({"module":first}),
    ]);
    let program = service::generate(
        &repo,
        &DraftRunRegistry::new(),
        &llm,
        request(LearningCourseDepth::Course),
        vec![],
    )
    .await?;
    let review = program.outline_review.as_ref().unwrap();
    assert_eq!(
        review.status,
        LearningOutlineReviewStatus::Passed,
        "{}",
        review.note
    );
    assert!(!review.note.contains("no usable correction"));
    assert_eq!(review.issues.len(), 1);
    assert!(review.issues[0].exhausted, "kept as an advisory suggestion");
    assert_eq!(llm.prompts.lock().unwrap().len(), 5);
    Ok(())
}
