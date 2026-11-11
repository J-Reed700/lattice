//! The engine's observation of a program renders what the observation golden says, byte for byte.

#![allow(clippy::indexing_slicing, clippy::unwrap_used, clippy::expect_used)]

use crate::lab_runtime::LearningLabFile;
use crate::practice::coding::dto::{
    CaseViewDto, CodingRunScope, PracticeCheckDto, PracticeSessionDto,
};
use crate::practice::coding::golden_support::{assert_golden, rich_session, Golden};
use crate::practice::coding::observation::test_support::{problem, session};
use crate::practice::coding::problem::{
    CaseCompare, CaseResult as RunResult, CaseStatus as RunStatus, CaseVisibility as RunVisibility,
    Problem, ProblemCase,
};
use crate::practice::coding::proof::fakes::{Behaviour, FakeRunner};
use crate::practice::coding::session::{case_is_open, cut_case};
use crate::practice::kinds::program::diff::ProgramDiff;
use crate::practice::kinds::program::kind;
use crate::practice::kinds::program::shape::{
    brief_of, case_of, piece_of, result_of, run_detail_of,
};
use crate::practice::kinds::program::toolchain::FakeToolchains;
use crate::practice::practical_dto::{CaseCheckDetailDto, LearningPracticalRunStatus};
use crate::practice::session::kind::{Diff, Kind};
use crate::practice::session::observation::{
    CaseView, CheckView, ClockView, Coverage, HintView, LearnerCaseView, Observation,
};
use lattice_core::domain::learning::practice::work::CheckStatus;
use lattice_core::domain::learning::practice::work::{Form, Setting, Stage};
use serde_json::json;
use std::sync::Arc;

pub(super) use crate::practice::kinds::program::shape::{scope_of, verdict_of};

pub(super) fn program_kind() -> Kind {
    kind(Arc::new(FakeToolchains::new(FakeRunner::new(|_, _| {
        Behaviour::Fails
    }))))
}

fn check_status_of(status: LearningPracticalRunStatus) -> CheckStatus {
    match status {
        LearningPracticalRunStatus::Pending => CheckStatus::Pending,
        LearningPracticalRunStatus::Running => CheckStatus::Running,
        LearningPracticalRunStatus::Passed => CheckStatus::Passed,
        LearningPracticalRunStatus::Failed => CheckStatus::Failed,
        LearningPracticalRunStatus::TimedOut => CheckStatus::TimedOut,
        LearningPracticalRunStatus::Cancelled => CheckStatus::Cancelled,
        LearningPracticalRunStatus::Interrupted => CheckStatus::Interrupted,
        LearningPracticalRunStatus::RuntimeUnavailable => CheckStatus::Unavailable,
    }
}

/// The case a view shows, as far as the view says: what a cut left out stays empty.
fn problem_case_of_view(view: &CaseViewDto) -> ProblemCase {
    ProblemCase {
        id: view.case_id.clone(),
        category: view.category.clone(),
        args: view.args.clone().unwrap_or_default(),
        expected: view.expected.clone(),
        compare: CaseCompare::Exact,
        visibility: view.visibility,
        scale: view.scale,
        digest: None,
    }
}

fn case_view_of(view: &CaseViewDto, disclose: bool) -> CaseView {
    let result = RunResult {
        case_id: view.case_id.clone(),
        status: view.status,
        actual: view.actual.clone(),
        printed: view.printed.clone().unwrap_or_default(),
        error: view.error.clone(),
        elapsed_ms: view.elapsed_ms,
    };
    CaseView {
        case: case_of(&problem_case_of_view(view)).unwrap(),
        result: Some(result_of(&result).unwrap()),
        open: case_is_open(view.visibility, view.mine, disclose),
    }
}

