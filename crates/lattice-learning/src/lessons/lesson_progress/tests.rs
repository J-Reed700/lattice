#![allow(clippy::unwrap_used, clippy::panic)]
use super::*;
use crate::lessons::call_plan::PlannedAt;
use lattice_core::application::services::model_contract::{
    Contract, ContractCall, ContractId, PromptInput,
};
use lattice_core::domain::learning::acceptance::StandardPolicy;
use lattice_core::shared::runtime::jobs::{current_observer, QueueExit};

struct Probe;

impl Contract for Probe {
    const ID: ContractId = ContractId::new("progress-probe");
    type Input = PromptInput;
}

#[tokio::test]
async fn structured_progress_tracks_repeated_phases_and_concurrent_model_requests() {
    let progress = Progress::default();
    CURRENT
        .scope(progress.clone(), async {
            lesson("A lesson");
            new_pass();
            phase(Phase::Evidence, "Checking claims");
            begin_checks(3);
            let first = model_call(Term::Reading);
            let second = model_call(Term::Reading);
            received("Answer");
            drop(first);
            checked(false);
            stage("Checked 1 of 3 claims");
            assert!(
                progress.snapshot().2.model_running,
                "One request is still active"
            );
            assert_eq!(progress.snapshot().2.checks_completed, 1);
            drop(second);
            assert!(!progress.snapshot().2.model_running);
            checkpoint_saved();
            phase(Phase::Research, "Finding missing evidence");
            phase(Phase::Evidence, "Rechecking evidence");
            begin_checks(3);
            checked(true);
            let activity = progress.snapshot().2;
            // A recheck after research is a step of the same pass (§6.3).
            assert_eq!(activity.verification_pass, 1);
            assert_eq!(activity.checks_reused, 1);
            assert!(activity.last_checkpoint_at.is_some());
            assert_eq!(
                activity
                    .recent_steps
                    .iter()
                    .map(|step| step.phase)
                    .collect::<Vec<_>>(),
                [Phase::Evidence, Phase::Research, Phase::Evidence]
            );
        })
        .await;
}

#[tokio::test]
async fn a_recheck_against_new_references_is_named() {
    let progress = Progress::default();
    CURRENT
        .scope(progress.clone(), async {
            new_references(Some(5));
            begin_checks(3);
            let activity = progress.snapshot().2;
            assert_eq!(activity.new_references, Some(5));
            new_references(None);
            assert_eq!(progress.snapshot().2.new_references, None);
        })
        .await;
}

#[tokio::test]
async fn retry_discards_partial_response_progress_and_keeps_the_stage() {
    let progress = Progress::default();
    CURRENT.scope(progress.clone(), async {
        stage("Extracting factual claims");
        model_started();
        received("partial response that cannot be parsed");
        model_retry(2);
        assert_eq!(progress.snapshot().1,"Extracting factual claims · Model request attempt 2 · Waiting for the model’s response");
        received("{}");
        assert_eq!(progress.snapshot().1,"Extracting factual claims · Model request attempt 2 · 2 response characters received");
    }).await;
}

#[tokio::test(start_paused = true)]
async fn lesson_model_can_run_past_old_deadlines_and_reports_stream_activity() -> Result<()> {
    use lattice_core::application::ports::{
        llm_port::{CompletionRequest, CompletionResponse},
        LLMPort,
    };
    struct SlowModel;
    #[async_trait::async_trait]
    impl LLMPort for SlowModel {
        async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
            assert!(request.no_time_limit);
            assert!(request.wall_clock_budget().is_none());
            tokio::time::sleep(std::time::Duration::from_secs(1800)).await;
            Ok(CompletionResponse { text: "supported\nReason: The source explicitly states this.\nSource quote: Rust is a language.".into(), finish_reason: "stop".into(), ..Default::default() })
        }
        async fn complete_with_progress(
            &self,
            request: &CompletionRequest,
            on_text: &(dyn Fn(String) -> Result<()> + Send + Sync),
        ) -> Result<CompletionResponse> {
            let response = self.complete(request).await?;
            on_text(response.text.clone())?;
            Ok(response)
        }

        fn model_name(&self) -> &str {
            "slow-lesson-fixture"
        }
        fn max_context_tokens(&self) -> usize {
            128_000
        }
        fn count_tokens(&self, text: &str) -> usize {
            text.len() / 4
        }
        async fn is_ready(&self) -> Result<bool> {
            Ok(true)
        }
    }
    let progress = Progress::default();
    CURRENT.scope(progress.clone(), async {
        stage("Writing lesson");
        let call = ContractCall::of::<Probe>(&PromptInput { system: "Teach".into(), prompt: "Lesson".into() })?;
        crate::generation::complete_json(&SlowModel, &call, "Teach", "Lesson".into(), serde_json::json!({"type":"object"}), 4000).await?;
        assert!(progress.snapshot().2.response_characters > 0);
        assert!(!progress.snapshot().2.model_running);
        use lattice_core::application::services::claim_verification::{CheckPolicy, ClaimChecker, ClaimEvidence, ClaimJudgment, ClaimVerdict};
        let checker = ClaimChecker::new(&SlowModel, lattice_core::application::ports::llm_port::SamplingOverride::deterministic(), 512, CheckPolicy::Strict);
        let judgment = checker.check("Rust is a language.", &ClaimEvidence { text: "Rust is a language.".into(), quote: None }).await;
        assert!(matches!(judgment, ClaimJudgment::Judged(outcome) if outcome.verdict == ClaimVerdict::Supported));
        Ok(())
    }).await
}

