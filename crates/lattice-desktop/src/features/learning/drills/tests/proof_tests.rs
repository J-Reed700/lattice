//! Proof is a Strategy: the table of strategies cell by cell, and each strategy on its own.

use super::support::{
    files, judge_answer, last, literals_edit, literals_mutation, messages, operator_edit,
    operator_mutation, FakeRunner, ScriptedModel, KILLING_TEST, LITERALS, LITERALS_AFTER,
    LITERALS_BEFORE, OPERATOR, OPERATOR_AFTER, OPERATOR_BEFORE, SUITE,
};
use crate::features::learning::drills::build::{Mutation, Trail, Verification};
use crate::features::learning::drills::proof::{
    choose, is_behavioural, strategy, ProofCase, ProofVerdict, StrategyKind, Tools,
};
use crate::features::learning::drills::runner::{CodeRunner, RunOutcome};
use crate::features::learning::practice::practice_tests::text_length_rules;
use lattice_core::domain::learning::drills::{
    apply, pattern, DrillFile, Edit, Family, Proof, Severity,
};
use lattice_core::shared::error::Result;
use serde_json::json;
use tokio_util::sync::CancellationToken;

/// A test that passes on the clean limiter and fails with the operator changed, which the first one did not.
const BETTER: &str = "from src.limiter import RateLimiter\n\nlimiter = RateLimiter(2, 10, lambda: 0)\nassert limiter.allow(\"b\")\nassert limiter.allow(\"b\")\nassert not limiter.allow(\"b\")\n";

/// The project, and both mutations applied one at a time.
struct World {
    clean: Vec<DrillFile>,
    operator_mutated: Vec<DrillFile>,
    literals_mutated: Vec<DrillFile>,
    operator: Mutation,
    literals: Mutation,
    operator_others: Vec<Edit>,
    literals_others: Vec<Edit>,
}

impl World {
    fn new() -> Self {
        let clean = files();
        let (operator_mutated, _) = apply(&clean, &[operator_edit()]).unwrap();
        let (literals_mutated, _) = apply(&clean, &[literals_edit()]).unwrap();
        Self {
            clean,
            operator_mutated,
            literals_mutated,
            operator: operator_mutation(),
            literals: literals_mutation(),
            operator_others: vec![literals_edit()],
            literals_others: vec![operator_edit()],
        }
    }

    fn operator_case(&self) -> ProofCase<'_> {
        ProofCase {
            language: "python",
            clean: &self.clean,
            mutation: &self.operator,
            mutated: &self.operator_mutated,
            pattern: pattern(OPERATOR).unwrap(),
            suite: Some(SUITE),
            others: &self.operator_others,
        }
    }

    fn literals_case(&self) -> ProofCase<'_> {
        ProofCase {
            language: "python",
            clean: &self.clean,
            mutation: &self.literals,
            mutated: &self.literals_mutated,
            pattern: pattern(LITERALS).unwrap(),
            suite: Some(SUITE),
            others: &self.literals_others,
        }
    }
}

fn tools<'a>(
    model: &'a ScriptedModel,
    runner: Option<&'a FakeRunner>,
    cancel: &'a CancellationToken,
) -> Tools<'a> {
    Tools {
        llm: model,
        runner: runner.map(|runner| runner as &dyn CodeRunner),
        cancel,
    }
}

fn proven(verdict: ProofVerdict) -> (Proof, Mutation, Trail) {
    match verdict {
        ProofVerdict::Proven {
            proof,
            mutation,
            trail,
        } => (proof, mutation, trail),
        ProofVerdict::Unproven { why, .. } => panic!("unproven: {why}"),
    }
}

fn unproven(verdict: ProofVerdict) -> (String, Trail) {
    match verdict {
        ProofVerdict::Unproven { why, trail } => (why, trail),
        ProofVerdict::Proven { proof, .. } => panic!("proven: {proof:?}"),
    }
}

fn purposes(trail: &Trail) -> Vec<&str> {
    trail
        .calls
        .iter()
        .map(|call| call.purpose.as_str())
        .collect()
}

