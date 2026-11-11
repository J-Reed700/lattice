//! What the learner's screen draws of a program: the problem, the draft, what each run found and the learner's own
//! examples, cut by what the learner may see now.

use crate::practical_dto::CaseCheckDetailDto;
use crate::practice::coding::problem::{CaseStatus, CaseVisibility, Problem, ProblemCase};
use crate::practice::kinds::program::dto::{
    CaseViewDto, CodingExampleDto, ExampleVerdict, ProgramDebriefWorkDto, ProgramHintMarkDto,
    ProgramRunDto, ProgramWorkDto, PublicProblemDto, StrongAnswerDto,
};
use crate::practice::kinds::program::learner_cases::read_body;
use crate::practice::kinds::program::parts::line_of;
use crate::practice::kinds::program::rubric::{APPROACH, COMPLEXITY};
use crate::practice::kinds::program::shape::{
    case_detail_of, coding_scope_of, example_verdict_of, files_of, learner_found_in, problem_of,
    run_output_of, run_report_of, run_status_of_check,
};
use crate::practice::session::kind::Screen;
use crate::practice::session::record::{
    Session, SessionCheck, SessionDebrief, SessionHint, SessionLearnerCase,
};
use crate::practice::session::seen::Seen;
use lattice_core::domain::learning::practice::work::CheckScope;
use lattice_core::shared::error::{AppError, Result};
use serde::Serialize;
use serde_json::Value;

#[cfg(test)]
mod tests;

pub struct ProgramScreen;

/// Whether a learner may look inside a case: the setting discloses it, it is a statement example, or it is theirs.
pub(crate) fn case_is_open(visibility: CaseVisibility, mine: bool, disclose: bool) -> bool {
    disclose || visibility == CaseVisibility::Example || mine
}

/// A case with what a learner who may not look inside it (`shown` false) does not see left out.
pub(crate) fn cut_case(detail: &CaseCheckDetailDto, shown: bool) -> CaseViewDto {
    let keep = |value: &Option<String>| value.clone().filter(|_| shown);
    CaseViewDto {
        case_id: detail.case_id.clone(),
        category: detail.category.clone(),
        visibility: detail.visibility,
        mine: detail.mine,
        scale: detail.scale,
        status: detail.status,
        args: detail.args.clone().filter(|_| shown && !detail.scale),
        expected: keep(&detail.expected),
        actual: keep(&detail.actual).filter(|_| !detail.scale),
        printed: keep(&detail.printed),
        error: keep(&detail.error),
        elapsed_ms: detail.elapsed_ms,
    }
}

/// A statement example as the learner sees it before any run.
pub(crate) fn example_view(case: &ProblemCase) -> CaseViewDto {
    CaseViewDto {
        case_id: case.id.clone(),
        category: case.category.clone(),
        visibility: case.visibility,
        mine: false,
        scale: case.scale,
        status: CaseStatus::NotRun,
        args: Some(case.args.clone()),
        expected: case.expected.clone(),
        actual: None,
        printed: None,
        error: None,
        elapsed_ms: None,
    }
}

fn public_problem(problem: &Problem) -> PublicProblemDto {
    PublicProblemDto {
        title: problem.spec.title.clone(),
        statement: problem.spec.statement.clone(),
        signature: problem.spec.signature.clone(),
        constraints: problem.spec.constraints.clone(),
        difficulty: problem.spec.difficulty,
        language: problem.spec.language.clone(),
        examples: problem.examples().map(example_view).collect(),
        starter: problem.spec.starter.clone(),
    }
}

/// The learner's example as the learner sees it. `expects` is what the reference returned for a wrong example.
fn example_dto(case: &SessionLearnerCase, seen: &Seen) -> Result<CodingExampleDto> {
    let body = read_body(&case.body)?;
    let found = learner_found_in(&case.found)?;
    let verdict = example_verdict_of(case.verdict);
    Ok(CodingExampleDto {
        ordinal: case.ordinal,
        args: body.args,
        expected: body.expected,
        verdict,
        expects: found
            .reference_actual
            .filter(|_| seen.failing_case && verdict == ExampleVerdict::Wrong),
        why_invalid: found.why_invalid,
    })
}

