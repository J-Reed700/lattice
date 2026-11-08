//! The build as a whole: six stages in a fixed order, a checkpoint after each of the first five, and a resumed job
//! that starts at the first stage without one and repeats no model call and no run.

use super::support::{
    blind_answer, blind_assessment, clean_code_answer, code, entry_answer, files, judge_answer,
    plant_answer, planted, proved, purpose, quiet, review, review_answer, spec, system, FakeGrader,
    FakeRunner, Harness, ScriptedModel, KILLING_TEST, MODEL, SUITE,
};
use crate::drills::build::{DrillBuild, Evidence, ModelCall, Stage};
use crate::drills::material::CodeMaterial;
use crate::drills::repository::{self, StoredDrill};
use crate::drills::runner::{CodeRunner, RunOutcome};
use crate::persistence::db;
use lattice_core::domain::learning::drills::{Origin, Proof};
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::runtime::jobs::{JobFailure, JobOutcome, JobStatus};
use serde_json::json;
use tokio_util::sync::CancellationToken;

/// What the model answers, call after call, when everything goes well: the code, no repairs, the mutations, the
/// judge of the smell (the killed one asks nothing), the blind review and the one extra described.
fn answers() -> Vec<String> {
    vec![
        clean_code_answer(),
        review_answer(vec![]),
        plant_answer(),
        judge_answer(true),
        blind_answer(),
        entry_answer(),
    ]
}

const PURPOSES: [&str; 6] = [
    "clean code",
    "clean review",
    "plant",
    "judge",
    "blind review",
    "key entry",
];

async fn stored(harness: &Harness) -> Result<StoredDrill> {
    let mut connection = harness.pool.acquire().await.map_err(db)?;
    repository::oldest_ready_in(&mut connection, "python")
        .await?
        .ok_or_else(|| AppError::NotFound("the stored drill".into()))
}

fn every_call(evidence: &Evidence) -> Vec<ModelCall> {
    let mut calls = evidence.clean_code.calls.clone();
    calls.extend(evidence.clean_review.trail.calls.clone());
    calls.extend(evidence.plant.calls.clone());
    for proof in &evidence.proofs {
        calls.extend(proof.trail.calls.clone());
    }
    for dropped in &evidence.dropped {
        calls.extend(dropped.trail.calls.clone());
    }
    calls.extend(evidence.blind_review.trail.calls.clone());
    calls
}

#[tokio::test]
async fn the_stages_run_in_order_each_leaves_a_checkpoint_and_the_drill_is_stored() -> Result<()> {
    let harness = Harness::new(&spec()).await?;
    let model = ScriptedModel::new(answers());
    let runner = FakeRunner::killing_the_operator();
    let grader = FakeGrader::new(blind_assessment());
    let context = harness.context().await?;
    let outcome = harness
        .build(&model, Some(&runner), &grader)
        .run(&context, &spec())
        .await?;
    assert_eq!(outcome, JobOutcome::Committed);
    assert_eq!(model.purposes(), PURPOSES);
    assert_eq!(model.unused(), 0);

    assert_eq!(
        Stage::ALL.map(|stage| stage.key()),
        [
            "drill-build-v1:clean_code",
            "drill-build-v1:clean_review",
            "drill-build-v1:plant",
            "drill-build-v1:prove",
            "drill-build-v1:blind_review"
        ]
    );
    for stage in Stage::ALL {
        assert!(
            harness
                .store
                .checkpoint(&harness.job, &stage.key())
                .await?
                .is_some(),
            "{}",
            stage.key()
        );
    }

    let job = harness.store.get(&harness.job).await?;
    assert_eq!(job.status, JobStatus::Completed);
    assert_eq!(job.progress_current, job.progress_total);

    let drill = stored(&harness).await?;
    assert_eq!(job.result_ref, Some(format!("drill:{}", drill.drill.id)));
    let ids: Vec<&str> = drill
        .drill
        .key
        .iter()
        .map(|issue| issue.id.as_str())
        .collect();
    assert_eq!(ids, ["planted-1", "planted-2", "review-1"]);
    assert_eq!(
        drill.drill.key[0].proof,
        Proof::Killed {
            test: KILLING_TEST.into()
        }
    );
    assert!(matches!(&drill.drill.key[1].proof, Proof::Judged { .. }));
    assert_eq!(drill.drill.key[2].origin, Origin::FoundInReview);
    assert_eq!(drill.clean_files, files());
    assert_eq!(drill.suite.as_deref(), Some(SUITE));
    assert_eq!(drill.runner.as_deref(), Some("fake-python"));
    assert_eq!(drill.domain, "rate limiting");

    // The evidence names the model of every call, and holds the runs and the judge's reason.
    let calls = every_call(&drill.evidence);
    assert_eq!(calls.len(), PURPOSES.len());
    assert!(calls.iter().all(|call| call.model == MODEL));
    assert_eq!(drill.evidence.proofs[0].trail.runs.len(), 2);
    assert_eq!(drill.evidence.blind_review.extras, ["review-1"]);
    assert_eq!(drill.evidence.blind_review.findability.len(), 2);
    assert!(drill.evidence.dropped.is_empty());
    Ok(())
}

