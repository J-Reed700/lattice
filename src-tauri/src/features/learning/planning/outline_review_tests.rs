//! The review protocol: a verdict per open finding, new findings only where
//! allowed, stable ids, dedupe, the skip rule, and the kind backstops.
use super::outline_reference_tests::*;
use crate::features::learning::{
    dto::*,
    lessons::course_generation_tests::request,
    outline_draft::{
        repair_saved, DraftRunRegistry, LearningOutlineIssueKind, LearningOutlineReviewStatus,
        RepairLearningOutlineRequestDto,
    },
    outline_progress::OutlineProgress,
    repository::LearningRepository,
    service,
};
use lattice_core::shared::error::Result;
use serde_json::{json, Value};

fn changed_module(candidate: &Value, index: usize, objective: &str) -> Value {
    let mut module = candidate["modules"][index].clone();
    module["lessons"][0]["objective"] = json!(objective);
    json!({"module":module})
}

async fn generate(
    repo: &LearningRepository,
    llm: &crate::features::learning::lessons::course_generation_tests::ScriptedModel,
) -> Result<LearningProgramDto> {
    service::generate(
        repo,
        &DraftRunRegistry::new(),
        llm,
        request(LearningCourseDepth::Course),
        vec![],
    )
    .await
}

#[tokio::test]
async fn an_open_finding_without_a_verdict_stays_open_and_is_asked_about_again() -> Result<()> {
    let repo = LearningRepository::new(crate::features::learning::tests::pool().await?);
    let initial = candidate();
    // The second review omits finding-1's verdict. That never stops the
    // outline: the finding stays open, gets its second attempt, and the third
    // review resolves it.
    let llm = model(vec![
        initial.clone(),
        issue(0),
        changed_module(
            &initial,
            0,
            "Compare the groups with an observable measurement.",
        ),
        clean(),
        changed_module(
            &initial,
            0,
            "Compare the groups with one stated, observable measurement.",
        ),
        verdicts(&[("finding-1", "resolved")], vec![]),
    ]);
    let program = generate(&repo, &llm).await?;
    let review = program.outline_review.as_ref().unwrap();
    assert_eq!(
        review.status,
        LearningOutlineReviewStatus::Passed,
        "{}",
        review.note
    );
    assert!(review.issues.is_empty());
    let prompts = llm.prompts.lock().unwrap();
    assert_eq!(prompts.len(), 6);
    // The third review still asked about finding-1.
    assert_eq!(payload(&prompts[5])["openFindings"][0]["id"], "finding-1");
    Ok(())
}

#[tokio::test]
async fn new_findings_outside_the_reviewed_modules_are_dropped() -> Result<()> {
    let repo = LearningRepository::new(crate::features::learning::tests::pool().await?);
    let initial = candidate();
    // Module 0 changed; the incremental pass may report on module 0 only,
    // never on an unchanged module.
    let llm = model(vec![
        initial.clone(),
        issue(0),
        changed_module(
            &initial,
            0,
            "Compare the groups with an observable measurement.",
        ),
        verdicts(
            &[("finding-1", "resolved")],
            vec![plan_finding(
                "/modules/2",
                "Interpret results and explain uncertainty.",
                "This unchanged module repeats the previous project deliverable.",
            )],
        ),
    ]);
    let program = generate(&repo, &llm).await?;
    let review = program.outline_review.as_ref().unwrap();
    // The unchanged module is not re-litigated: its finding is dropped.
    assert_eq!(
        review.status,
        LearningOutlineReviewStatus::Passed,
        "{}",
        review.note
    );
    assert!(review.issues.is_empty());
    let prompts = llm.prompts.lock().unwrap();
    let schema_paths = |prompt: &str| -> Vec<String> {
        // The reviewer's allowed paths are the enum in the response schema; the
        // fixture records only the prompt, so check the scope instead.
        payload(prompt)["reviewScope"]["moduleIndices"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.to_string())
            .collect()
    };
    assert_eq!(schema_paths(&prompts[3]), vec!["0".to_owned()]);
    Ok(())
}

