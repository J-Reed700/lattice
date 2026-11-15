//! The session as the debrief's model reads it: one JSON document, keys in the order the kind names its work. Every
//! thing the model may point at holds a place in it, which the request fills with the thing's name.

use super::things::{Example, Hinted, Line, Run, Said};
use crate::practice::practical_dto::LearningSimulationSpeaker;
use crate::practice::session::kind::Kind;
use crate::practice::session::record::DebriefSession;
use lattice_core::application::services::model_contract::shown::Told;
use lattice_core::domain::learning::practice::work::PartRef;
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::text::sentences;
use serde_json::{json, Map, Value};
use std::cell::RefCell;
use std::collections::HashMap;

const OPEN: char = '\u{1}';
const CLOSE: char = '\u{2}';

/// What the place of the `at`th thing of a kind of pointable things is called.
pub(super) fn token(kind: char, at: usize) -> String {
    format!("{kind}{at}")
}

fn hole(kind: char, at: usize) -> String {
    format!("{OPEN}{}{CLOSE}", token(kind, at))
}

/// The text with each place in it filled with the name `names` gives it.
fn filled_text(text: &str, names: &HashMap<String, String>) -> String {
    let mut pieces = text.split(OPEN);
    let mut out = pieces.next().unwrap_or_default().to_owned();
    for piece in pieces {
        match piece.split_once(CLOSE) {
            Some((place, rest)) if names.contains_key(place) => {
                out.push_str(names.get(place).map_or("", String::as_str));
                out.push_str(rest);
            }
            _ => {
                out.push(OPEN);
                out.push_str(piece);
            }
        }
    }
    out
}

/// The document with every place in it filled with the name `names` gives it.
pub(super) fn filled(value: &Value, names: &HashMap<String, String>) -> Value {
    match value {
        Value::String(text) => Value::String(filled_text(text, names)),
        Value::Array(items) => {
            Value::Array(items.iter().map(|item| filled(item, names)).collect())
        }
        Value::Object(fields) => Value::Object(
            fields
                .iter()
                .map(|(key, item)| (key.clone(), filled(item, names)))
                .collect(),
        ),
        other => other.clone(),
    }
}

/// The places a kind writes the names of a check and of the parts of its piece into: the request fills each with
/// the name the model reads.
pub struct Names {
    check: String,
    same_as: Option<String>,
    first: usize,
    parts: RefCell<Vec<PartRef>>,
}

impl Names {
    pub(crate) fn new(check: usize, same_as: Option<usize>, first: usize) -> Self {
        Self {
            check: hole('r', check),
            same_as: same_as.map(|before| hole('r', before)),
            first,
            parts: RefCell::new(Vec::new()),
        }
    }

    /// The name of the check.
    pub fn check(&self) -> String {
        self.check.clone()
    }

    /// The name of the check made just before, when it holds the same piece: the piece is then told by saying so.
    pub fn same_as(&self) -> Option<String> {
        self.same_as.clone()
    }

    /// The name of a part of the check's piece. A part that is named is one the model may cite.
    pub fn part(&self, part: &PartRef) -> String {
        let mut named = self.parts.borrow_mut();
        let at = match named.iter().position(|known| known == part) {
            Some(at) => at,
            None => {
                named.push(part.clone());
                named.len().saturating_sub(1)
            }
        };
        hole('p', self.first + at)
    }

    fn into_parts(self) -> Vec<PartRef> {
        self.parts.into_inner()
    }
}

/// The session told, with the things the model may point at.
pub(super) struct Rendered {
    pub body: Value,
    pub runs: Vec<Run>,
    pub lines: Vec<Line>,
    pub said: Vec<Said>,
    pub examples: Vec<Example>,
    pub hints: Vec<Hinted>,
}

fn speaker(speaker: LearningSimulationSpeaker) -> &'static str {
    match speaker {
        LearningSimulationSpeaker::Learner => "learner",
        LearningSimulationSpeaker::Counterpart => "counterpart",
        LearningSimulationSpeaker::Coach => "coach",
    }
}

