//! Incremental review receipts under optional citations. Carried over from
//! the deleted research tests; the review protocol itself is rebuilt next.
use super::outline_reference_tests::*;
use crate::features::learning::{
    dto::*,
    lessons::course_generation_tests::{outline, request},
    outline_linking,
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
use serde_json::json;

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
        links(&[("/modules/1/summary", ORIGINAL)]),
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
    let prompts = calls(&llm);
    // Draft, full review, repair, incremental review. The repaired module kept
    // its verified citation and changed no item text, so every linking call
    // precedes the first review and none follows the repair.
    assert_eq!(prompts.len(), 4);
    {
        let all = llm.prompts.lock().unwrap();
        let last_link = all.iter().rposition(|p| p.starts_with(outline_linking::LINK_SYSTEM)).unwrap();
        let first_review = all.iter().position(|p| p.starts_with("Review instructional quality.")).unwrap();
        assert!(last_link < first_review);
    }
    let full = payload(&prompts[1]);
    let recheck = payload(&prompts[3]);
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
    let saved = repo.outline_draft(&program.summary.id).await?.unwrap();
    assert_eq!(saved.candidate["modules"][1]["summary"]["quote"], quote);
    Ok(())
}

#[tokio::test]
async fn review_receipts_survive_new_references_and_invalidate_on_model_request_structure_or_protocol_changes(
) -> Result<()> {
    let llm = model(vec![]);
    let mut initial = candidate();
    let sources = vec![original_source()];
    cite_at(&mut initial, "/modules/0/summary", 0, ORIGINAL);
    let mut draft = OutlineDraft::new(request(LearningCourseDepth::Course), initial, &sources);
    assert!(!ReviewScope::new(&draft, &llm).incremental);
    draft.completed_review = Some(ReviewReceipt::new(&draft, &llm));
    draft.candidate["modules"][1]["summary"]["text"] =
        json!("A corrected explanation of the prerequisite.");
    assert!(ReviewScope::new(&draft, &llm).incremental);
    let roundtrip: OutlineDraft = serde_json::from_str(&serde_json::to_string(&draft)?)?;
    assert!(ReviewScope::new(&roundtrip, &llm).incremental);
    // References are not part of the receipt: pages gathered for one finding
    // never reopen a module that passed. The draft's own source list growing
    // or a citation being linked on an unchanged module changes nothing.
    let mut with_more_references = draft.clone();
    with_more_references.source_ids.push("gathered-later".into());
    with_more_references.review.references_gathered += 1;
    assert!(ReviewScope::new(&with_more_references, &llm).incremental);
    let mut relinked = draft.clone();
    cite_at(&mut relinked.candidate, "/modules/2/outcomes/0", 0, ORIGINAL);
    relinked.candidate["modules"][0]["summary"]["sourceIndex"] = json!(null);
    relinked.candidate["modules"][0]["summary"]["quote"] = json!("");
    assert_eq!(
        ReviewReceipt::new(&relinked, &llm),
        ReviewReceipt::new(&draft, &llm)
    );
    for variant in 0..5 {
        let mut changed = draft.clone();
        match variant {
            0 => changed
                .request
                .prior_knowledge
                .push_str(" No algebra knowledge."),
            1 => changed.request.minutes_per_session = 15,
            2 => changed.candidate["modules"]
                .as_array_mut()
                .unwrap()
                .swap(0, 2),
            3 => changed.candidate["modules"][2]["lessons"]
                .as_array_mut()
                .unwrap()
                .pop()
                .map(|_| ())
                .unwrap(),
            _ => {
                let mut state = serde_json::to_value(&changed)?;
                state["completed_review"]["context_hash"] = json!("an-earlier-review-protocol");
                changed = serde_json::from_value(state)?;
            }
        }
        assert!(
            !ReviewScope::new(&changed, &llm).incremental,
            "variant {variant} must invalidate prior checks"
        );
    }
    assert!(
        !ReviewScope::new(&draft, &outline(4, 3)).incremental,
        "Changing the reviewer must invalidate previous checks"
    );
    // Only the code-generated References finding names no module; it needs
    // the whole course.
    draft.review.issues.push(LearningOutlineIssueDto {
        id: "references".into(),
        path: String::new(),
        kind: LearningOutlineIssueKind::References,
        claim: String::new(),
        message: "No references could be gathered.".into(),
        attempts: 0,
        exhausted: true,
        quote: String::new(),
        source_id: None,
        searched: vec![],
    });
    assert!(!ReviewScope::new(&draft, &llm).incremental);
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
    // The rewrite changes the summary text, so that one item is linked again
    // after the repair (nothing qualifies) before the review that fails.
    let llm = model(vec![
        initial,
        links(&[("/modules/0/summary", ORIGINAL)]),
        issue(1),
        json!({"module":fixed}),
        no_links(),
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
    // The resumed check reuses the link receipts and the module receipts:
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
async fn incremental_review_anchors_cross_module_effects_to_the_module_that_must_change(
) -> Result<()> {
    let repo = LearningRepository::new(crate::features::learning::tests::pool().await?);
    let mut initial = candidate();
    let sources = vec![original_source()];
    cite_at(&mut initial, "/modules/0/summary", 0, ORIGINAL);
    let mut fixed = initial["modules"][1].clone();
    fixed["project"]["brief"] =
        json!("Revise the experiment to use an observational measurement protocol.");
    // On the incremental pass only module 1 is an allowed path: there is no
    // course-wide path, so the effect on the final project is reported at the
    // edited module. Only that module is put to a rewrite; it comes back
    // unchanged, so the loop stops with the finding kept for the user.
    let llm = model(vec![
        initial.clone(),
        links(&[("/modules/0/summary", ORIGINAL)]),
        issue(1),
        json!({"module":fixed}),
        verdicts(
            &[("finding-1", "resolved")],
            vec![plan_finding(
                "/modules/1",
                "Experimental design module 1",
                "The changed project no longer supplies the randomized comparison the final project needs. Restore that dependency here.",
            )],
        ),
        json!({"module":fixed}),
    ]);
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
    assert_eq!(review.issues[0].path, "/modules/1");
    assert_eq!(review.issues[0].id, "finding-2");
    assert_eq!(review.issues[0].attempts, 1);
    assert!(review.note.contains("no usable correction"));
    let prompts = calls(&llm);
    // Draft, full review, repair, incremental review, one repair: no other
    // module is rewritten for a problem anchored to module 1.
    assert_eq!(prompts.len(), 5);
    let recheck = payload(&prompts[3]);
    assert_eq!(recheck["openFindings"][0]["id"], "finding-1");
    assert_eq!(recheck["openFindings"][0]["attempts"], 1);
    assert_eq!(payload(&prompts[4])["moduleIndex"], 1);
    Ok(())
}