#[tokio::test]
async fn verdicts_close_or_refresh_findings_and_the_reviewer_sees_current_text() -> Result<()> {
    let repo = LearningRepository::new(crate::features::learning::tests::pool().await?);
    let initial = candidate();
    let first = "Measure the effect of random assignment on the comparison.";
    let second = "Measure the effect of random assignment on a stated, observable outcome.";
    let llm = model(vec![
        initial.clone(),
        findings(vec![plan_finding(
            "/modules/0/lessons/0",
            "Design a controlled experiment and explain its limitations.",
            "The objective does not name what is measured.",
        )]),
        changed_module(&initial, 0, first),
        json!({"verdicts":[{"id":"finding-1","status":"unresolved","reason":"The measurement is still not observable."}],"newFindings":[]}),
        changed_module(&initial, 0, second),
        verdicts(&[("finding-1", "resolved")], vec![]),
    ]);
    let program = generate(&repo, &llm).await?;
    let review = program.outline_review.as_ref().unwrap();
    assert_eq!(review.status, LearningOutlineReviewStatus::Passed);
    assert!(review.issues.is_empty());
    assert_eq!(review.repair_passes, 2);
    let prompts = llm.prompts.lock().unwrap();
    assert_eq!(prompts.len(), 6);
    // The first review's claim is the item text; the unresolved verdict
    // refreshed it to the rewritten text and replaced the reason.
    let opened = payload(&prompts[1]);
    assert_eq!(opened["openFindings"], json!([]));
    assert!(opened.get("verifiedAssertions").is_none());
    // Open findings are refreshed to the current text before the reviewer
    // sees them, so a verdict is never asked about stale wording.
    let second_review = payload(&prompts[3]);
    assert_eq!(second_review["openFindings"][0]["claim"], first);
    let third_review = payload(&prompts[5]);
    assert_eq!(third_review["openFindings"][0]["id"], "finding-1");
    assert_eq!(third_review["openFindings"][0]["claim"], second);
    assert_eq!(
        third_review["openFindings"][0]["reason"],
        "The measurement is still not observable."
    );
    assert_eq!(third_review["openFindings"][0]["attempts"], 2);
    assert_eq!(third_review["openFindings"][0]["kind"], "plan");
    assert!(!prompts[3].contains("previousFindings"));
    Ok(())
}

#[tokio::test]
async fn a_repeated_finding_at_the_same_path_merges_into_the_open_one() -> Result<()> {
    let repo = LearningRepository::new(crate::features::learning::tests::pool().await?);
    let initial = candidate();
    // Two findings on one lesson objective, worded differently, are one finding
    // with the later reason. The repair call then fails for want of a reply,
    // which leaves the reviewed state saved for inspection.
    let llm = model(vec![
        initial.clone(),
        findings(vec![
            plan_finding(
                "/modules/0/lessons/0",
                "Design a controlled experiment",
                "The objective does not name what is measured.",
            ),
            plan_finding(
                "/modules/0/lessons/0",
                "...and explain its limitations.",
                "The objective names no measurement and no comparison.",
            ),
        ]),
    ]);
    let program = generate(&repo, &llm).await?;
    assert_eq!(
        program.outline_review.as_ref().unwrap().status,
        LearningOutlineReviewStatus::Unchecked
    );
    let saved = repo.outline_draft(&program.summary.id).await?.unwrap();
    assert_eq!(saved.review.issues.len(), 1);
    assert_eq!(saved.review.issues[0].id, "finding-1");
    assert_eq!(
        saved.review.issues[0].claim,
        "Design a controlled experiment and explain its limitations."
    );
    assert_eq!(
        saved.review.issues[0].message,
        "The objective names no measurement and no comparison."
    );
    assert_eq!(saved.findings_raised, 1);
    Ok(())
}

#[tokio::test]
async fn a_passed_draft_is_repaired_without_a_review_call() -> Result<()> {
    let repo = LearningRepository::new(crate::features::learning::tests::pool().await?);
    let llm = model(vec![candidate(), clean()]);
    let program = generate(&repo, &llm).await?;
    let review = program.outline_review.as_ref().unwrap();
    assert_eq!(review.status, LearningOutlineReviewStatus::Passed);
    assert_eq!(llm.prompts.lock().unwrap().len(), 2);
    let silent = model(vec![]);
    let repaired = repair_saved(
        &repo,
        &DraftRunRegistry::new(),
        &silent,
        &RepairLearningOutlineRequestDto {
            program_id: program.summary.id.clone(),
            expected_revision: program.summary.revision,
        },
        &OutlineProgress::default(),
        None,
    )
    .await?;
    let review = repaired.outline_review.as_ref().unwrap();
    assert_eq!(review.status, LearningOutlineReviewStatus::Passed);
    assert!(review.note.starts_with("Outline checks passed"));
    assert_eq!(review.repair_passes, 0);
    assert_eq!(silent.prompts.lock().unwrap().len(), 0);
    Ok(())
}

