//! Incremental review receipts under optional citations. Carried over from
//! the deleted research tests; the review protocol itself is rebuilt next.
use super::outline_reference_tests::*;
use crate::features::learning::{
    dto::*,
    lessons::course_generation_tests::{outline, request},
    outline_draft::{
        repair_saved, DraftRunRegistry, LearningOutlineIssueDto, LearningOutlineIssueKind,
        LearningOutlineReviewStatus, OutlineDraft, RepairLearningOutlineRequestDto,
    },
    outline_progress::OutlineProgress,
    outline_review_scope::{ReviewReceipt, ReviewScope},
    repository::LearningRepository,
    service,
};
use crate::shared::error::Result;
use serde_json::{json, Value};

#[tokio::test]
async fn single_module_recheck_reuses_completed_checks_and_keeps_whole_course_connections(
) -> Result<()> {
    let repo = LearningRepository::new(crate::features::learning::tests::pool().await?);
    let mut initial = candidate();
    let mut source = original_source();
    let quote = ORIGINAL.repeat(12);
    source.excerpt = quote.clone();
    cite_at(&mut initial, "/modules/1/summary", 0, &quote);
    let mut fixed = initial["modules"][1].clone();
    fixed["project"]["brief"] = json!("Extend the Experimental design module 0 project with a measurement protocol and observable outcomes.");
    let llm = model(vec![
        initial,
        covers(&["/modules/1/summary"]),
        issue(1),
        json!({"module":fixed}),
        verdicts(&[("finding-1", "resolved")], vec![]),
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
    let prompts = llm.prompts.lock().unwrap();
    // Draft, coverage, full review, repair, incremental review. The repaired
    // module kept its verified citation, so no second coverage call was made.
    assert_eq!(prompts.len(), 5);
    let full = payload(&prompts[2]);
    let recheck = payload(&prompts[4]);
    assert_eq!(full["reviewScope"]["mode"], "full");
    // The reviewer never sees citation fields anywhere in the payload (source
    // passages are still supplied, so the text itself may appear there).
    assert!(full["candidate"]["modules"][1]["summary"]
        .get("quote")
        .is_none());
    assert!(full["candidate"]["modules"][1]["summary"]
        .get("sourceIndex")
        .is_none());
    assert_eq!(recheck["reviewScope"]["mode"], "changed_modules");
    assert_eq!(recheck["reviewScope"]["moduleIndices"], json!([1]));
    assert!(recheck.get("candidate").is_none());
    assert_eq!(recheck["modulesToReview"].as_array().unwrap().len(), 1);
    assert_eq!(recheck["modulesToReview"][0]["path"], "/modules/1");
    assert_eq!(recheck["modulesToReview"][0]["moduleNumber"], 2);
    assert!(recheck["modulesToReview"][0]["module"]["summary"]
        .get("quote")
        .is_none());
    assert_eq!(recheck["courseSequence"].as_array().unwrap().len(), 4);
    assert_eq!(
        recheck["courseSequence"][3]["prerequisiteIndices"],
        json!([2])
    );
    assert!(recheck["courseSequence"][3]["lessons"][0]["objective"]
        .as_str()
        .unwrap()
        .contains("limitations"));
    assert!(recheck["courseSequence"][3]["summary"]
        .get("quote")
        .is_none());
    drop(prompts);
    let saved = repo.outline_draft(&program.summary.id).await?.unwrap();
    assert_eq!(saved.candidate["modules"][1]["summary"]["quote"], quote);
    Ok(())
}

#[tokio::test]
async fn review_receipts_invalidate_on_evidence_model_request_structure_or_protocol_changes(
) -> Result<()> {
    let llm = model(vec![]);
    let mut initial = candidate();
    let sources = vec![original_source()];
    cite_at(&mut initial, "/modules/0/summary", 0, ORIGINAL);
    let mut draft = OutlineDraft::new(request(LearningCourseDepth::Course), initial, &sources);
    assert!(!ReviewScope::new(&draft, &sources, &llm).incremental);
    draft.completed_review = Some(ReviewReceipt::new(&draft, &sources, &llm));
    draft.candidate["modules"][1]["summary"]["text"] =
        json!("A corrected explanation of the prerequisite.");
    assert!(ReviewScope::new(&draft, &sources, &llm).incremental);
    let roundtrip: OutlineDraft = serde_json::from_str(&serde_json::to_string(&draft)?)?;
    assert!(ReviewScope::new(&roundtrip, &sources, &llm).incremental);
    for variant in 0..8 {
        let mut changed = draft.clone();
        let mut evidence = sources.clone();
        match variant {
            0 => evidence[0]
                .excerpt
                .push_str(" A correction to the reference."),
            1 => evidence[0].id = "different-version".into(),
            2 => changed
                .request
                .prior_knowledge
                .push_str(" No algebra knowledge."),
            3 => changed.request.minutes_per_session = 15,
            4 => changed.candidate["modules"]
                .as_array_mut()
                .unwrap()
                .swap(0, 2),
            5 => changed.candidate["modules"][2]["lessons"]
                .as_array_mut()
                .unwrap()
                .pop()
                .map(|_| ())
                .unwrap(),
            6 => {
                let mut state = serde_json::to_value(&changed)?;
                state["completed_review"]["context_hash"] = json!("an-earlier-review-protocol");
                changed = serde_json::from_value(state)?;
            }
            _ => evidence.push(original_source()),
        }
        assert!(
            !ReviewScope::new(&changed, &evidence, &llm).incremental,
            "variant {variant} must invalidate prior checks"
        );
    }
    assert!(
        !ReviewScope::new(&draft, &sources, &outline(4, 3)).incremental,
        "Changing the reviewer must invalidate previous checks"
    );
    draft.review.issues.push(LearningOutlineIssueDto {
        id: "finding-9".into(),
        path: String::new(),
        kind: LearningOutlineIssueKind::Plan,
        claim: "Course-wide contradiction".into(),
        message: "Revisit the whole course sequence.".into(),
        attempts: 0,
        exhausted: false,
        quote: String::new(),
        source_id: None,
        searched: vec![],
    });
    assert!(!ReviewScope::new(&draft, &sources, &llm).incremental);
    Ok(())
}

#[tokio::test]
async fn interrupted_incremental_review_resumes_from_last_completed_receipt_without_approval(
) -> Result<()> {
    let pool = crate::features::learning::tests::pool().await?;
    let repo = LearningRepository::new(pool.clone());
    let mut initial = candidate();
    let sources = vec![original_source()];
    cite_at(&mut initial, "/modules/0/summary", 0, ORIGINAL);
    let mut fixed = initial["modules"][1].clone();
    fixed["summary"]["text"] =
        json!("Use the earlier experiment protocol to compare observable outcomes.");
    let llm = model(vec![
        initial,
        covers(&["/modules/0/summary"]),
        issue(1),
        json!({"module":fixed}),
        json!({"invalid":"unfinished review"}),
    ]);
    let drafts = DraftRunRegistry::new();
    let saved = service::generate_with_references(
        &repo,
        &drafts,
        &llm,
        request(LearningCourseDepth::Course),
        sources.clone(),
        &references(&sources),
    )
    .await?;
    assert_eq!(
        saved.outline_review.as_ref().unwrap().status,
        LearningOutlineReviewStatus::Unchecked
    );
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
    let reopened = LearningRepository::new(pool);
    // The resumed check reuses the coverage receipt and the module receipts:
    // one incremental review call, nothing else.
    let llm = model(vec![verdicts(&[("finding-1", "resolved")], vec![])]);
    let result = repair_saved(
        &reopened,
        &drafts,
        &llm,
        &RepairLearningOutlineRequestDto {
            program_id: saved.summary.id,
            expected_revision: saved.summary.revision,
        },
        &OutlineProgress::default(),
        None,
    )
    .await?;
    assert_eq!(
        result.outline_review.as_ref().unwrap().status,
        LearningOutlineReviewStatus::Passed
    );
    let prompts = llm.prompts.lock().unwrap();
    assert_eq!(prompts.len(), 1);
    assert_eq!(
        payload(&prompts[0])["reviewScope"]["moduleIndices"],
        json!([1])
    );
    assert_eq!(
        payload(&prompts[0])["reviewScope"]["mode"],
        "changed_modules"
    );
    Ok(())
}

#[tokio::test]
async fn incremental_review_reports_cross_module_effects_course_wide() -> Result<()> {
    let repo = LearningRepository::new(crate::features::learning::tests::pool().await?);
    let mut initial = candidate();
    let sources = vec![original_source()];
    cite_at(&mut initial, "/modules/0/summary", 0, ORIGINAL);
    let mut fixed = initial["modules"][1].clone();
    fixed["project"]["brief"] =
        json!("Revise the experiment to use an observational measurement protocol.");
    // On the incremental pass only module 1 and the course as a whole are
    // allowed paths, so the effect on the final project is a course-wide
    // finding. Every module is then put to a rewrite; each comes back
    // unchanged, so the loop stops with the finding kept for the user.
    let unchanged: Vec<Value> = (0..4)
        .map(|i| json!({"module":if i == 1 { &fixed } else { &initial["modules"][i] }}))
        .collect();
    let mut outputs = vec![
        initial.clone(),
        covers(&["/modules/0/summary"]),
        issue(1),
        json!({"module":fixed}),
        verdicts(
            &[("finding-1", "resolved")],
            vec![plan_finding(
                "",
                "Interpret results and explain uncertainty.",
                "The changed earlier project no longer supplies the randomized comparison needed by the final project. Correct that dependency.",
            )],
        ),
    ];
    outputs.extend(unchanged);
    let llm = model(outputs);
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
    assert_eq!(review.status, LearningOutlineReviewStatus::NeedsRepair);
    assert_eq!(review.issues.len(), 1);
    assert_eq!(review.issues[0].path, "");
    assert_eq!(review.issues[0].id, "finding-2");
    assert_eq!(review.issues[0].attempts, 1);
    assert!(review.note.contains("no usable correction"));
    let prompts = llm.prompts.lock().unwrap();
    assert_eq!(prompts.len(), 9);
    let recheck = payload(&prompts[4]);
    assert_eq!(recheck["openFindings"][0]["id"], "finding-1");
    assert_eq!(recheck["openFindings"][0]["attempts"], 1);
    Ok(())
}