/// The engine's observation of the session `observe` observes. `disclose` is what the session's views were cut
/// with.
pub(super) fn engine_observation(
    problem: &Problem,
    session: &PracticeSessionDto,
    seen: Option<&[LearningLabFile]>,
    disclose: bool,
) -> Observation {
    let brief = brief_of(&problem.spec).unwrap();
    let form = session.form;
    let stage = session.stage;
    let mut stages_reached = Vec::new();
    for reached in form.stages(true) {
        stages_reached.push(reached);
        if reached == stage {
            break;
        }
    }
    let piece = piece_of(&session.draft).unwrap();
    let changes = match seen {
        Some(seen) => ProgramDiff
            .changes(&brief, &piece_of(seen).unwrap(), &piece)
            .unwrap(),
        None => Vec::new(),
    };
    Observation {
        session_id: session.session_id.clone(),
        form,
        setting: session.setting,
        stage,
        stages_reached,
        brief,
        shown: session
            .problem
            .examples
            .iter()
            .map(|view| case_of(&problem_case_of_view(view)).unwrap())
            .collect(),
        clock: ClockView {
            active_ms: session.clock.active_ms,
            budget_seconds: session.clock.budget_seconds,
        },
        piece,
        has_baseline: seen.is_some(),
        changes,
        last_check: session.runs.last().map(|run| CheckView {
            id: run.run_id.clone(),
            scope: scope_of(run.scope),
            status: check_status_of(run.status),
            cases: run
                .cases
                .iter()
                .map(|view| case_view_of(view, disclose))
                .collect(),
            detail: run_detail_of(&run.stdout, &run.stderr).unwrap(),
        }),
        learner_cases: session
            .examples
            .iter()
            .map(|example| LearnerCaseView {
                ordinal: example.ordinal,
                body: json!({"args": example.args, "expected": example.expected}),
                verdict: verdict_of(example.verdict),
            })
            .collect(),
        coverage: Coverage {
            caught: session.coverage.caught,
            total: session.coverage.total,
            uncaught: session.coverage.uncaught.clone(),
        },
        hints: session
            .hints
            .iter()
            .map(|hint| HintView {
                rung: hint.rung,
                text: hint.text.clone(),
                check_id: hint.run_id.clone(),
            })
            .collect(),
        turns: session.turns.clone(),
    }
}

fn rendered(
    problem: &Problem,
    session: &PracticeSessionDto,
    seen: Option<&[LearningLabFile]>,
    disclose: bool,
) -> String {
    engine_observation(problem, session, seen, disclose)
        .render(&program_kind())
        .unwrap()
}

fn detail(id: &str, category: &str, status: RunStatus) -> CaseCheckDetailDto {
    CaseCheckDetailDto {
        case_id: id.to_owned(),
        category: category.to_owned(),
        visibility: RunVisibility::Hidden,
        scale: false,
        mine: false,
        status,
        args: Some(vec!["[1, 2]".to_owned()]),
        expected: Some("3".to_owned()),
        actual: Some("4".to_owned()),
        printed: Some("debug".to_owned()),
        error: None,
        elapsed_ms: Some(2),
    }
}

fn run_of(
    scope: CodingRunScope,
    status: LearningPracticalRunStatus,
    cases: Vec<CaseViewDto>,
) -> PracticeCheckDto {
    PracticeCheckDto {
        run_id: "run-3333-cccc".to_owned(),
        scope,
        status,
        cases,
        stdout: "printed outside".to_owned(),
        stderr: "an error line".to_owned(),
        duration_ms: Some(9),
        reference_actual: None,
        created_at: 30,
    }
}

#[test]
fn the_observations_render_what_the_golden_says() {
    let mut golden = Golden::default();
    let problem = problem();
    let plain = session(&problem, Setting::Mock, Stage::Make);
    golden.add(
        "plain, Mock, Code, nothing run, no baseline",
        &rendered(&problem, &plain, None, false),
    );
    for task in [Form::Build, Form::Repair] {
        for setting in [Setting::Coached, Setting::Mock, Setting::Watched] {
            let (problem, session, seen) = rich_session(task, setting);
            golden.add(
                &format!("{task:?} {setting:?}"),
                &rendered(&problem, &session, Some(&seen), false),
            );
        }
    }
    assert_golden(
        "observation",
        include_str!("../../coding/golden/observation.txt"),
        golden.text(),
    );
}

#[test]
fn the_plain_fixture_holds_no_reference_and_no_misreading() {
    use crate::practice::coding::observation::test_support::{
        HIDDEN_ARGS, MISREADING_DESCRIPTION, MISREADING_ID,
    };
    for setting in [Setting::Coached, Setting::Mock, Setting::Watched] {
        let problem = problem();
        let session = session(&problem, setting, Stage::Make);
        let text = rendered(&problem, &session, None, false);
        for secret in [
            "REFERENCE_SECRET_BODY",
            MISREADING_ID,
            MISREADING_DESCRIPTION,
            "MISREADING_SECRET_BODY",
            HIDDEN_ARGS,
        ] {
            assert!(!text.contains(secret), "the rendering holds {secret}");
        }
    }
}