#[tokio::test]
async fn factual_findings_on_plan_items_are_recorded_as_plan_findings() -> Result<()> {
    let repo = LearningRepository::new(crate::features::learning::tests::pool().await?);
    let initial = candidate();
    // A "factual" finding on a lesson objective is a plan finding: never
    // researched, never narrowed to fit a passage. The first repair call has
    // no reply, so the reviewed state is what is saved.
    let llm = model(vec![
        initial.clone(),
        findings(vec![factual_finding(
            "/modules/0/lessons/0",
            "Design a controlled experiment and explain its limitations.",
            "No single experiment design suits every question; this is overstated.",
        )]),
    ]);
    let program = generate(&repo, &llm).await?;
    let saved = repo.outline_draft(&program.summary.id).await?.unwrap();
    assert_eq!(saved.review.issues.len(), 1);
    assert_eq!(saved.review.issues[0].kind, LearningOutlineIssueKind::Plan);
    assert_eq!(saved.review.issues[0].path, "/modules/0/lessons/0");
    let prompts = llm.prompts.lock().unwrap();
    assert_eq!(prompts.len(), 3);
    assert!(prompts[2].starts_with("Repair a saved curriculum module"));
    assert!(!prompts
        .iter()
        .any(|p| p.starts_with("Plan focused reference searches")));
    Ok(())
}

#[tokio::test]
async fn the_reviewer_cannot_raise_a_course_wide_finding() -> Result<()> {
    let repo = LearningRepository::new(crate::features::learning::tests::pool().await?);
    // The empty path is not an allowed path on any pass: the schema enum
    // excludes it and a finding that uses it anyway is dropped, so nothing is
    // saved from it and no module is rewritten for it.
    let llm = model(vec![
        candidate(),
        findings(vec![plan_finding(
            "",
            "The course claims to cover experimental design in four modules.",
            "Four modules cannot cover the stated scope.",
        )]),
    ]);
    let program = generate(&repo, &llm).await?;
    let review = program.outline_review.as_ref().unwrap();
    assert_eq!(
        review.status,
        LearningOutlineReviewStatus::Passed,
        "{}",
        review.note
    );
    assert!(review.issues.is_empty());
    assert_eq!(llm.prompts.lock().unwrap().len(), 2);
    Ok(())
}

#[tokio::test]
async fn a_verified_assertion_is_listed_to_the_reviewer_and_a_repeat_is_dropped_by_code(
) -> Result<()> {
    let repo = LearningRepository::new(crate::features::learning::tests::pool().await?);
    let mut initial = candidate();
    initial["modules"][0]["summary"]["text"] = json!(ADDITIONAL);
    let mut source = original_source();
    source.excerpt = ADDITIONAL.into();
    let sources = vec![source];
    // The strict judge supports the summary's assertion and closes it in code.
    // A plan finding on the same module forces a rewrite, so the module is
    // reviewed again: the reviewer is told the assertion is verified, raises
    // it anyway, and code drops the repeat.
    let objective = "Compare the groups with an observable measurement.";
    let llm = model(vec![
        initial.clone(),
        no_links(),
        findings(vec![
            factual_finding(
                "/modules/0/summary",
                ADDITIONAL,
                "This assertion needs evidence from the saved references.",
            ),
            plan_finding(
                "/modules/0/lessons/0",
                "Design a controlled experiment and explain its limitations.",
                "Name the measurement this lesson compares.",
            ),
        ]),
        judged("supported", Some(0)),
        no_objections(),
        changed_module(&initial, 0, objective),
        no_links(),
        verdicts(
            &[("finding-2", "resolved")],
            vec![factual_finding(
                "/modules/0/summary",
                ADDITIONAL,
                "This assertion still needs evidence.",
            )],
        ),
    ]);
    let program = service::generate_with_references(
        &repo,
        &DraftRunRegistry::new(),
        &llm,
        request(LearningCourseDepth::Course),
        sources.clone(),
        &references(&sources),
    )
    .await?;
    let review = program.outline_review.as_ref().unwrap();
    assert_eq!(
        review.status,
        LearningOutlineReviewStatus::Passed,
        "{}",
        review.note
    );
    assert!(review.issues.is_empty());
    let prompts = calls(&llm);
    assert_eq!(prompts.len(), 6);
    assert!(prompts[4].starts_with("Repair a saved curriculum module"));
    let recheck = payload(&prompts[5]);
    assert_eq!(recheck["verifiedAssertions"], json!([ADDITIONAL]));
    assert_eq!(recheck["openFindings"].as_array().unwrap().len(), 1);
    assert_eq!(recheck["openFindings"][0]["id"], "finding-2");
    assert!(prompts[5].contains("do not raise them again"));
    // Two findings were raised by the first review; the repeat got no id.
    let saved = repo.outline_draft(&program.summary.id).await?.unwrap();
    assert_eq!(saved.findings_raised, 2);
    Ok(())
}
