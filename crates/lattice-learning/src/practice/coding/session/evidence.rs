//! What a finished session is judged on: the evidence the debrief reads.
use crate::lab_runtime::LearningLabFile;
use crate::practical_dto::CaseCheckDetailDto;
use crate::practice::coding::debrief::{CodingEventRow, EvidenceRun, SessionEvidence};
use crate::practice::coding::problem::CaseResult;
use crate::practice::coding::records::{SessionBundle, StoredExample, StoredHint};
use crate::practice::kinds::program::dto::{CodingExampleDto, ExampleVerdict};
use lattice_core::domain::learning::practice::work::Setting;
use lattice_core::shared::error::{AppError, Result};

/// The hints the session gave, in order.
pub fn hints_of(bundle: &SessionBundle) -> Result<Vec<StoredHint>> {
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

/// The settings a session has used, in order, from the one it started in.
pub fn settings_used(bundle: &SessionBundle) -> Result<Vec<Setting>> {
    let mut changes = Vec::new();
    for event in bundle.events.iter().filter(|event| event.kind == "setting") {
        let setting = |key: &str| -> Result<Setting> {
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

/// What the reference answered on the input of a run, from the run's `check` event.
pub fn reference_actual_of(bundle: &SessionBundle, run_id: &str) -> Option<String> {
    bundle
        .events
        .iter()
        .filter(|event| event.kind == "check")
        .find(|event| event.payload.get("runId").and_then(|id| id.as_str()) == Some(run_id))
        .and_then(|event| event.payload.get("referenceActual"))
        .and_then(|actual| actual.as_str())
        .map(str::to_owned)
}

pub fn result_of(detail: &CaseCheckDetailDto) -> CaseResult {
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
        form: bundle.row.form,
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