#[test]
fn the_strategy_is_chosen_by_family_and_by_whether_the_language_has_a_runner() {
    let killing = Verification::KillingTest("a test".into());
    let unobservable = Verification::Unobservable("A race cannot be seen.".into());
    let not_asked = Verification::NotAsked;
    let verifications = [&killing, &unobservable, &not_asked];
    for family in Family::ALL {
        let behavioural = matches!(
            family,
            Family::Correctness
                | Family::Errors
                | Family::Concurrency
                | Family::Resources
                | Family::Security
                | Family::Performance
        );
        assert_eq!(is_behavioural(family), behavioural, "{family:?}");
        for verification in verifications {
            assert_eq!(
                choose(family, false, verification),
                StrategyKind::Judged,
                "{family:?} without a runner"
            );
        }
        if behavioural {
            assert_eq!(choose(family, true, &killing), StrategyKind::Killing);
            assert_eq!(choose(family, true, &unobservable), StrategyKind::Judged);
            assert_eq!(choose(family, true, &not_asked), StrategyKind::Judged);
        } else if matches!(family, Family::Design | Family::Readability) {
            for verification in verifications {
                assert_eq!(
                    choose(family, true, verification),
                    StrategyKind::BehaviourKeptThenJudged,
                    "{family:?}"
                );
            }
        } else {
            for verification in verifications {
                assert_eq!(
                    choose(family, true, verification),
                    StrategyKind::WeakenedTestsThenJudged,
                    "{family:?}"
                );
            }
        }
    }
}

#[tokio::test]
async fn a_test_that_passes_on_the_clean_code_and_fails_with_the_mutation_kills_it() -> Result<()> {
    let world = World::new();
    let model = ScriptedModel::new([]);
    let runner = FakeRunner::killing_the_operator();
    let cancel = CancellationToken::new();
    let verdict = strategy(StrategyKind::Killing, tools(&model, Some(&runner), &cancel))
        .prove(&world.operator_case())
        .await?;
    let (proof, mutation, trail) = proven(verdict);
    assert_eq!(
        proof,
        Proof::Killed {
            test: KILLING_TEST.into()
        }
    );
    assert_eq!(mutation, world.operator);
    assert_eq!(
        trail
            .runs
            .iter()
            .map(|run| run.what.as_str())
            .collect::<Vec<_>>(),
        [
            "The killing test on the clean code",
            "The killing test with the mutation"
        ]
    );
    assert_eq!(trail.runs[0].outcome, RunOutcome::Passed);
    assert!(
        matches!(&trail.runs[1].outcome, RunOutcome::Failed { output } if output.contains("second call"))
    );
    let runs = runner.runs();
    assert_eq!(runs.len(), 2);
    assert_eq!(runs[0].0, world.clean);
    assert_eq!(runs[1].0, world.operator_mutated);
    assert!(runs.iter().all(|run| run.1 == KILLING_TEST));
    assert!(model.requests().is_empty(), "a killing test asks nothing");
    Ok(())
}

/// A runner on which `KILLING_TEST` passes everywhere and `BETTER` separates the clean code from the mutation.
fn only_the_better_test_kills() -> FakeRunner {
    FakeRunner::new(|files, test| {
        let mutated = files
            .iter()
            .any(|file| file.text.contains(OPERATOR_AFTER.trim()));
        if test == BETTER && mutated {
            RunOutcome::Failed {
                output: "AssertionError".into(),
            }
        } else {
            RunOutcome::Passed
        }
    })
}

