//! What the model answers: read, checked against the session, and settled into rows.

use super::plan::{count_word, Plan, HINT, QUOTE};
use super::{Debrief, DebriefRow, Evidence, MAX_POINTS};
use crate::lessons::structured_call::complete_checked_request;
use crate::practice::practical_dto::LearningSimulationSpeaker;
use crate::practice::session::record::DebriefSession;
use lattice_core::application::ports::LLMPort;
use lattice_core::application::services::grounded_generation::GroundedRequest;
use lattice_core::domain::learning::practice::ScoreStatus;
use lattice_core::shared::error::{AppError, Result};
use serde::Deserialize;
use serde_json::{Map, Value};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct RawAnswer {
    rows: Vec<RawRow>,
    /// What the kind has the model state beside the rows.
    #[serde(flatten)]
    stated: Map<String, Value>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawRow {
    criterion_id: String,
    status: RawStatus,
    points: Option<i64>,
    evidence: Vec<RawEvidence>,
    feedback: String,
    next_step: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
enum RawStatus {
    Observed,
    InsufficientEvidence,
}

#[derive(Deserialize)]
struct RawEvidence {
    kind: String,
    #[serde(rename = "ref", default)]
    reference: String,
    #[serde(default)]
    text: String,
    /// The fields the kind adds to an item.
    #[serde(flatten)]
    fields: Map<String, Value>,
}

fn refuse(message: impl Into<String>) -> AppError {
    AppError::InvalidInput(message.into())
}

fn capitalised(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// The answer as a whole, or why it cannot be read: every criterion exactly once, only kinds of evidence the kind
/// names, what the kind has it state written.
fn read(raw: &str, plan: &Plan<'_>) -> Result<RawAnswer> {
    let answer: RawAnswer = serde_json::from_str(raw).map_err(|error| {
        refuse(format!(
            "The debrief was not valid JSON for its schema: {error}."
        ))
    })?;
    for criterion in &plan.criteria {
        let rows = answer
            .rows
            .iter()
            .filter(|row| row.criterion_id == criterion.id)
            .count();
        if rows != 1 {
            return Err(refuse(format!(
                "Criterion {} must have exactly one row; it has {rows}.",
                criterion.id
            )));
        }
    }
    if answer.rows.len() != plan.criteria.len() {
        return Err(refuse(format!(
            "A row names a criterion that is not one of the {}.",
            count_word(plan.criteria.len())
        )));
    }
    let tags = plan.tags();
    for row in &answer.rows {
        for item in &row.evidence {
            if !tags.contains(&item.kind) {
                return Err(refuse(format!(
                    "{}: evidence of kind {} must be one of {}.",
                    row.criterion_id,
                    item.kind,
                    tags.join(", ")
                )));
            }
        }
    }
    if let Some(statement) = &plan.statement {
        let written = statement.keys.iter().all(|key| {
            answer
                .stated
                .get(key)
                .and_then(Value::as_str)
                .is_some_and(|text| !text.trim().is_empty())
        });
        if !written {
            return Err(refuse(format!(
                "{} must be written.",
                capitalised(&statement.names)
            )));
        }
    }
    Ok(answer)
}

/// Why one evidence item is not in the session, or the item as the debrief keeps it. An error is a fault of the kind.
fn resolve(
    plan: &Plan<'_>,
    session: &DebriefSession,
    item: &RawEvidence,
) -> Result<std::result::Result<Evidence, String>> {
    let (tag, reference) = (item.kind.as_str(), item.reference.as_str());
    let noun = plan.kind.wording.check_noun();
    if tag == QUOTE {
        return Ok(resolve_quote(session, item));
    }
    if tag == HINT {
        return Ok(by_ordinal(tag, reference, |ordinal| {
            session
                .hints
                .iter()
                .any(|hint| hint.ordinal == ordinal)
                .then_some(Evidence::Hint { ordinal })
        }));
    }
    if tag == plan.check.tag {
        return Ok(session
            .checks
            .iter()
            .find(|check| check.id == reference)
            .map(|check| Evidence::Check {
                check_id: check.id.clone(),
            })
            .ok_or_else(|| format!("{tag}: there is no {noun} {reference}.")));
    }
    if plan.case.as_ref().is_some_and(|case| case.tag == tag) {
        return Ok(by_ordinal(tag, reference, |ordinal| {
            session
                .learner_cases
                .iter()
                .any(|case| case.ordinal == ordinal)
                .then_some(Evidence::LearnerCase { ordinal })
        }));
    }
    if tag != plan.part.tag {
        return Ok(Err(format!("{tag} is not a kind of evidence.")));
    }
    let Some(check) = session.checks.iter().find(|check| check.id == reference) else {
        return Ok(Err(format!("{tag}: there is no {noun} {reference}.")));
    };
    let parts = match plan
        .kind
        .account
        .parts(&session.exercise, check, &item.fields)
    {
        Ok(parts) => parts,
        Err(AppError::InvalidInput(message)) => return Ok(Err(message)),
        Err(fault) => return Err(fault),
    };
    if parts.is_empty() {
        return Ok(Err(format!(
            "{tag}: no part of {noun} {reference} is named."
        )));
    }
    for part in &parts {
        if !plan
            .kind
            .parts
            .holds(&session.exercise.brief, &check.piece, part)?
        {
            return Ok(Err(format!(
                "{tag}: {} is not in {noun} {reference}.",
                part.as_str()
            )));
        }
    }
    Ok(Ok(Evidence::Part {
        check_id: check.id.clone(),
        parts,
    }))
}

fn by_ordinal(
    tag: &str,
    reference: &str,
    found: impl Fn(u32) -> Option<Evidence>,
) -> std::result::Result<Evidence, String> {
    reference
        .parse::<u32>()
        .ok()
        .and_then(found)
        .ok_or_else(|| format!("{tag}: there is no {tag} {reference}."))
}

fn resolve_quote(
    session: &DebriefSession,
    item: &RawEvidence,
) -> std::result::Result<Evidence, String> {
    let turn = session
        .turns
        .iter()
        .find(|turn| turn.id == item.reference)
        .ok_or_else(|| format!("quote: there is no turn {}.", item.reference))?;
    if turn.speaker != LearningSimulationSpeaker::Learner {
        return Err(format!(
            "quote: turn {} is not the learner's.",
            item.reference
        ));
    }
    if item.text.is_empty() || !turn.content.contains(&item.text) {
        return Err(format!(
            "quote: the words are not in turn {}, which says: {}",
            item.reference, turn.content
        ));
    }
    Ok(Evidence::Quote {
        turn_id: turn.id.clone(),
        text: item.text.clone(),
    })
}

/// What checking an answer against the session found.
pub(super) struct Examined {
    /// Everything wrong with the answer's rows, one line each.
    pub faults: Vec<String>,
    /// The evidence of each row that is in the session, in the order of the answer's rows.
    kept: Vec<Vec<Evidence>>,
}

pub(super) fn examine(
    plan: &Plan<'_>,
    session: &DebriefSession,
    answer: &RawAnswer,
) -> Result<Examined> {
    let mut faults = Vec::new();
    let mut kept = Vec::with_capacity(answer.rows.len());
    for row in &answer.rows {
        let id = row.criterion_id.as_str();
        match (row.status, row.points) {
            (RawStatus::Observed, None) => {
                faults.push(format!("{id}: an observed row needs points."))
            }
            (RawStatus::Observed, Some(points))
                if !(0..=i64::from(MAX_POINTS)).contains(&points) =>
            {
                faults.push(format!(
                    "{id}: points must be from 0 to {MAX_POINTS}, not {points}."
                ));
            }
            (RawStatus::InsufficientEvidence, Some(_)) => {
                faults.push(format!(
                    "{id}: a row without enough evidence has no points."
                ));
            }
            _ => {}
        }
        match row.status {
            RawStatus::Observed if row.evidence.is_empty() => {
                faults.push(format!("{id}: an observed row needs evidence."));
            }
            RawStatus::InsufficientEvidence if !row.evidence.is_empty() => {
                faults.push(format!("{id}: a row without enough evidence cites none."));
            }
            _ => {}
        }
        let mut row_kept = Vec::new();
        for (at, item) in row.evidence.iter().enumerate() {
            match resolve(plan, session, item)? {
                Ok(evidence) => row_kept.push(evidence),
                Err(why) => faults.push(format!("{id}, evidence {}: {why}", at + 1)),
            }
        }
        kept.push(row_kept);
    }
    Ok(Examined { faults, kept })
}

/// The rows the answer comes to: a row keeps its points only on evidence that was checked, and every other row has
/// none.
pub(super) fn settle(plan: &Plan<'_>, answer: RawAnswer, examined: Examined) -> Result<Debrief> {
    let mut rows = Vec::with_capacity(plan.criteria.len());
    for criterion in &plan.criteria {
        let (row, checked) = answer
            .rows
            .iter()
            .zip(examined.kept.iter())
            .find(|(row, _)| row.criterion_id == criterion.id)
            .ok_or_else(|| refuse(format!("Criterion {} has no row.", criterion.id)))?;
        let points = row
            .points
            .and_then(|points| u8::try_from(points).ok())
            .filter(|points| *points <= MAX_POINTS);
        let scored = match (row.status, points) {
            (RawStatus::Observed, Some(points)) if !checked.is_empty() => Some(points),
            _ => None,
        };
        rows.push(DebriefRow {
            criterion_id: criterion.id.clone(),
            title: criterion.title.clone(),
            status: if scored.is_some() {
                ScoreStatus::Observed
            } else {
                ScoreStatus::InsufficientEvidence
            },
            points: scored,
            max_points: MAX_POINTS,
            evidence: if scored.is_some() {
                checked.clone()
            } else {
                Vec::new()
            },
            feedback: row.feedback.clone(),
            next_step: row.next_step.clone(),
        });
    }
    let stated = plan
        .statement
        .iter()
        .flat_map(|statement| &statement.keys)
        .map(|key| {
            let text = answer
                .stated
                .get(key)
                .and_then(Value::as_str)
                .ok_or_else(|| refuse(format!("{key} must be written.")))?;
            Ok((key.clone(), text.to_owned()))
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(Debrief { rows, stated })
}

/// The answer to `request`, and the raw text it was read from.
pub(super) async fn ask(
    llm: &dyn LLMPort,
    plan: &Plan<'_>,
    request: GroundedRequest,
) -> Result<(String, RawAnswer)> {
    complete_checked_request(llm, request, None, None, |raw| {
        read(raw, plan).map(|answer| (raw.to_owned(), answer))
    })
    .await
}
