//! What the screen sees of a session: the stored facts, cut by the session's setting.
use super::evidence::{evidence_of, hints_of, last_all_run};
use crate::lab_runtime::LearningLabFile;
use crate::practical_dto::CaseCheckDetailDto;
use crate::practice::coding::debrief::facts;
use crate::practice::coding::dto::{
    stages_of, CaseViewDto, ClockDto, CodingDebriefDto, CodingExampleDto, CodingRunDto,
    CodingSessionDto, ExampleCoverageDto, ExampleVerdict, PublicProblemDto, StrongAnswerDto,
};
use crate::practice::coding::hints::next_rung;
use crate::practice::coding::problem::{
    CaseStatus, CaseVisibility, CodingSetting, Hidden, Problem,
};
use crate::practice::coding::records::{SessionBundle, StoredExample, StoredRun};
use lattice_core::shared::error::{AppError, Result};

/// The settings a session has used, in order, from the one it started in.
pub fn settings_used(bundle: &SessionBundle) -> Result<Vec<CodingSetting>> {
    let mut changes = Vec::new();
    for event in bundle.events.iter().filter(|event| event.kind == "setting") {
        let setting = |key: &str| -> Result<CodingSetting> {
            let value = event
                .payload
                .get(key)
                .ok_or_else(|| AppError::Database("A setting event lacks its setting.".into()))?;
            serde_json::from_value(value.clone())
                .map_err(|error| AppError::Serialization(error.to_string()))
        };
        changes.push((setting("from")?, setting("to")?));
    }
    let mut used = vec![changes
        .first()
        .map_or(bundle.row.setting, |(from, _)| *from)];
    for (_, to) in changes {
        if !used.contains(&to) {
            used.push(to);
        }
    }
    Ok(used)
}