#[tokio::test]
async fn progress_is_scoped_and_new_calls_reset_response_counts() {
    assert!(!active());
    let progress = Progress::default();
    CURRENT
        .scope(progress.clone(), async {
            stage("Writing lesson");
            model_started();
            received("Hello 🦀");
            assert_eq!(
                (progress.snapshot().0, progress.snapshot().1),
                (0, "Writing lesson · 7 response characters received".into())
            );
            assert!(!tokio::spawn(async { active() }).await.unwrap());
            stage("Checking answer keys");
            model_started();
            assert_eq!(
                progress.snapshot().1,
                "Checking answer keys · Waiting for the model’s response"
            );
            completed(1);
            stage("Saving verified lesson");
            assert_eq!(
                (progress.snapshot().0, progress.snapshot().1),
                (1, "Saving verified lesson".into())
            );
        })
        .await;
    assert!(!active());
}

#[tokio::test]
async fn only_the_calls_made_inside_the_checks_are_counted() {
    let progress = Progress::default();
    CURRENT
        .scope(progress.clone(), async {
            drop(model_call(Term::Reading));
            drop(request());
            assert_eq!(progress.snapshot().2.model_calls, 0);
            checks(async {
                drop(model_calls(3, Term::Reading));
                // The teaching review after a repair's patches is no
                // check of the plan's statements.
                outside_checks(async { drop(model_call(Term::Rewrite)) }).await;
                drop(request());
            })
            .await;
            assert_eq!(progress.snapshot().2.model_calls, 3);
            assert!(!progress.snapshot().2.model_running);
        })
        .await;
}

#[tokio::test]
async fn calls_are_counted_and_the_ceiling_appears_once_the_statements_are_sorted() {
    let progress = Progress::default();
    CURRENT
        .scope(progress.clone(), async {
            lesson("A lesson");
            checks(async { drop(model_call(Term::Sort)) }).await;
            let activity = progress.snapshot().2;
            assert_eq!(
                (activity.model_calls, activity.model_calls_ceiling),
                (1, None)
            );

            let policy = StandardPolicy::new();
            let alone = |class, at| Planned {
                class,
                at,
                sentence: None,
            };
            let statements = [
                alone(Class::Sourced, PlannedAt::Entry),
                alone(Class::Common, PlannedAt::Entry),
                alone(Class::Common, PlannedAt::Entry),
            ];
            assert_eq!(progress.snapshot().2.statements, None);
            calls_planned(&policy, &statements, 1);
            // The plan is what the statements were sorted into.
            assert_eq!(
                progress.snapshot().2.statements,
                Some(LearningStatementPlan {
                    sourced: 1,
                    reasoning: 0,
                    common: 2
                })
            );
            // The sorting call made (1), the longest path of each
            // statement from its entry (19 for the Sourced one, 19 for
            // each Common one) and the flow repair of the section (1).
            assert_eq!(progress.snapshot().2.model_calls_ceiling, Some(59));
            calls_planned(&policy, &[], 0);
            assert_eq!(progress.snapshot().2.model_calls_ceiling, Some(59));

            // The Sourced statement is resolved: its path is over.
            statements_moved(
                &policy,
                &[],
                &[alone(Class::Sourced, PlannedAt::Ended(None))],
                &[],
            );
            assert_eq!(progress.snapshot().2.model_calls_ceiling, Some(40));
            cuts_known(0);
            assert_eq!(progress.snapshot().2.model_calls_ceiling, Some(39));
            checks(async { drop(model_calls(3, Term::Reading)) }).await;
            assert_eq!(progress.snapshot().2.model_calls, 4);

            lesson("Another lesson");
            let activity = progress.snapshot().2;
            assert_eq!(
                (activity.model_calls, activity.model_calls_ceiling),
                (0, None)
            );
        })
        .await;
}

