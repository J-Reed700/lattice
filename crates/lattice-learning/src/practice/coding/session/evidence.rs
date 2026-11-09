//! What a finished session is judged on: the evidence the debrief reads and the outcome the pattern ratings use.
use super::view::{example_views, settings_used};
use crate::lab_runtime::LearningLabFile;
use crate::practical_dto::CaseCheckDetailDto;
use crate::practice::coding::debrief::{CodingEventRow, EvidenceRun, SessionEvidence};
use crate::practice::coding::dto::{CodeHintDto, CodingRunScope};
use crate::practice::coding::patterns::ProblemOutcome;
use crate::practice::coding::problem::{CaseResult, CaseStatus};
use crate::practice::coding::records::{SessionBundle, StoredRun};
use lattice_core::shared::error::{AppError, Result};

/// The hints the session gave, in order.
pub fn hints_of(bundle: &SessionBundle) -> Result<Vec<CodeHintDto>> {
    bundle
        .events
        .iter()
        .filter(|event| event.kind == "hint")
        .map(|event| {
            serde_json::from_value(event.payload.clone())
                .map_err(|error| AppError::Serialization(error.to_string()))
        })
        .collect()
}

/// The last run that covered every case.
pub fn last_all_run(bundle: &SessionBundle) -> Option<&StoredRun> {
    bundle
        .runs
        .iter()
        .rev()
        .find(|run| run.scope == CodingRunScope::All)
}

fn passed_every_case(run: &StoredRun) -> bool {
    !run.cases.is_empty()
        && run
            .cases
            .iter()
            .all(|case| case.status == CaseStatus::Passed)
}

/// Cases passed and run on the last submit.
pub fn submit_counts(bundle: &SessionBundle) -> (u32, u32) {
    last_all_run(bundle).map_or((0, 0), |run| {
        let passed = run
            .cases
            .iter()
            .filter(|case| case.status == CaseStatus::Passed)
            .count();
        (
            u32::try_from(passed).unwrap_or(u32::MAX),
            u32::try_from(run.cases.len()).unwrap_or(u32::MAX),
        )
    })
}

/// How the session went, for the pattern ratings.
pub fn outcome_of(bundle: &SessionBundle) -> Result<ProblemOutcome> {
    let first_pass = bundle
        .runs
        .iter()
        .filter(|run| run.scope == CodingRunScope::All && passed_every_case(run))
        .map(|run| run.created_at)
        .min();
    Ok(ProblemOutcome {
        all_passed: last_all_run(bundle).is_some_and(passed_every_case),
        highest_rung: hints_of(bundle)?.iter().map(|hint| hint.rung).max(),
        solution_seen_before_passing: bundle
            .row
            .solution_seen_at
            .is_some_and(|seen| first_pass.map_or(true, |pass| seen < pass)),
    })
}

fn result_of(detail: &CaseCheckDetailDto) -> CaseResult {
    CaseResult {
        case_id: detail.case_id.clone(),
        status: detail.status,
        actual: detail.actual.clone(),
        printed: detail.printed.clone().unwrap_or_default(),
        error: detail.error.clone(),
        elapsed_ms: detail.elapsed_ms,
    }
}

/// The whole session for the debriefer: nothing is cut by a setting.
pub fn evidence_of(bundle: &SessionBundle, draft: &[LearningLabFile]) -> Result<SessionEvidence> {
    Ok(SessionEvidence {
        problem: bundle.problem.clone(),
        task: bundle.row.task,
        settings: settings_used(bundle)?,
        turns: bundle.turns.clone(),
        runs: bundle
            .runs
            .iter()
            .map(|run| EvidenceRun {
                run_id: run.run_id.clone(),
                scope: run.scope,
                files: run.files.clone(),
                results: run.cases.iter().map(result_of).collect(),
                created_at: run.created_at,
            })
            .collect(),
        examples: example_views(&bundle.examples, true),
        hints: hints_of(bundle)?,
        events: bundle
            .events
            .iter()
            .map(|event| CodingEventRow {
                ordinal: event.ordinal,
                kind: event.kind.clone(),
                payload: event.payload.clone(),
                at: event.at,
            })
            .collect(),
        draft: draft.to_vec(),
    })
}

/// The draft the counterpart last saw, from the latest `draft_seen` event.
pub fn last_draft_seen(bundle: &SessionBundle) -> Result<Option<Vec<LearningLabFile>>> {
    bundle
        .events
        .iter()
        .rev()
        .find(|event| event.kind == "draft_seen")
        .map(|event| {
            let files = event
                .payload
                .get("files")
                .ok_or_else(|| AppError::Database("A draft_seen event lacks its files.".into()))?;
            serde_json::from_value(files.clone())
                .map_err(|error| AppError::Serialization(error.to_string()))
        })
        .transpose()
}