#[tokio::test]
async fn progress_is_reported_for_each_stage() -> Result<()> {
    let harness = Harness::new(&spec()).await?;
    let model = ScriptedModel::new(answers());
    let runner = FakeRunner::killing_the_operator();
    let grader = FakeGrader::new(blind_assessment());
    let context = harness.context().await?;
    harness
        .build(&model, Some(&runner), &grader)
        .run(&context, &spec())
        .await?;
    let messages: Vec<String> = sqlx::query_scalar(
        "SELECT message FROM job_events WHERE job_id=? AND message LIKE '% of 6)' ORDER BY ordinal",
    )
    .bind(&harness.job)
    .fetch_all(&harness.pool)
    .await
    .map_err(db)?;
    assert_eq!(
        messages,
        [
            "Writing the code (1 of 6)",
            "Reviewing the code (2 of 6)",
            "Planting the defects (3 of 6)",
            "Proving the defects (4 of 6)",
            "Reviewing the drill blind (5 of 6)",
            "Storing the drill (6 of 6)"
        ]
    );
    Ok(())
}

/// The run a crash interrupts after `done` stages, then the run that resumes it.
async fn crash_after(done: usize, calls_before: usize) -> Result<()> {
    let harness = Harness::new(&spec()).await?;

    let first = ScriptedModel::new(answers().into_iter().take(calls_before));
    let runner = FakeRunner::killing_the_operator();
    let grader = FakeGrader::new(blind_assessment());
    let context = harness.context().await?;
    let error = harness
        .build(&first, Some(&runner), &grader)
        .run(&context, &spec())
        .await
        .unwrap_err();
    assert!(
        matches!(&error, AppError::InternalError(message) if message.contains("Unexpected extra model call")),
        "{error:?}"
    );
    for (position, stage) in Stage::ALL.iter().enumerate() {
        let saved = harness.store.checkpoint(&harness.job, &stage.key()).await?;
        assert_eq!(
            saved.is_some(),
            position < done,
            "{} after {done}",
            stage.key()
        );
    }

    let second = ScriptedModel::new(answers().into_iter().skip(calls_before));
    let runner = FakeRunner::killing_the_operator();
    let grader = FakeGrader::new(blind_assessment());
    let context = harness.context().await?;
    let outcome = harness
        .build(&second, Some(&runner), &grader)
        .run(&context, &spec())
        .await?;
    assert_eq!(outcome, JobOutcome::Committed);
    assert_eq!(
        second.purposes(),
        PURPOSES[calls_before..],
        "only the stages without a checkpoint are asked"
    );
    assert_eq!(second.unused(), 0);
    if done > 0 {
        assert!(
            !runner
                .runs()
                .iter()
                .any(|(run_files, test)| test == SUITE && *run_files == files()),
            "the suite is not run on the clean code again"
        );
    }
    let drill = stored(&harness).await?;
    assert_eq!(drill.drill.key.len(), 3);
    Ok(())
}

#[tokio::test]
async fn a_job_resumed_after_each_checkpoint_repeats_no_call() -> Result<()> {
    // The number of calls the first run makes, per stage done: 1, 2, 3 and 4.
    for (done, calls_before) in [(1, 1), (2, 2), (3, 3), (4, 4)] {
        crash_after(done, calls_before).await?;
    }
    Ok(())
}

#[tokio::test]
async fn a_job_resumed_with_every_checkpoint_stores_the_drill_with_no_call_and_no_run() -> Result<()>
{
    let harness = Harness::new(&spec()).await?;
    let outputs = [
        (Stage::CleanCode, serde_json::to_value(code())?),
        (Stage::CleanReview, serde_json::to_value(review())?),
        (Stage::Plant, serde_json::to_value(planted())?),
        (Stage::Prove, serde_json::to_value(proved())?),
        (Stage::BlindReview, serde_json::to_value(quiet())?),
    ];
    for (stage, output) in &outputs {
        assert!(
            harness
                .store
                .put_checkpoint(&harness.job, &stage.key(), output)
                .await?
        );
    }
    let model = ScriptedModel::new([]);
    let runner = FakeRunner::passing();
    let grader = FakeGrader::failing("unused");
    let context = harness.context().await?;
    let outcome = harness
        .build(&model, Some(&runner), &grader)
        .run(&context, &spec())
        .await?;
    assert_eq!(outcome, JobOutcome::Committed);
    assert!(model.requests().is_empty());
    assert!(runner.runs().is_empty());
    assert!(grader.seen().is_empty());
    let drill = stored(&harness).await?;
    assert_eq!(drill.drill.id, code().drill_id);
    assert_eq!(drill.drill.key, proved().planted_key());
    Ok(())
}