#[test]
fn a_ceiling_that_goes_up_is_counted_and_one_that_comes_down_is_not() {
    let policy = StandardPolicy::new();
    let sourced = |at| Planned {
        class: Class::Sourced,
        at,
        sentence: None,
    };
    // The sort made one call.
    let mut state = State {
        plan: Some(CallPlan::new(&policy, &[sourced(PlannedAt::Entry)], 1)),
        activity: LearningGenerationActivity {
            model_calls: 1,
            ..Default::default()
        },
        ..Default::default()
    };
    publish_ceiling(&mut state, Change::default());
    let first = state.activity.model_calls_ceiling.unwrap();
    assert_eq!(first, 1 + 19 + 1);
    assert_eq!(state.activity.model_calls_ceiling_rises, 0);

    // The plan's own events only lower it.
    if let Some(plan) = state.plan.as_mut() {
        plan.moved(&policy, &[sourced(PlannedAt::Ended(None))]);
    }
    publish_ceiling(&mut state, Change::default());
    assert_eq!(state.activity.model_calls_ceiling, Some(2));
    assert!(state.activity.model_calls_ceiling.unwrap() <= first);
    assert_eq!(state.activity.model_calls_ceiling_rises, 0);

    // A ceiling shown lower and then published higher is a rise.
    state.activity.model_calls_ceiling = Some(1);
    publish_ceiling(&mut state, Change::default());
    assert_eq!(state.activity.model_calls_ceiling_rises, 1);
}

#[tokio::test]
async fn a_flow_repair_with_its_call_leaves_the_ceiling_where_it_was_and_one_without_brings_it_down(
) {
    let progress = Progress::default();
    CURRENT
        .scope(progress.clone(), async {
            lesson("A lesson");
            checks(async { drop(model_call(Term::Sort)) }).await;
            let policy = StandardPolicy::new();
            let statement = |at| Planned {
                class: Class::Sourced,
                at,
                sentence: None,
            };
            let ceiling = || progress.snapshot().2.model_calls_ceiling;
            calls_planned(&policy, &[statement(PlannedAt::Entry)], 2);
            statements_moved(&policy, &[], &[statement(PlannedAt::Ended(None))], &[]);
            cuts_known(2);
            // The sorting call made (1) and the flow repair of each of the
            // two sections that took a cut.
            assert_eq!(ceiling(), Some(3));

            // The first repair makes its call and marks its section mended.
            checks(async { drop(model_call(Term::FlowRepair)) }).await;
            flow_repaired();
            assert_eq!(ceiling(), Some(3));
            // The pass after the repairs does not raise it.
            statements_moved(&policy, &[], &[statement(PlannedAt::Ended(None))], &[]);
            assert_eq!(ceiling(), Some(3));

            // The second finds no gap to mend: it makes no call, and the
            // call held for it is not needed.
            flow_repaired();
            assert_eq!(ceiling(), Some(2));
            assert_eq!(progress.snapshot().2.model_calls, 2);
            assert_eq!(progress.snapshot().2.model_calls_ceiling_rises, 0);
        })
        .await;
}

#[tokio::test]
async fn a_statement_the_plan_takes_in_is_followed_and_raises_the_ceiling_by_its_sort_and_share() {
    let progress = Progress::default();
    CURRENT
        .scope(progress.clone(), async {
            let policy = StandardPolicy::new();
            let planned = |class, at| Planned {
                class,
                at,
                sentence: None,
            };
            let done = planned(Class::Sourced, PlannedAt::Ended(None));
            calls_planned(&policy, &[done], 0);
            assert_eq!(plan_claims(), Some(Vec::new()));
            let first = ClaimId("first".into());
            statements_moved(&policy, std::slice::from_ref(&first), &[done], &[]);
            assert_eq!(progress.snapshot().2.model_calls_ceiling, Some(0));

            // A later sort sorts a statement new to the plan, which joins it.
            checks(async { drop(model_call(Term::Sort)) }).await;
            let joined = ClaimId("joined".into());
            let joining = Joining {
                planned: planned(Class::Common, PlannedAt::Entry),
                beside: Vec::new(),
            };
            statements_moved(
                &policy,
                &[first.clone(), joined.clone()],
                &[done],
                &[joining],
            );

            // The sorting call made and the newcomer's longest path from
            // its entry: a rise, counted.
            let activity = progress.snapshot().2;
            assert_eq!(activity.model_calls_ceiling, Some(1 + 19));
            assert_eq!(activity.model_calls_ceiling_rises, 1);
            assert_eq!(plan_claims(), Some(vec![first, joined]));
        })
        .await;
}

#[test]
fn a_restarted_job_counts_its_calls_again_and_keeps_the_rest() {
    let saved = LearningGenerationActivity {
        model_calls: 40,
        model_calls_ceiling: Some(300),
        model_calls_ceiling_rises: 2,
        checks_total: 10,
        phase_started_at: 5,
        ..Default::default()
    };

    let activity = resumed(Some(serde_json::to_value(&saved).unwrap()));

    assert_eq!(
        (
            activity.model_calls,
            activity.model_calls_ceiling,
            activity.model_calls_ceiling_rises,
            activity.phase_started_at
        ),
        (0, None, 0, 0)
    );
    assert_eq!(activity.checks_total, 10);
}