#[tokio::test]
async fn a_test_that_passes_on_both_is_sent_back_once_with_both_runs_for_a_new_test() -> Result<()>
{
    let world = World::new();
    let model = ScriptedModel::new([json!({"killingTest": BETTER}).to_string()]);
    let runner = only_the_better_test_kills();
    let cancel = CancellationToken::new();
    let verdict = strategy(StrategyKind::Killing, tools(&model, Some(&runner), &cancel))
        .prove(&world.operator_case())
        .await?;
    let (proof, _, trail) = proven(verdict);
    assert_eq!(
        proof,
        Proof::Killed {
            test: BETTER.into()
        }
    );
    assert_eq!(purposes(&trail), ["killing test (asked again)"]);
    assert_eq!(trail.runs.len(), 4, "both versions, twice");
    let requests = model.requests();
    assert_eq!(requests.len(), 1);
    let sent = messages(&requests[0]);
    assert!(sent[0].1.contains("You write a test that shows a defect"));
    assert!(sent[0].1.contains("Fake contract"));
    assert_eq!(sent[1].0, "user");
    assert!(sent[1].1.contains("Wrong operator or comparison"));
    assert!(sent[1].1.contains(OPERATOR_AFTER.trim()));
    assert_eq!(
        sent[2],
        (
            "assistant".to_owned(),
            json!({"killingTest": KILLING_TEST}).to_string()
        )
    );
    let reask = last(&requests[0]);
    assert!(
        reask.starts_with("That answer could not be used:"),
        "{reask}"
    );
    assert!(
        reask.contains("Run on the code without the change, it passed. Run on the code with the change, it passed."),
        "{reask}"
    );
    assert!(text_length_rules(requests[0].json_schema.as_ref().unwrap()).is_empty());
    Ok(())
}

#[tokio::test]
async fn a_test_that_fails_on_the_clean_code_is_sent_back_with_its_output() -> Result<()> {
    let world = World::new();
    let model = ScriptedModel::new([json!({"killingTest": BETTER}).to_string()]);
    let runner = FakeRunner::new(|files, test| {
        let mutated = files
            .iter()
            .any(|file| file.text.contains(OPERATOR_AFTER.trim()));
        match (test == BETTER, mutated) {
            (true, true) => RunOutcome::Failed {
                output: "AssertionError".into(),
            },
            (true, false) => RunOutcome::Passed,
            (false, _) => RunOutcome::Failed {
                output: "NameError: name 'limiter' is not defined".into(),
            },
        }
    });
    let cancel = CancellationToken::new();
    let verdict = strategy(StrategyKind::Killing, tools(&model, Some(&runner), &cancel))
        .prove(&world.operator_case())
        .await?;
    let (proof, _, _) = proven(verdict);
    assert_eq!(
        proof,
        Proof::Killed {
            test: BETTER.into()
        }
    );
    let reask = last(&model.requests()[0]);
    assert!(
        reask.contains("NameError: name 'limiter' is not defined"),
        "{reask}"
    );
    Ok(())
}

#[tokio::test]
async fn a_second_miss_leaves_the_mutation_unproven() -> Result<()> {
    let world = World::new();
    let model = ScriptedModel::new([json!({"killingTest": BETTER}).to_string()]);
    let runner = FakeRunner::passing();
    let cancel = CancellationToken::new();
    let verdict = strategy(StrategyKind::Killing, tools(&model, Some(&runner), &cancel))
        .prove(&world.operator_case())
        .await?;
    let (why, trail) = unproven(verdict);
    assert!(why.contains("even written again"), "{why}");
    assert_eq!(trail.runs.len(), 4);
    assert_eq!(model.requests().len(), 1, "sent back once");
    Ok(())
}

/// The runs that say nothing about the code, and what a verdict then says: a timeout, a cancellation and a runtime
/// that is not there prove nothing.
fn silent_runs() -> Vec<(RunOutcome, &'static str)> {
    vec![
        (
            RunOutcome::TimedOut {
                output: "partial".into(),
            },
            "time limit",
        ),
        (RunOutcome::Cancelled, "cancelled"),
        (
            RunOutcome::Unavailable {
                why: "The runtime is gone.".into(),
            },
            "The runtime is gone.",
        ),
    ]
}

#[tokio::test]
async fn a_run_that_does_not_end_proves_nothing_and_nothing_is_asked() -> Result<()> {
    let world = World::new();
    for (silent, fragment) in silent_runs() {
        for silent_on in ["clean", "mutated"] {
            let model = ScriptedModel::new([]);
            let silent = silent.clone();
            let runner = FakeRunner::new(move |files, _| {
                let mutated = files
                    .iter()
                    .any(|file| file.text.contains(OPERATOR_AFTER.trim()));
                if mutated == (silent_on == "mutated") {
                    silent.clone()
                } else {
                    RunOutcome::Passed
                }
            });
            let cancel = CancellationToken::new();
            let verdict = strategy(StrategyKind::Killing, tools(&model, Some(&runner), &cancel))
                .prove(&world.operator_case())
                .await?;
            let (why, _) = unproven(verdict);
            assert!(why.contains("proves nothing"), "{why}");
            assert!(why.contains(fragment), "{fragment} in {why}");
            assert!(model.requests().is_empty(), "{fragment} on {silent_on}");
            assert_eq!(runner.runs().len(), 2);
        }
    }
    Ok(())
}