#[tokio::test]
async fn a_checkpoint_that_does_not_read_is_made_again() -> Result<()> {
    let harness = Harness::new(&spec()).await?;
    harness
        .store
        .put_checkpoint(
            &harness.job,
            &Stage::CleanCode.key(),
            &json!({"nonsense": true}),
        )
        .await?;
    let model = ScriptedModel::new(answers());
    let runner = FakeRunner::killing_the_operator();
    let grader = FakeGrader::new(blind_assessment());
    let context = harness.context().await?;
    let outcome = harness
        .build(&model, Some(&runner), &grader)
        .run(&context, &spec())
        .await?;
    assert_eq!(outcome, JobOutcome::Committed);
    assert_eq!(model.purposes(), PURPOSES);
    Ok(())
}

#[tokio::test]
async fn a_run_cancelled_under_the_builds_token_stops_the_job_and_saves_nothing_of_its_stage(
) -> Result<()> {
    let harness = Harness::new(&spec()).await?;
    let cancel = CancellationToken::new();
    let trigger = cancel.clone();
    let runner = FakeRunner::new(move |_, _| {
        trigger.cancel();
        RunOutcome::Cancelled
    });
    let model = ScriptedModel::new(answers());
    let grader = FakeGrader::new(blind_assessment());
    let context = harness.context().await?;
    let outcome = DrillBuild::new(
        &model,
        Box::new(CodeMaterial::new(Some(&runner as &dyn CodeRunner))),
        &grader,
        &harness.pool,
        cancel,
    )
    .run(&context, &spec())
    .await?;
    assert_eq!(outcome, JobOutcome::Stopped);
    assert_eq!(
        model.requests().len(),
        1,
        "the answer is not asked for again"
    );
    for stage in Stage::ALL {
        assert!(
            harness
                .store
                .checkpoint(&harness.job, &stage.key())
                .await?
                .is_none(),
            "{}",
            stage.key()
        );
    }
    assert_eq!(
        harness.store.get(&harness.job).await?.status,
        JobStatus::Running,
        "the runtime then suspends it to resume"
    );
    Ok(())
}

#[tokio::test]
async fn a_suite_that_never_finishes_fails_the_job_and_a_missing_runtime_defers_it() -> Result<()> {
    let harness = Harness::new(&spec()).await?;
    let model = ScriptedModel::new([clean_code_answer(), clean_code_answer()]);
    let runner = FakeRunner::new(|_, _| RunOutcome::TimedOut {
        output: String::new(),
    });
    let grader = FakeGrader::new(blind_assessment());
    let context = harness.context().await?;
    let error = harness
        .build(&model, Some(&runner), &grader)
        .run(&context, &spec())
        .await
        .unwrap_err();
    assert!(
        matches!(error, AppError::ModelAnswerUnusable(_)),
        "{error:?}"
    );
    assert!(
        matches!(JobFailure::classify(&error), JobFailure::Fail { .. }),
        "a suite or code that loops must not be retried for ever"
    );
    assert_eq!(model.requests().len(), 2);

    let model = ScriptedModel::new([clean_code_answer()]);
    let runner = FakeRunner::new(|_, _| RunOutcome::Unavailable {
        why: "The bundled Python runtime is missing.".into(),
    });
    let error = harness
        .build(&model, Some(&runner), &grader)
        .run(&context, &spec())
        .await
        .unwrap_err();
    assert!(
        matches!(JobFailure::classify(&error), JobFailure::Defer(_)),
        "{error:?}"
    );
    assert_eq!(model.requests().len(), 1);
    Ok(())
}

#[tokio::test]
async fn no_prompt_forbids_what_a_pattern_needs() -> Result<()> {
    let harness = Harness::new(&spec()).await?;
    let model = ScriptedModel::new(answers());
    let runner = FakeRunner::killing_the_operator();
    let grader = FakeGrader::new(blind_assessment());
    let context = harness.context().await?;
    harness
        .build(&model, Some(&runner), &grader)
        .run(&context, &spec())
        .await?;
    let requests = model.requests();
    for request in &requests {
        let system = system(request);
        for forbidden in ["rename nothing", "Add no comment, rename"] {
            assert!(
                !system.contains(forbidden),
                "{}: {forbidden}",
                purpose(request)
            );
        }
    }
    let plant = requests
        .iter()
        .find(|request| purpose(request) == "plant")
        .map(system)
        .unwrap_or_default();
    assert!(
        plant.contains("Add no comment, name or string that points at the defect or admits it."),
        "a rename, a comment or a string that does not point at the defect is allowed"
    );
    Ok(())
}