fn case_view(detail: &CaseCheckDetailDto, disclose: bool) -> CaseViewDto {
    let shown = disclose || detail.visibility == CaseVisibility::Example || detail.mine;
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

fn run_view(run: &StoredRun, disclose: bool, reference_actual: Option<String>) -> CodingRunDto {
    CodingRunDto {
        run_id: run.run_id.clone(),
        scope: run.scope,
        status: run.status,
        cases: run
            .cases
            .iter()
            .map(|detail| case_view(detail, disclose))
            .collect(),
        stdout: run.stdout.clone(),
        stderr: run.stderr.clone(),
        duration_ms: run.duration_ms,
        reference_actual: reference_actual.filter(|_| disclose),
        created_at: run.created_at,
    }
}

/// The stored examples as the learner sees them. `expects` is what the reference returned for a wrong example.
pub fn example_views(examples: &[StoredExample], expects_shown: bool) -> Vec<CodingExampleDto> {
    examples
        .iter()
        .map(|example| CodingExampleDto {
            ordinal: example.ordinal,
            args: example.args.clone(),
            expected: example.expected.clone(),
            verdict: example.verdict,
            expects: example
                .reference_actual
                .clone()
                .filter(|_| expects_shown && example.verdict == ExampleVerdict::Wrong),
            why_invalid: example.why_invalid.clone(),
        })
        .collect()
}

fn caught_ids(examples: &[StoredExample]) -> Vec<&str> {
    examples
        .iter()
        .filter(|example| example.verdict == ExampleVerdict::Right)
        .flat_map(|example| example.catches.iter().map(String::as_str))
        .collect()
}

fn coverage(
    problem: &Problem,
    examples: &[StoredExample],
    uncaught_shown: bool,
) -> ExampleCoverageDto {
    let caught = caught_ids(examples);
    let missed = problem
        .misreadings
        .iter()
        .filter(|misreading| !caught.contains(&misreading.id.as_str()))
        .collect::<Vec<_>>();
    ExampleCoverageDto {
        caught: u32::try_from(problem.misreadings.len() - missed.len()).unwrap_or(u32::MAX),
        total: u32::try_from(problem.misreadings.len()).unwrap_or(u32::MAX),
        uncaught: missed
            .first()
            .filter(|_| uncaught_shown)
            .map(|misreading| misreading.description.clone()),
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
        examples: problem
            .examples()
            .map(|case| CaseViewDto {
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
            })
            .collect(),
        starter: problem.spec.starter.clone(),
    }
}

fn reference_actual_of(bundle: &SessionBundle, run_id: &str) -> Option<String> {
    bundle
        .events
        .iter()
        .filter(|event| event.kind == "run")
        .find(|event| event.payload.get("runId").and_then(|id| id.as_str()) == Some(run_id))
        .and_then(|event| event.payload.get("referenceActual"))
        .and_then(|actual| actual.as_str())
        .map(str::to_owned)
}

fn debrief_view(bundle: &SessionBundle, draft: &[LearningLabFile]) -> Result<Option<CodingDebriefDto>> {
    let (Some(stored), Some(scorecard), true) = (
        &bundle.debrief,
        &bundle.scorecard,
        bundle.row.finished_at.is_some(),
    ) else {
        return Ok(None);
    };
    let caught = caught_ids(&bundle.examples);
    Ok(Some(CodingDebriefDto {
        scorecard: scorecard.clone(),
        facts: facts(&evidence_of(bundle, draft)?),
        strong_answer: StrongAnswerDto {
            approach: stored.approach.clone(),
            complexity: stored.complexity.clone(),
            reference: bundle.problem.reference.clone(),
        },
        patterns: stored.ratings.clone(),
        cases: last_all_run(bundle)
            .map(|run| run.cases.iter().map(|detail| case_view(detail, true)).collect())
            .unwrap_or_default(),
        misreadings: bundle
            .problem
            .misreadings
            .iter()
            .map(|misreading| crate::practice::coding::dto::MisreadingViewDto {
                description: misreading.description.clone(),
                caught: caught.contains(&misreading.id.as_str()),
            })
            .collect(),
        model: stored.model.clone(),
    }))
}

/// Everything the screen shows of a session. A finished session shows all of it: the debrief keeps nothing back.
pub fn session_view(bundle: &SessionBundle) -> Result<CodingSessionDto> {
    let row = &bundle.row;
    let finished = row.finished_at.is_some();
    let submitted = row.submitted_at.is_some();
    let discloses = |hidden: Hidden| finished || row.setting.discloses(hidden, submitted);
    let hints = hints_of(bundle)?;
    let solution_available = row.setting.discloses(Hidden::WorkedSolution, submitted);
    Ok(CodingSessionDto {
        session_id: row.id.clone(),
        program_id: row.program_id.clone(),
        activity_id: row.activity_id.clone(),
        lesson_id: Some(bundle.lesson_id.clone()),
        task: row.task,
        setting: row.setting,
        settings_used: settings_used(bundle)?,
        stage: row.stage,
        stages: stages_of(row.task).to_vec(),
        problem: public_problem(&bundle.problem),
        clock: ClockDto {
            active_ms: row.active_ms,
            open_since: row.opened_at,
            budget_seconds: row.budget_seconds,
        },
        examples: example_views(&bundle.examples, discloses(Hidden::FailingCase)),
        coverage: coverage(
            &bundle.problem,
            &bundle.examples,
            discloses(Hidden::UncaughtMisreading),
        ),
        runs: bundle
            .runs
            .iter()
            .map(|run| {
                run_view(
                    run,
                    discloses(Hidden::FailingCase),
                    reference_actual_of(bundle, &run.run_id),
                )
            })
            .collect(),
        next_rung: next_rung(&hints),
        hints,
        turns: bundle.turns.clone(),
        cue_pending: row.pending_cue.is_some(),
        draft: row.draft.clone(),
        submitted,
        solution_available,
        worked_solution: (finished || (row.solution_seen_at.is_some() && solution_available))
            .then(|| bundle.problem.reference.clone()),
        debrief: debrief_view(bundle, &row.draft)?,
        finished,
        revision: row.revision,
    })
}