#[tokio::test]
async fn a_new_test_that_cannot_be_used_leaves_the_mutation_unproven_and_does_not_fail_the_build(
) -> Result<()> {
    let world = World::new();
    let empty = || json!({"killingTest": "  "}).to_string();
    let model = ScriptedModel::new([empty(), empty()]);
    let runner = FakeRunner::passing();
    let cancel = CancellationToken::new();
    let verdict = strategy(StrategyKind::Killing, tools(&model, Some(&runner), &cancel))
        .prove(&world.operator_case())
        .await?;
    let (why, trail) = unproven(verdict);
    assert!(why.contains("no new test could be used"), "{why}");
    assert_eq!(purposes(&trail), ["killing test (asked again)"]);
    assert_eq!(model.requests().len(), 2, "asked once more, never twice");
    Ok(())
}

#[tokio::test]
async fn a_mutation_without_a_test_or_a_runner_cannot_be_killed() -> Result<()> {
    let world = World::new();
    let model = ScriptedModel::new([]);
    let runner = FakeRunner::passing();
    let cancel = CancellationToken::new();
    let observable = Mutation {
        verification: Verification::Unobservable("A race cannot be seen.".into()),
        ..operator_mutation()
    };
    let mut untested = world.operator_case();
    untested.mutation = &observable;
    let (why, _) = unproven(
        strategy(StrategyKind::Killing, tools(&model, Some(&runner), &cancel))
            .prove(&untested)
            .await?,
    );
    assert!(why.contains("no killing test"), "{why}");
    let (why, _) = unproven(
        strategy(StrategyKind::Killing, tools(&model, None, &cancel))
            .prove(&world.operator_case())
            .await?,
    );
    assert!(why.contains("no runner"), "{why}");
    assert!(runner.runs().is_empty());
    Ok(())
}

#[tokio::test]
async fn the_judge_confirms_a_mutation_that_introduces_exactly_the_problem() -> Result<()> {
    let world = World::new();
    let model = ScriptedModel::new([judge_answer(true)]);
    let cancel = CancellationToken::new();
    let verdict = strategy(StrategyKind::Judged, tools(&model, None, &cancel))
        .prove(&world.operator_case())
        .await?;
    let (proof, mutation, trail) = proven(verdict);
    assert!(
        matches!(&proof, Proof::Judged { reason } if reason.starts_with("The edit changes the comparison")),
        "{proof:?}"
    );
    assert_eq!(mutation, world.operator);
    assert_eq!(purposes(&trail), ["judge"]);
    let request = &model.requests()[0];
    let sent = messages(request);
    assert!(sent[0].1.contains("You judge a code review exercise"));
    assert!(sent[0].1.contains("Give your reasons first"));
    let user = &sent[1].1;
    assert!(user.contains("Wrong operator or comparison (correctness.wrong_operator)"));
    assert!(user.contains("Claimed consequence: A caller gets one call more than the limit."));
    assert!(user.contains("The original code:"));
    assert!(user.contains("if len(recent) >= self.limit:"));
    assert!(user.contains("if len(recent) > self.limit:"));
    let schema = request.json_schema.clone().unwrap();
    assert_eq!(
        schema["required"],
        json!(["reason", "editIntroducesIt", "cleanLacksIt"]),
        "the reasons come before the verdict"
    );
    Ok(())
}

