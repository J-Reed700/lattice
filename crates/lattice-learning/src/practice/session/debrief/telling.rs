//! The session as the debrief's model reads it: one JSON document, keys in the order the kind names its work.

use super::plan::Plan;
use crate::practice::practical_dto::LearningSimulationSpeaker;
use crate::practice::session::record::DebriefSession;
use lattice_core::shared::error::{AppError, Result};
use serde_json::{json, Map, Value};

fn speaker(speaker: LearningSimulationSpeaker) -> &'static str {
    match speaker {
        LearningSimulationSpeaker::Learner => "learner",
        LearningSimulationSpeaker::Counterpart => "counterpart",
        LearningSimulationSpeaker::Coach => "coach",
    }
}

pub(super) fn told(plan: &Plan<'_>, session: &DebriefSession) -> Result<String> {
    let kind = plan.kind;
    let (account, wording) = (&kind.account, &kind.wording);
    let exercise = &session.exercise;
    let mut body = Map::new();
    body.insert(account.exercise_key(), account.exercise(exercise)?);
    body.insert(account.form_key(), account.form(session.form)?);
    body.insert(
        "settings".to_owned(),
        Value::Array(
            session
                .settings
                .iter()
                .map(|setting| Value::String(setting.label().to_lowercase()))
                .collect(),
        ),
    );
    body.insert(
        "transcript".to_owned(),
        Value::Array(
            session
                .turns
                .iter()
                .map(|turn| json!({"turnId": turn.id, "speaker": speaker(turn.speaker), "text": turn.content}))
                .collect(),
        ),
    );
    match &kind.learner_cases {
        Some(cases) => {
            let told = session
                .learner_cases
                .iter()
                .map(|case| cases.account(&exercise.brief, case))
                .collect::<Result<Vec<_>>>()?;
            body.insert(cases.key(), Value::Array(told));
        }
        None if !session.learner_cases.is_empty() => {
            return Err(AppError::InvalidState(
                "The session has cases the learner wrote, and its kind takes none.".into(),
            ));
        }
        None => {}
    }
    let mut checks = Vec::with_capacity(session.checks.len());
    for (at, check) in session.checks.iter().enumerate() {
        let before = at
            .checked_sub(1)
            .and_then(|before| session.checks.get(before));
        checks.push(account.check(exercise, check, before)?);
    }
    body.insert(account.checks_key(), Value::Array(checks));
    let held = session.checks.last().map(|check| &check.piece) == Some(&session.draft);
    body.insert(
        "finalDraft".to_owned(),
        if held {
            Value::Null
        } else {
            account.piece(exercise, &session.draft)?
        },
    );
    body.insert(
        "hints".to_owned(),
        Value::Array(
            session
                .hints
                .iter()
                .map(|hint| {
                    json!({
                        "ordinal": hint.ordinal,
                        "rung": wording.rung_label(hint.rung).to_lowercase(),
                        "text": hint.text,
                        "stage": wording.stage_label(&exercise.brief, session.form, hint.stage).to_lowercase(),
                    })
                })
                .collect(),
        ),
    );
    serde_json::to_string_pretty(&Value::Object(body)).map_err(|error| {
        AppError::InternalError(format!("The session could not be written out: {error}"))
    })
}