/// The sentences of a learner's turn, one to a row, each under the place of its name.
fn sentence_rows(turn: &str, content: &str, said: &mut Vec<Said>) -> String {
    let mut rows = Vec::new();
    for (index, (start, stop)) in sentences(content).into_iter().enumerate() {
        let Some(words) = content.get(start..stop) else {
            continue;
        };
        rows.push(format!(
            "{} {}",
            hole('s', said.len()),
            words.replace(['\r', '\n'], " ")
        ));
        said.push(Said {
            turn: turn.to_owned(),
            index,
            words: Told(words.to_owned()),
        });
    }
    rows.join("\n")
}

/// The exercise and the form of the session, under the keys the kind names them by.
pub(super) fn exercise_told(kind: &Kind, session: &DebriefSession) -> Result<Map<String, Value>> {
    let account = &kind.account;
    let mut body = Map::new();
    body.insert(account.exercise_key(), account.exercise(&session.exercise)?);
    body.insert(account.form_key(), account.form(session.form)?);
    Ok(body)
}

pub(super) fn render(kind: &Kind, session: &DebriefSession) -> Result<Rendered> {
    let (account, wording) = (&kind.account, &kind.wording);
    let exercise = &session.exercise;
    let mut body = exercise_told(kind, session)?;
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
    let mut said = Vec::new();
    let mut turns = Vec::with_capacity(session.turns.len());
    for turn in &session.turns {
        let text = if turn.speaker == LearningSimulationSpeaker::Learner {
            sentence_rows(&turn.id, &turn.content, &mut said)
        } else {
            turn.content.clone()
        };
        turns.push(json!({"speaker": speaker(turn.speaker), "text": text}));
    }
    body.insert("transcript".to_owned(), Value::Array(turns));
    let mut examples = Vec::new();
    match &kind.learner_cases {
        Some(cases) => {
            let mut told = Vec::with_capacity(session.learner_cases.len());
            for case in &session.learner_cases {
                told.push(cases.account(&exercise.brief, case, &hole('e', examples.len()))?);
                examples.push(Example {
                    ordinal: case.ordinal,
                });
            }
            body.insert(cases.key(), Value::Array(told));
        }
        None if !session.learner_cases.is_empty() => {
            return Err(AppError::InvalidState(
                "The session has cases the learner wrote, and its kind takes none.".into(),
            ));
        }
        None => {}
    }
    let mut runs = Vec::with_capacity(session.checks.len());
    let mut lines = Vec::new();
    let mut checks = Vec::with_capacity(session.checks.len());
    for (at, check) in session.checks.iter().enumerate() {
        let before = at
            .checked_sub(1)
            .and_then(|before| session.checks.get(before));
        let same = before.filter(|before| before.piece == check.piece);
        let names = Names::new(at, same.map(|_| at.saturating_sub(1)), lines.len());
        checks.push(account.check(exercise, check, &names)?);
        lines.extend(names.into_parts().into_iter().map(|part| Line {
            check: check.id.clone(),
            part,
        }));
        runs.push(Run {
            id: check.id.clone(),
        });
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
    let mut hints = Vec::with_capacity(session.hints.len());
    let mut told = Vec::with_capacity(session.hints.len());
    for hint in &session.hints {
        told.push(json!({
            "hint": hole('h', hints.len()),
            "rung": wording.rung_label(hint.rung).to_lowercase(),
            "text": hint.text,
            "stage": wording.stage_label(&exercise.brief, session.form, hint.stage).to_lowercase(),
        }));
        hints.push(Hinted {
            ordinal: hint.ordinal,
        });
    }
    body.insert("hints".to_owned(), Value::Array(told));
    Ok(Rendered {
        body: Value::Object(body),
        runs,
        lines,
        said,
        examples,
        hints,
    })
}