#[tokio::test]
async fn a_judge_that_does_not_confirm_both_halves_leaves_the_mutation_unproven() -> Result<()> {
    let world = World::new();
    let half = json!({
        "reason": "The edit introduces it, but the original code already had it.",
        "editIntroducesIt": true,
        "cleanLacksIt": false
    })
    .to_string();
    for answer in [judge_answer(false), half] {
        let model = ScriptedModel::new([answer]);
        let cancel = CancellationToken::new();
        let verdict = strategy(StrategyKind::Judged, tools(&model, None, &cancel))
            .prove(&world.operator_case())
            .await?;
        let (why, _) = unproven(verdict);
        assert!(why.contains("original code already"), "{why}");
    }
    Ok(())
}

#[tokio::test]
async fn a_judge_whose_answer_cannot_be_used_leaves_the_mutation_unproven() -> Result<()> {
    let world = World::new();
    let blank =
        || json!({"reason": " ", "editIntroducesIt": true, "cleanLacksIt": true}).to_string();
    let model = ScriptedModel::new([blank(), blank()]);
    let cancel = CancellationToken::new();
    let verdict = strategy(StrategyKind::Judged, tools(&model, None, &cancel))
        .prove(&world.operator_case())
        .await?;
    let (why, trail) = unproven(verdict);
    assert!(
        why.contains("The judge's answer could not be used"),
        "{why}"
    );
    assert_eq!(purposes(&trail), ["judge"]);
    assert_eq!(model.requests().len(), 2);
    Ok(())
}

#[tokio::test]
async fn a_smell_that_keeps_the_suite_passing_goes_to_the_judge() -> Result<()> {
    let world = World::new();
    let model = ScriptedModel::new([judge_answer(true)]);
    let runner = FakeRunner::passing();
    let cancel = CancellationToken::new();
    let verdict = strategy(
        StrategyKind::BehaviourKeptThenJudged,
        tools(&model, Some(&runner), &cancel),
    )
    .prove(&world.literals_case())
    .await?;
    let (proof, mutation, trail) = proven(verdict);
    assert!(matches!(proof, Proof::Judged { .. }));
    assert_eq!(mutation, world.literals);
    assert_eq!(
        runner.runs(),
        vec![(world.literals_mutated.clone(), SUITE.to_owned())]
    );
    assert_eq!(trail.runs[0].what, "The suite with the mutation");
    assert_eq!(purposes(&trail), ["judge"]);
    Ok(())
}

/// A runner on which the suite fails when the literals are inlined, as the first literals mutation does.
fn failing_when_inlined() -> FakeRunner {
    FakeRunner::new(|files, _| {
        if files
            .iter()
            .any(|file| file.text.contains(LITERALS_AFTER.trim()))
        {
            RunOutcome::Failed {
                output: "AssertionError: the defaults changed".into(),
            }
        } else {
            RunOutcome::Passed
        }
    })
}

fn replacement(path: &str, before: &str, after: &str) -> String {
    json!({
        "path": path, "before": before, "after": after, "severity": "minor",
        "consequence": "A local variable hides which constant is used.",
        "explanation": "The constant is copied to a local for no reason. Use the constant directly."
    })
    .to_string()
}

const RESHAPED: &str =
    "    limit = DEFAULT_LIMIT\n    return RateLimiter(limit, DEFAULT_WINDOW, clock)\n";

