//! How a Program's values are written in the core vocabulary, and back.

use crate::lab_runtime::LearningLabFile;
use crate::practice::coding::dto::{CodeHintRung, CodingRunScope, CodingStage, ExampleVerdict};
use crate::practice::coding::policy::CodingCue;
use crate::practice::coding::problem::{
    self as coding, CodingSetting, CodingTask, Problem, ProblemCase, ProblemSpec,
};
use crate::practice::kinds::program::KIND;
use crate::practice::practical_dto::LearningPracticalRunStatus;
use crate::practice::session::policy::Cue;
use lattice_core::domain::learning::practice::work::{
    self, Brief, Case, CheckScope, CheckStatus, Exercise, Form, KindId, LearnerCaseVerdict,
    Misreading, Piece, Rung, Setting, Stage,
};
use lattice_core::shared::error::{AppError, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[cfg(test)]
mod tests;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CaseBody {
    args: Vec<String>,
    expected: Option<String>,
    compare: coding::CaseCompare,
    scale: bool,
    digest: Option<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ResultDetail {
    actual: Option<String>,
    printed: String,
    error: Option<String>,
    elapsed_ms: Option<u64>,
}

#[derive(Serialize, Deserialize)]
struct RunDetail {
    #[serde(default)]
    stdout: String,
    #[serde(default)]
    stderr: String,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExerciseProof {
    reference_ms: BTreeMap<String, u64>,
}

const TITLE_KEY: &str = "title";
const DIFFICULTY_KEY: &str = "difficulty";

fn invalid(what: &str, error: impl std::fmt::Display) -> AppError {
    AppError::InvalidState(format!("{what}: {error}."))
}

/// The piece that holds `files`.
pub fn piece_of(files: &[LearningLabFile]) -> Result<Piece> {
    serde_json::to_value(files)
        .map(Piece::new)
        .map_err(|error| invalid("The files could not be written", error))
}

pub fn files_of(piece: &Piece) -> Result<Vec<LearningLabFile>> {
    serde_json::from_value(piece.value().clone())
        .map_err(|error| invalid("The piece is not a list of files", error))
}

/// The piece that holds `code` as the solution file.
pub fn solution_piece(code: &str) -> Result<Piece> {
    piece_of(&[LearningLabFile {
        path: coding::SOLUTION_PATH.to_owned(),
        content: code.to_owned(),
    }])
}

/// The code of the piece's solution file.
pub fn solution_of(piece: &Piece) -> Result<String> {
    files_of(piece)?
        .into_iter()
        .find(|file| file.path == coding::SOLUTION_PATH)
        .map(|file| file.content)
        .ok_or_else(|| {
            AppError::InvalidState(format!("The piece has no {} file.", coding::SOLUTION_PATH))
        })
}

fn difficulty_of(difficulty: coding::ProblemDifficulty) -> work::Difficulty {
    match difficulty {
        coding::ProblemDifficulty::Easy => work::Difficulty::Easy,
        coding::ProblemDifficulty::Medium => work::Difficulty::Medium,
        coding::ProblemDifficulty::Hard => work::Difficulty::Hard,
    }
}

pub fn spec_difficulty_of(difficulty: work::Difficulty) -> coding::ProblemDifficulty {
    match difficulty {
        work::Difficulty::Easy => coding::ProblemDifficulty::Easy,
        work::Difficulty::Medium => coding::ProblemDifficulty::Medium,
        work::Difficulty::Hard => coding::ProblemDifficulty::Hard,
    }
}

/// The brief of `spec`; its title and difficulty are kept in the brief and not in the stored spec.
pub fn brief_of(spec: &ProblemSpec) -> Result<Brief> {
    let mut value = serde_json::to_value(spec)
        .map_err(|error| invalid("The problem could not be written", error))?;
    if let Some(fields) = value.as_object_mut() {
        fields.remove(TITLE_KEY);
        fields.remove(DIFFICULTY_KEY);
    }
    Ok(Brief {
        title: spec.title.clone(),
        difficulty: difficulty_of(spec.difficulty),
        spec: value,
    })
}

pub fn spec_of(brief: &Brief) -> Result<ProblemSpec> {
    let mut value = brief.spec.clone();
    let Some(fields) = value.as_object_mut() else {
        return Err(AppError::InvalidState(
            "The brief is not a program's spec: it is not an object.".into(),
        ));
    };
    fields.insert(TITLE_KEY.to_owned(), brief.title.clone().into());
    fields.insert(
        DIFFICULTY_KEY.to_owned(),
        serde_json::to_value(spec_difficulty_of(brief.difficulty))
            .map_err(|error| invalid("The difficulty could not be written", error))?,
    );
    serde_json::from_value(value)
        .map_err(|error| invalid("The brief is not a program's spec", error))
}

pub fn case_of(case: &ProblemCase) -> Result<Case> {
    let body = CaseBody {
        args: case.args.clone(),
        expected: case.expected.clone(),
        compare: case.compare.clone(),
        scale: case.scale,
        digest: case.digest.clone(),
    };
    Ok(Case {
        id: case.id.clone(),
        title: case.category.clone(),
        visibility: match case.visibility {
            coding::CaseVisibility::Example => work::CaseVisibility::Shown,
            coding::CaseVisibility::Hidden => work::CaseVisibility::Hidden,
        },
        body: serde_json::to_value(body)
            .map_err(|error| invalid("The case could not be written", error))?,
    })
}

pub fn problem_case_of(case: &Case) -> Result<ProblemCase> {
    let body: CaseBody = serde_json::from_value(case.body.clone()).map_err(|error| {
        invalid(
            &format!("The body of the case {} is not a program's case", case.id),
            error,
        )
    })?;
    Ok(ProblemCase {
        id: case.id.clone(),
        category: case.title.clone(),
        args: body.args,
        expected: body.expected,
        compare: body.compare,
        visibility: match case.visibility {
            work::CaseVisibility::Shown => coding::CaseVisibility::Example,
            work::CaseVisibility::Hidden => coding::CaseVisibility::Hidden,
        },
        scale: body.scale,
        digest: body.digest,
    })
}

fn status_of(status: coding::CaseStatus) -> work::CaseStatus {
    match status {
        coding::CaseStatus::Passed => work::CaseStatus::Passed,
        coding::CaseStatus::Failed => work::CaseStatus::Failed,
        coding::CaseStatus::Error => work::CaseStatus::Error,
        coding::CaseStatus::Ran => work::CaseStatus::Ran,
        coding::CaseStatus::TimedOut => work::CaseStatus::TimedOut,
        coding::CaseStatus::NotRun => work::CaseStatus::NotRun,
    }
}

fn run_status_of(status: work::CaseStatus) -> coding::CaseStatus {
    match status {
        work::CaseStatus::Passed => coding::CaseStatus::Passed,
        work::CaseStatus::Failed => coding::CaseStatus::Failed,
        work::CaseStatus::Error => coding::CaseStatus::Error,
        work::CaseStatus::Ran => coding::CaseStatus::Ran,
        work::CaseStatus::TimedOut => coding::CaseStatus::TimedOut,
        work::CaseStatus::NotRun => coding::CaseStatus::NotRun,
    }
}

pub fn result_of(result: &coding::CaseResult) -> Result<work::CaseResult> {
    let detail = ResultDetail {
        actual: result.actual.clone(),
        printed: result.printed.clone(),
        error: result.error.clone(),
        elapsed_ms: result.elapsed_ms,
    };
    Ok(work::CaseResult {
        case_id: result.case_id.clone(),
        status: status_of(result.status),
        detail: serde_json::to_value(detail)
            .map_err(|error| invalid("The result could not be written", error))?,
    })
}

pub fn run_result_of(result: &work::CaseResult) -> Result<coding::CaseResult> {
    let detail: ResultDetail = serde_json::from_value(result.detail.clone()).map_err(|error| {
        invalid(
            &format!(
                "The detail of the result for the case {} is not a program's",
                result.case_id
            ),
            error,
        )
    })?;
    Ok(coding::CaseResult {
        case_id: result.case_id.clone(),
        status: run_status_of(result.status),
        actual: detail.actual,
        printed: detail.printed,
        error: detail.error,
        elapsed_ms: detail.elapsed_ms,
    })
}

/// The detail of a check whose program printed `stdout` and `stderr` outside any case.
pub fn run_detail_of(stdout: &str, stderr: &str) -> Result<serde_json::Value> {
    serde_json::to_value(RunDetail {
        stdout: stdout.to_owned(),
        stderr: stderr.to_owned(),
    })
    .map_err(|error| invalid("The run could not be written", error))
}

/// What a check's detail says the program printed outside any case, as `(stdout, stderr)`. A detail of `Null`
/// says nothing.
pub fn run_output_of(detail: &serde_json::Value) -> Result<(String, String)> {
    if detail.is_null() {
        return Ok((String::new(), String::new()));
    }
    let detail: RunDetail = serde_json::from_value(detail.clone())
        .map_err(|error| invalid("The detail of the check is not a program's", error))?;
    Ok((detail.stdout, detail.stderr))
}

pub fn misreading_of(misreading: &coding::Misreading) -> Result<Misreading> {
    Ok(Misreading {
        id: misreading.id.clone(),
        description: misreading.description.clone(),
        piece: solution_piece(&misreading.code)?,
        killed_by: misreading.killed_by.clone(),
    })
}

pub fn problem_misreading_of(misreading: &Misreading) -> Result<coding::Misreading> {
    Ok(coding::Misreading {
        id: misreading.id.clone(),
        description: misreading.description.clone(),
        code: solution_of(&misreading.piece)?,
        killed_by: misreading.killed_by.clone(),
    })
}

/// The proof record of an exercise whose reference took `reference_ms` on its scale cases.
pub fn proof_of(reference_ms: &BTreeMap<String, u64>) -> Result<serde_json::Value> {
    serde_json::to_value(ExerciseProof {
        reference_ms: reference_ms.clone(),
    })
    .map_err(|error| invalid("The proof could not be written", error))
}

pub fn exercise_of(problem: &Problem) -> Result<Exercise> {
    let (form, start) = match &problem.bug {
        Some(bug) => {
            let planted = problem
                .misreadings
                .iter()
                .find(|misreading| &misreading.id == bug)
                .ok_or_else(|| {
                    AppError::InvalidState(format!(
                        "The bug is the misreading {bug}, which the problem does not have."
                    ))
                })?;
            (Form::Repair, solution_piece(&planted.code)?)
        }
        None => (Form::Build, solution_piece(&problem.spec.starter)?),
    };
    Ok(Exercise {
        kind: KindId::new(KIND),
        form,
        brief: brief_of(&problem.spec)?,
        reference: solution_piece(&problem.reference)?,
        misreadings: problem
            .misreadings
            .iter()
            .map(misreading_of)
            .collect::<Result<Vec<_>>>()?,
        cases: problem
            .cases
            .iter()
            .map(case_of)
            .collect::<Result<Vec<_>>>()?,
        start,
        planted: problem.bug.clone(),
        proof: proof_of(&problem.reference_ms)?,
    })
}

pub fn problem_of(exercise: &Exercise) -> Result<Problem> {
    if exercise.kind.as_str() != KIND {
        return Err(AppError::InvalidState(format!(
            "The exercise is of the kind {}, not {KIND}.",
            exercise.kind.as_str()
        )));
    }
    if matches!(exercise.form, Form::Complete | Form::Arrange) {
        return Err(AppError::InvalidState(format!(
            "A program has no {} exercises.",
            exercise.form.label()
        )));
    }
    let proof: ExerciseProof = serde_json::from_value(exercise.proof.clone())
        .map_err(|error| invalid("The exercise's proof is not a program's", error))?;
    Ok(Problem {
        spec: spec_of(&exercise.brief)?,
        reference: solution_of(&exercise.reference)?,
        misreadings: exercise
            .misreadings
            .iter()
            .map(problem_misreading_of)
            .collect::<Result<Vec<_>>>()?,
        cases: exercise
            .cases
            .iter()
            .map(problem_case_of)
            .collect::<Result<Vec<_>>>()?,
        reference_ms: proof.reference_ms,
        bug: exercise.planted.clone(),
    })
}

/// The setting a program's coding code calls `setting`.
pub fn coding_setting_of(setting: Setting) -> CodingSetting {
    match setting {
        Setting::Coached => CodingSetting::Coached,
        Setting::Mock => CodingSetting::Mock,
        Setting::Watched => CodingSetting::Watched,
    }
}

/// The rung a program's coding code calls `rung`: pointing is a hint at lines.
pub fn coding_rung_of(rung: Rung) -> CodeHintRung {
    match rung {
        Rung::Nudge => CodeHintRung::Nudge,
        Rung::Idea => CodeHintRung::Idea,
        Rung::Plan => CodeHintRung::Plan,
        Rung::Point => CodeHintRung::Line,
    }
}

pub fn rung_of(rung: CodeHintRung) -> Rung {
    match rung {
        CodeHintRung::Nudge => Rung::Nudge,
        CodeHintRung::Idea => Rung::Idea,
        CodeHintRung::Plan => Rung::Plan,
        CodeHintRung::Line => Rung::Point,
    }
}

/// The setting of the core vocabulary that a program's coding code calls `setting`.
pub fn setting_of(setting: CodingSetting) -> Setting {
    match setting {
        CodingSetting::Coached => Setting::Coached,
        CodingSetting::Mock => Setting::Mock,
        CodingSetting::Watched => Setting::Watched,
    }
}

/// The form of a task.
pub fn form_of(task: CodingTask) -> Form {
    match task {
        CodingTask::Problem => Form::Build,
        CodingTask::BugFix => Form::Repair,
    }
}

pub fn stage_of(stage: CodingStage) -> Stage {
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

pub fn coding_stage_of(stage: Stage) -> CodingStage {
    match stage {
        Stage::Understand => CodingStage::Understand,
        Stage::Examples => CodingStage::Examples,
        Stage::Plan => CodingStage::Plan,
        Stage::Make => CodingStage::Code,
        Stage::Check => CodingStage::Test,
        Stage::Review => CodingStage::Review,
        Stage::Reproduce => CodingStage::Reproduce,
        Stage::Locate => CodingStage::Locate,
        Stage::Fix => CodingStage::Fix,
        Stage::Verify => CodingStage::Verify,
    }
}

pub fn scope_of(scope: CodingRunScope) -> CheckScope {
    match scope {
        CodingRunScope::Examples => CheckScope::Shown,
        CodingRunScope::All => CheckScope::All,
        CodingRunScope::Input => CheckScope::Own,
    }
}

pub fn coding_scope_of(scope: CheckScope) -> CodingRunScope {
    match scope {
        CheckScope::Shown => CodingRunScope::Examples,
        CheckScope::All => CodingRunScope::All,
        CheckScope::Own => CodingRunScope::Input,
    }
}

pub fn verdict_of(verdict: ExampleVerdict) -> LearnerCaseVerdict {
    match verdict {
        ExampleVerdict::Right => LearnerCaseVerdict::Right,
        ExampleVerdict::Wrong => LearnerCaseVerdict::Wrong,
        ExampleVerdict::Invalid => LearnerCaseVerdict::Invalid,
    }
}

/// The engine's cue as the coding code stores it.
pub fn coding_cue_of(_cue: &Cue) -> CodingCue {
    CodingCue::Idle
}

/// The cue the coding code stored, in the engine's words.
pub fn cue_of(_cue: &CodingCue) -> Cue {
    Cue::Idle
}

/// How a run stands, as a check stands.
pub fn check_status_of(status: LearningPracticalRunStatus) -> CheckStatus {
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
