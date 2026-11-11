//! The engine's observation of a program renders what today's `WorkspaceObservation` renders, byte for byte.

#![allow(clippy::indexing_slicing, clippy::unwrap_used, clippy::expect_used)]

use crate::lab_runtime::LearningLabFile;
use crate::practice::coding::dto::{
    CaseViewDto, CodeHintRung, CodingRunDto, CodingRunScope, CodingSessionDto, CodingStage,
    ExampleVerdict,
};
use crate::practice::coding::golden_support::{rich_observation, rich_session};
use crate::practice::coding::observation::test_support::{problem, session};
use crate::practice::coding::observation::{observe, WorkspaceObservation};
use crate::practice::coding::problem::{
    CaseCompare, CaseResult as RunResult, CaseStatus as RunStatus, CaseVisibility as RunVisibility,
    CodingSetting, CodingTask, Problem, ProblemCase,
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
use lattice_core::domain::learning::practice::work::{
    CheckScope, CheckStatus, Form, LearnerCaseVerdict, Rung, Setting, Stage,
};
use serde_json::json;
use std::sync::Arc;

pub(super) fn program_kind() -> Kind {
    kind(Arc::new(FakeToolchains::new(FakeRunner::new(|_, _| {
        Behaviour::Fails
    }))))
}

pub(super) fn form_of(task: CodingTask) -> Form {
    match task {
        CodingTask::Problem => Form::Build,
        CodingTask::BugFix => Form::Repair,
    }
}

pub(super) fn setting_of(setting: CodingSetting) -> Setting {
    match setting {
        CodingSetting::Coached => Setting::Coached,
        CodingSetting::Mock => Setting::Mock,
        CodingSetting::Watched => Setting::Watched,
    }
}

pub(super) fn stage_of(stage: CodingStage) -> Stage {
    match stage {
        CodingStage::Understand => Stage::Understand,
        CodingStage::Examples => Stage::Examples,
        CodingStage::Plan => Stage::Plan,
        CodingStage::Code => Stage::Make,
        CodingStage::Test => Stage::Check,
        CodingStage::Review => Stage::Review,
        CodingStage::Reproduce => Stage::Reproduce,
        CodingStage::Locate => Stage::Locate,
        CodingStage::Fix => Stage::Fix,
        CodingStage::Verify => Stage::Verify,
    }
}

pub(super) fn scope_of(scope: CodingRunScope) -> CheckScope {
    match scope {
        CodingRunScope::Examples => CheckScope::Shown,
        CodingRunScope::All => CheckScope::All,
        CodingRunScope::Input => CheckScope::Own,
    }
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

fn verdict_of(verdict: ExampleVerdict) -> LearnerCaseVerdict {
    match verdict {
        ExampleVerdict::Right => LearnerCaseVerdict::Right,
        ExampleVerdict::Wrong => LearnerCaseVerdict::Wrong,
        ExampleVerdict::Invalid => LearnerCaseVerdict::Invalid,
    }
}

fn rung_of(rung: CodeHintRung) -> Rung {
    match rung {
        CodeHintRung::Nudge => Rung::Nudge,
        CodeHintRung::Idea => Rung::Idea,
        CodeHintRung::Plan => Rung::Plan,
        CodeHintRung::Line => Rung::Point,
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
    session: &CodingSessionDto,
    seen: Option<&[LearningLabFile]>,
    disclose: bool,
) -> Observation {
    let brief = brief_of(&problem.spec).unwrap();
    let form = form_of(session.task);
    let stage = stage_of(session.stage);
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
        setting: setting_of(session.setting),
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
                rung: rung_of(hint.rung),
                text: hint.text.clone(),
                check_id: hint.run_id.clone(),
            })
            .collect(),
        turns: session.turns.clone(),
    }
}

fn assert_equal(
    problem: &Problem,
    session: &CodingSessionDto,
    seen: Option<&[LearningLabFile]>,
    disclose: bool,
    workspace: &WorkspaceObservation,
) {
    let engine = engine_observation(problem, session, seen, disclose);
    assert_eq!(engine.render(&program_kind()).unwrap(), workspace.render());
    assert_eq!(engine.internal_ids(), workspace.internal_ids());
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
) -> CodingRunDto {
    CodingRunDto {
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
fn the_plain_fixture_renders_the_same() {
    for setting in [
        CodingSetting::Coached,
        CodingSetting::Mock,
        CodingSetting::Watched,
    ] {
        let problem = problem();
        let session = session(&problem, setting, CodingStage::Code);
        let workspace = observe(&problem, &session, None);
        assert_equal(&problem, &session, None, false, &workspace);
    }
}

#[test]
fn the_rich_fixture_renders_the_same_for_both_tasks_in_every_setting() {
    for task in [CodingTask::Problem, CodingTask::BugFix] {
        for setting in [
            CodingSetting::Coached,
            CodingSetting::Mock,
            CodingSetting::Watched,
        ] {
            let (problem, session, seen) = rich_session(task, setting);
            let (_, workspace) = rich_observation(task, setting);
            assert_equal(&problem, &session, Some(&seen), false, &workspace);
        }
    }
}

#[test]
fn a_run_with_every_case_disclosed_renders_the_same() {
    let problem = problem();
    let mut session = session(&problem, CodingSetting::Coached, CodingStage::Test);
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
        CodingRunScope::Examples,
        CodingRunScope::All,
        CodingRunScope::Input,
    ] {
        session.runs = vec![run_of(
            scope,
            LearningPracticalRunStatus::Failed,
            cases.clone(),
        )];
        let workspace = observe(&problem, &session, None);
        assert!(workspace.render().contains("got 4"));
        assert_equal(&problem, &session, None, true, &workspace);
    }
}

#[test]
fn a_run_that_keeps_hidden_cases_back_but_shows_the_learners_own_renders_the_same() {
    let problem = problem();
    let mut session = session(&problem, CodingSetting::Mock, CodingStage::Test);
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
        CodingRunScope::Examples,
        LearningPracticalRunStatus::Failed,
        cases,
    )];
    let workspace = observe(&problem, &session, None);
    let text = workspace.render();
    assert!(text.contains("- Failed: an empty list\n"));
    assert!(text.contains("my own input; input ([1, 2]); expected 3"));
    assert_equal(&problem, &session, None, false, &workspace);
}

#[test]
fn every_run_status_but_one_is_worded_the_same() {
    let problem = problem();
    let mut session = session(&problem, CodingSetting::Coached, CodingStage::Test);
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
        let workspace = observe(&problem, &session, None);
        assert_equal(&problem, &session, None, false, &workspace);
    }
}