#[tokio::test]
async fn a_smell_that_fails_the_suite_is_sent_back_once_for_a_new_edit_that_keeps_every_name(
) -> Result<()> {
    let world = World::new();
    let model = ScriptedModel::new([
        replacement("src/config.py", LITERALS_BEFORE, RESHAPED),
        judge_answer(true),
    ]);
    let runner = failing_when_inlined();
    let cancel = CancellationToken::new();
    let verdict = strategy(
        StrategyKind::BehaviourKeptThenJudged,
        tools(&model, Some(&runner), &cancel),
    )
    .prove(&world.literals_case())
    .await?;
    let (proof, mutation, trail) = proven(verdict);
    assert!(matches!(proof, Proof::Judged { .. }));
    assert_eq!(mutation.pattern, LITERALS);
    assert_eq!(mutation.edit.before, LITERALS_BEFORE);
    assert_eq!(mutation.edit.after, RESHAPED, "the new edit stands");
    assert_eq!(mutation.severity, Severity::Minor);
    assert_eq!(mutation.verification, Verification::NotAsked);
    assert_eq!(purposes(&trail), ["new edit (asked again)", "judge"]);
    assert_eq!(trail.runs.len(), 2);
    assert!(matches!(trail.runs[0].outcome, RunOutcome::Failed { .. }));
    assert_eq!(trail.runs[1].outcome, RunOutcome::Passed);
    let requests = model.requests();
    assert_eq!(model.purposes(), ["new edit", "judge"]);
    let sent = messages(&requests[0]);
    assert!(sent[1].1.contains("<suite>"));
    assert!(
        sent[1].1.contains("In src/limiter.py, replacing:"),
        "the other edits stay"
    );
    assert!(text_length_rules(requests[0].json_schema.as_ref().unwrap()).is_empty());
    let reask = last(&requests[0]);
    assert!(
        reask.starts_with("That answer could not be used:"),
        "{reask}"
    );
    assert!(
        reask.contains("AssertionError: the defaults changed"),
        "{reask}"
    );
    assert!(
        reask.contains("Keep every name and signature the suite uses"),
        "{reask}"
    );
    assert!(
        messages(&requests[1])[1]
            .1
            .contains("limit = DEFAULT_LIMIT"),
        "the judge sees the new edit"
    );
    Ok(())
}

#[tokio::test]
async fn a_smell_that_fails_the_suite_again_is_unproven_without_asking_the_judge() -> Result<()> {
    let world = World::new();
    let model = ScriptedModel::new([replacement("src/config.py", LITERALS_BEFORE, RESHAPED)]);
    let runner = FakeRunner::new(|_, _| RunOutcome::Failed {
        output: "boom".into(),
    });
    let cancel = CancellationToken::new();
    let verdict = strategy(
        StrategyKind::BehaviourKeptThenJudged,
        tools(&model, Some(&runner), &cancel),
    )
    .prove(&world.literals_case())
    .await?;
    let (why, _) = unproven(verdict);
    assert!(why.contains("even after a new edit"), "{why}");
    assert_eq!(model.purposes(), ["new edit"]);
    Ok(())
}

#[tokio::test]
async fn a_new_edit_must_apply_beside_the_others() -> Result<()> {
    let world = World::new();
    let overlapping = replacement(
        "src/limiter.py",
        "        if len(recent) >= self.limit:\n",
        "        if len(recent) >= self.limit and recent:\n",
    );
    let missing = replacement("src/config.py", "    return nothing\n", RESHAPED);
    let model = ScriptedModel::new([
        missing,
        replacement("src/config.py", LITERALS_BEFORE, RESHAPED),
        judge_answer(true),
    ]);
    let runner = failing_when_inlined();
    let cancel = CancellationToken::new();
    let verdict = strategy(
        StrategyKind::BehaviourKeptThenJudged,
        tools(&model, Some(&runner), &cancel),
    )
    .prove(&world.literals_case())
    .await?;
    let (_, mutation, _) = proven(verdict);
    assert_eq!(mutation.edit.after, RESHAPED);
    let requests = model.requests();
    assert_eq!(model.purposes(), ["new edit", "new edit", "judge"]);
    let refusal = last(&requests[1]);
    assert!(
        refusal.contains("The new change: `before` is not in src/config.py"),
        "{refusal}"
    );

    let model = ScriptedModel::new([overlapping.clone(), overlapping]);
    let runner = failing_when_inlined();
    let verdict = strategy(
        StrategyKind::BehaviourKeptThenJudged,
        tools(&model, Some(&runner), &cancel),
    )
    .prove(&world.literals_case())
    .await?;
    let (why, _) = unproven(verdict);
    assert!(why.contains("no new edit could be used"), "{why}");
    assert!(
        last(&model.requests()[1]).contains("Another change and The new change overlap"),
        "{}",
        last(&model.requests()[1])
    );
    Ok(())
}