fn case_details(check: &SessionCheck) -> Result<Vec<CaseCheckDetailDto>> {
    check
        .cases
        .iter()
        .zip(&check.report.results)
        .map(|(checked, result)| case_detail_of(&checked.case, Some(result), checked.own))
        .collect()
}

fn run_dto(check: &SessionCheck, seen: &Seen) -> Result<ProgramRunDto> {
    let (stdout, stderr) = run_output_of(&check.report.detail)?;
    let (duration_ms, reference_actual) = run_report_of(&check.report.detail)?;
    Ok(ProgramRunDto {
        run_id: check.id.clone(),
        scope: coding_scope_of(check.scope),
        status: run_status_of_check(check.status),
        cases: case_details(check)?
            .iter()
            .map(|detail| {
                cut_case(
                    detail,
                    case_is_open(detail.visibility, detail.mine, seen.failing_case),
                )
            })
            .collect(),
        stdout,
        stderr,
        duration_ms,
        reference_actual: reference_actual.filter(|_| seen.failing_case),
        created_at: check.at,
    })
}

/// Where a hint points, when it points at lines.
fn hint_mark(hint: &SessionHint) -> Result<Option<ProgramHintMarkDto>> {
    let mut path = None;
    let mut lines = Vec::new();
    for part in &hint.parts {
        let (file, line) = line_of(part)?;
        path.get_or_insert(file);
        lines.push(line);
    }
    Ok((!lines.is_empty()).then_some(ProgramHintMarkDto {
        ordinal: hint.ordinal,
        path,
        lines,
    }))
}

fn written<T: Serialize>(work: &T) -> Result<Value> {
    serde_json::to_value(work).map_err(|error| AppError::Serialization(error.to_string()))
}

fn stated(debrief: &SessionDebrief, key: &str) -> Result<String> {
    debrief
        .stated
        .iter()
        .find(|(known, _)| known == key)
        .map(|(_, text)| text.clone())
        .ok_or_else(|| AppError::InvalidState(format!("The debrief states no {key}.")))
}

impl Screen for ProgramScreen {
    fn work(&self, session: &Session, seen: &Seen) -> Result<Value> {
        let problem = problem_of(&session.exercise)?;
        written(&ProgramWorkDto {
            problem: public_problem(&problem),
            draft: files_of(&session.row.draft)?,
            examples: session
                .learner_cases
                .iter()
                .map(|case| example_dto(case, seen))
                .collect::<Result<Vec<_>>>()?,
            runs: session
                .checks
                .iter()
                .map(|check| run_dto(check, seen))
                .collect::<Result<Vec<_>>>()?,
            hint_marks: session
                .hints
                .iter()
                .filter_map(|hint| hint_mark(hint).transpose())
                .collect::<Result<Vec<_>>>()?,
            worked_solution: seen.reference.then(|| problem.reference.clone()),
        })
    }

    fn debrief(&self, session: &Session, debrief: &SessionDebrief) -> Result<Value> {
        let problem = problem_of(&session.exercise)?;
        let cases = match session
            .checks
            .iter()
            .rev()
            .find(|check| check.scope == CheckScope::All)
        {
            Some(check) => case_details(check)?
                .iter()
                .map(|detail| cut_case(detail, true))
                .collect(),
            None => Vec::new(),
        };
        written(&ProgramDebriefWorkDto {
            strong_answer: StrongAnswerDto {
                approach: stated(debrief, APPROACH)?,
                complexity: stated(debrief, COMPLEXITY)?,
                reference: problem.reference,
            },
            cases,
        })
    }
}