#[test]
fn the_rendering_numbers_every_line_and_lists_the_techniques_without_instructions() {
    let problem = problem();
    let session = session(&problem, Setting::Coached, Stage::Plan);
    let text = rendered(&problem, &session, None, false);
    assert!(text.contains("solution.py"));
    assert!(text.contains("1 | def total(values):"));
    assert!(text.contains("2 |     result = 0"));
    assert!(text.contains("3 |     return result"));
    assert!(text.contains("Techniques the problem is built around: "));
    assert!(!text.contains("never name"));
}

#[test]
fn the_rendering_carries_the_transcript_and_the_clarifications_without_turn_ids() {
    use crate::practice::practical_dto::{LearningSimulationSpeaker, LearningSimulationTurnDto};
    let problem = problem();
    let mut session = session(&problem, Setting::Coached, Stage::Understand);
    session.turns.push(LearningSimulationTurnDto {
        id: "turn-99999999".to_owned(),
        ordinal: 0,
        speaker: LearningSimulationSpeaker::Learner,
        content: "What does it return for no values?".to_owned(),
        citations: Vec::new(),
        model_name: None,
        created_at: 5,
    });
    let text = rendered(&problem, &session, None, false);
    assert!(text.contains("What does it return for no values?"));
    assert!(!text.contains("turn-99999999"));
    assert!(text.contains("Can the list be empty?"));
    assert!(text.contains("O(n) time, O(1) space"));
    assert!(text.contains("Return the sum of the values."));
}

#[test]
fn a_run_with_every_case_disclosed_renders_the_got_value() {
    let problem = problem();
    let mut session = session(&problem, Setting::Coached, Stage::Check);
    let mut scale = detail("case-scale-1", "a long list", RunStatus::Ran);
    scale.scale = true;
    scale.expected = None;
    let mut raised = detail("case-hidden-2", "a string", RunStatus::Error);
    raised.actual = None;
    raised.error = Some("ValueError: boom".to_owned());
    let mut example = detail("case-example-1", "an ordinary list", RunStatus::Passed);
    example.visibility = RunVisibility::Example;
    let cases = vec![
        cut_case(
            &detail("case-hidden-1", "an empty list", RunStatus::Failed),
            true,
        ),
        cut_case(&scale, true),
        cut_case(&raised, true),
        cut_case(&example, true),
    ];
    for scope in [
        CodingRunScope::Shown,
        CodingRunScope::All,
        CodingRunScope::Input,
    ] {
        session.runs = vec![run_of(
            scope,
            LearningPracticalRunStatus::Failed,
            cases.clone(),
        )];
        assert!(rendered(&problem, &session, None, true).contains("got 4"));
    }
}

#[test]
fn a_run_that_keeps_hidden_cases_back_but_shows_the_learners_own_renders_by_title_and_in_full() {
    let problem = problem();
    let mut session = session(&problem, Setting::Mock, Stage::Check);
    let mut mine = detail("case-mine-1", "my own input", RunStatus::Passed);
    mine.mine = true;
    let mut example = detail("case-example-1", "an ordinary list", RunStatus::Failed);
    example.visibility = RunVisibility::Example;
    let hidden = detail("case-hidden-1", "an empty list", RunStatus::Failed);
    let cases = [&hidden, &mine, &example]
        .into_iter()
        .map(|detail| cut_case(detail, case_is_open(detail.visibility, detail.mine, false)))
        .collect::<Vec<_>>();
    session.runs = vec![run_of(
        CodingRunScope::Shown,
        LearningPracticalRunStatus::Failed,
        cases,
    )];
    let text = rendered(&problem, &session, None, false);
    assert!(text.contains("- Failed: an empty list\n"));
    assert!(text.contains("my own input; input ([1, 2]); expected 3"));
}

#[test]
fn every_run_status_but_one_renders_without_the_run_id() {
    let problem = problem();
    let mut session = session(&problem, Setting::Coached, Stage::Check);
    for status in [
        LearningPracticalRunStatus::Pending,
        LearningPracticalRunStatus::Running,
        LearningPracticalRunStatus::Passed,
        LearningPracticalRunStatus::Failed,
        LearningPracticalRunStatus::TimedOut,
        LearningPracticalRunStatus::Cancelled,
        LearningPracticalRunStatus::Interrupted,
    ] {
        session.runs = vec![run_of(CodingRunScope::All, status, Vec::new())];
        assert!(!rendered(&problem, &session, None, false).contains("run-3333"));
    }
}