#[tokio::test]
async fn a_suite_that_does_not_end_leaves_a_smell_unproven_and_asks_nothing() -> Result<()> {
    let world = World::new();
    for (silent, fragment) in silent_runs() {
        let model = ScriptedModel::new([]);
        let runner = FakeRunner::new(move |_, _| silent.clone());
        let cancel = CancellationToken::new();
        let verdict = strategy(
            StrategyKind::BehaviourKeptThenJudged,
            tools(&model, Some(&runner), &cancel),
        )
        .prove(&world.literals_case())
        .await?;
        let (why, _) = unproven(verdict);
        assert!(
            why.contains("The suite could not be run to an end"),
            "{why}"
        );
        assert!(why.contains(fragment), "{fragment} in {why}");
        assert!(model.requests().is_empty());
        assert_eq!(runner.runs().len(), 1, "a timeout is not sent back");
    }
    Ok(())
}

/// A test file of the project that a mutation weakens.
fn testing_case_files() -> (Vec<DrillFile>, Vec<DrillFile>, Mutation) {
    let mut clean = files();
    clean.push(DrillFile {
        path: "tests/test_limiter.py".into(),
        text: "from src.limiter import RateLimiter\n\n\ndef test_it():\n    assert RateLimiter(1, 1, lambda: 0).allow(\"a\")\n".into(),
    });
    let edit = Edit {
        path: "tests/test_limiter.py".into(),
        before: "    assert RateLimiter(1, 1, lambda: 0).allow(\"a\")\n".into(),
        after: "    RateLimiter(1, 1, lambda: 0).allow(\"a\")\n".into(),
    };
    let (weakened, _) = apply(&clean, std::slice::from_ref(&edit)).unwrap();
    let mutation = Mutation {
        pattern: "testing.assertion_free_test".into(),
        edit,
        severity: Severity::Minor,
        consequence: "The test passes whatever the limiter does.".into(),
        explanation: "The test checks nothing. Assert on the result.".into(),
        verification: Verification::NotAsked,
    };
    (clean, weakened, mutation)
}

#[tokio::test]
async fn a_weakened_test_that_still_passes_on_the_clean_code_goes_to_the_judge() -> Result<()> {
    let (clean, weakened, mutation) = testing_case_files();
    let case = ProofCase {
        language: "python",
        clean: &clean,
        mutation: &mutation,
        mutated: &weakened,
        pattern: pattern("testing.assertion_free_test").unwrap(),
        suite: Some(SUITE),
        others: &[],
    };
    let model = ScriptedModel::new([judge_answer(true)]);
    let runner = FakeRunner::passing();
    let cancel = CancellationToken::new();
    let verdict = strategy(
        StrategyKind::WeakenedTestsThenJudged,
        tools(&model, Some(&runner), &cancel),
    )
    .prove(&case)
    .await?;
    let (proof, _, trail) = proven(verdict);
    assert!(matches!(proof, Proof::Judged { .. }));
    assert_eq!(
        runner.runs(),
        vec![(weakened.clone(), "run tests/test_limiter.py\n".to_owned())],
        "the weakened test runs as a program beside the clean code"
    );
    assert_eq!(trail.runs[0].what, "The weakened test on the clean code");
    assert_eq!(purposes(&trail), ["judge"]);
    Ok(())
}

#[tokio::test]
async fn a_test_that_fails_on_the_clean_code_is_broken_and_not_weak() -> Result<()> {
    let (clean, weakened, mutation) = testing_case_files();
    let case = ProofCase {
        language: "python",
        clean: &clean,
        mutation: &mutation,
        mutated: &weakened,
        pattern: pattern("testing.assertion_free_test").unwrap(),
        suite: Some(SUITE),
        others: &[],
    };
    let mut outcomes = vec![(
        RunOutcome::Failed {
            output: "ImportError".into(),
        },
        "broken and not weak: ImportError",
    )];
    outcomes.extend(silent_runs());
    for (outcome, fragment) in outcomes {
        let model = ScriptedModel::new([]);
        let runner = FakeRunner::new(move |_, _| outcome.clone());
        let cancel = CancellationToken::new();
        let verdict = strategy(
            StrategyKind::WeakenedTestsThenJudged,
            tools(&model, Some(&runner), &cancel),
        )
        .prove(&case)
        .await?;
        let (why, _) = unproven(verdict);
        assert!(why.contains(fragment), "{why}");
        assert!(model.requests().is_empty(), "nothing is judged");
    }
    Ok(())
}