#[tokio::test]
async fn a_lessons_progress_keeps_the_furthest_phase_it_reached() {
    let progress = Progress::default();
    CURRENT
        .scope(progress.clone(), async {
            lesson("A lesson");
            phase(Phase::Review, "Reviewing");
            phase(Phase::Evidence, "Checking");
            phase(Phase::Repair, "Repairing");
            phase(Phase::Review, "Reviewing the repaired draft");
            let activity = progress.snapshot().2;
            assert_eq!(
                (activity.phase, activity.reached),
                (Phase::Review, Phase::Repair)
            );
            phase(Phase::Evidence, "Checking again");
            assert_eq!(progress.snapshot().2.reached, Phase::Repair);
            lesson("A lesson");
            assert_eq!(progress.snapshot().2.reached, Phase::References);
            phase(Phase::Repair, "Repairing");
            lesson("Another lesson");
            assert_eq!(progress.snapshot().2.reached, Phase::References);
        })
        .await;
}

#[test]
fn a_restarted_job_reaches_its_phases_again() {
    let saved = LearningGenerationActivity {
        phase: Phase::Repair,
        reached: Phase::Repair,
        ..Default::default()
    };

    let activity = resumed(Some(serde_json::to_value(&saved).unwrap()));

    assert_eq!(activity.reached, Phase::References);
}

fn waiting(progress: &Progress) -> bool {
    progress.snapshot().2.waiting_for_model
}

#[tokio::test]
async fn a_lesson_waits_for_the_model_only_while_none_of_its_requests_holds_a_slot() {
    let progress = Progress::default();
    scoped(&progress, async {
        let observer = current_observer().expect("a lesson job's scope has an observer");
        observer.queued();
        assert!(
            waiting(&progress),
            "a request is queued and none holds a slot, so the lesson is waiting for the model"
        );
        observer.holding();
        assert!(
            !waiting(&progress),
            "a request holds a slot, so the lesson is working and not waiting"
        );
        observer.released();
        assert!(
            waiting(&progress),
            "the slot was given back and the queued request remains, so the lesson waits again"
        );
        observer.left_queue(QueueExit::Granted);
        observer.holding();
        assert!(
            !waiting(&progress),
            "the queued request was granted and holds a slot"
        );
        observer.released();
        assert!(
            !waiting(&progress),
            "nothing is queued, so a lesson that gave its last slot back is not waiting"
        );
    })
    .await;
}

#[tokio::test]
async fn a_queued_request_that_is_abandoned_stops_the_lesson_waiting() {
    let progress = Progress::default();
    scoped(&progress, async {
        let observer = current_observer().expect("a lesson job's scope has an observer");
        observer.queued();
        assert!(waiting(&progress), "its only request is queued");
        observer.left_queue(QueueExit::Abandoned);
        assert!(
            !waiting(&progress),
            "a request cancelled while queued is no longer queued"
        );
    })
    .await;
}

#[tokio::test]
async fn a_lesson_with_nothing_queued_is_not_waiting() {
    let progress = Progress::default();
    scoped(&progress, async {
        let observer = current_observer().expect("a lesson job's scope has an observer");
        assert!(
            !waiting(&progress),
            "a lesson that has made no request is not waiting"
        );
        observer.holding();
        assert!(
            !waiting(&progress),
            "a request that holds a slot and nothing queued is not a lesson waiting for the model"
        );
    })
    .await;
}

#[tokio::test]
async fn a_call_counted_twice_still_shows_the_lesson_waiting() {
    let progress = Progress::default();
    scoped(&progress, async {
        let observer = current_observer().expect("a lesson job's scope has an observer");
        let _sort = model_call(Term::Sort);
        let _send = request();
        observer.queued();
        assert!(
            waiting(&progress),
            "one request registered by two layers and queued at the scheduler holds no slot, so the lesson is waiting"
        );
    })
    .await;
}

#[tokio::test]
async fn a_request_made_inside_the_lesson_scope_reaches_its_own_progress_and_no_other() {
    let ours = Progress::default();
    let other = Progress::default();
    scoped(&ours, async {
        current_observer()
            .expect("the lesson scope has an observer")
            .queued();
        assert!(
            waiting(&ours),
            "the request reached its own lesson's progress"
        );
        assert!(!waiting(&other), "another lesson's progress is untouched");
    })
    .await;
    assert!(
        current_observer().is_none(),
        "outside a lesson job's scope there is no observer"
    );
}
