//! Moving a question's answer key.

use super::*;

/// The answer key moved to the option a sentence is in, and why the option it named is wrong.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Rekey {
    pub right: Picked<Unit>,
    pub was_wrong: String,
}

impl Authored for Rekey {
    fn form() -> Form {
        Form::new().point::<Unit>("right").text("wasWrong")
    }

    fn read(fields: &mut Fields<'_, '_>) -> Option<Self> {
        let right = fields.point::<Unit>("right")?;
        let was_wrong = fields.text("wasWrong")?;
        Some(Self { right, was_wrong })
    }
}

/// The two reasons a key move rewrites, by field.
#[derive(Debug, Clone)]
pub(super) struct Reasons {
    right: usize,
    old: usize,
    wrong: String,
}

/// A key move that passed its checks.
#[derive(Debug, Clone)]
pub(super) struct Moved {
    part: String,
    to: usize,
    reasons: Option<Reasons>,
}

const NO_KEY: &str = "is not a sentence of an option of a question that has an answer key.";

impl Rows {
    /// The question of each key of `keys` and the option it moves to, and what is wrong with them.
    pub(super) fn key_moves(
        &self,
        keys: &[Rekey],
        current: &Value,
        problems: &mut Vec<String>,
    ) -> Vec<Moved> {
        let mut moves: Vec<Moved> = Vec::new();
        for key in keys {
            let label = key.right.handle.written();
            let Some(field) = self.field_of(&key.right.reference) else {
                push_once(problems, format!("{label} {NO_KEY}"));
                continue;
            };
            let part = field.part.clone();
            let to = field
                .pointer
                .strip_prefix(&format!("{part}/options/"))
                .and_then(|place| place.parse::<usize>().ok());
            let now = current
                .pointer(&format!("{part}/correctIndex"))
                .and_then(Value::as_u64)
                .and_then(|index| usize::try_from(index).ok());
            let (true, Some(to), Some(now)) = (part.starts_with("/questions/"), to, now) else {
                push_once(problems, format!("{label} {NO_KEY}"));
                continue;
            };
            if to == now {
                push_once(
                    problems,
                    format!("The answer key already names the option {label} is in."),
                );
                continue;
            }
            if moves.iter().any(|moved| moved.part == part) {
                push_once(
                    problems,
                    "An entry moves a question's answer key once.".to_owned(),
                );
                continue;
            }
            let options = current
                .pointer(&format!("{part}/options"))
                .and_then(Value::as_array)
                .map(Vec::len);
            let reasons = current
                .pointer(&format!("{part}/rationales"))
                .and_then(Value::as_array)
                .map(Vec::len);
            let wrong = key.was_wrong.trim();
            let reasoned = options.is_some() && options == reasons;
            if reasoned && wrong.is_empty() {
                push_once(
                    problems,
                    "Say in wasWrong why the option the answer key named until now is wrong."
                        .to_owned(),
                );
                continue;
            }
            let at = |option: usize| {
                let pointer = format!("{part}/rationales/{option}");
                self.fields
                    .iter()
                    .position(|field| field.pointer == pointer)
            };
            let reasons = match (reasoned, at(to), at(now)) {
                (true, Some(right), Some(old)) => Some(Reasons {
                    right,
                    old,
                    wrong: wrong.to_owned(),
                }),
                _ => None,
            };
            moves.push(Moved { part, to, reasons });
        }
        moves
    }

    /// Makes the moves: each key and its two reasons. The `wasWrong` texts stored.
    pub(super) fn move_keys(&mut self, moves: Vec<Moved>) -> Vec<String> {
        let mut written = Vec::new();
        for moved in moves {
            if let Some(reasons) = moved.reasons {
                if let Some(field) = self.fields.get_mut(reasons.right) {
                    field.units.rewrite("");
                }
                if let Some(field) = self.fields.get_mut(reasons.old) {
                    field.units.rewrite(&reasons.wrong);
                }
                written.push(reasons.wrong);
            }
            self.keyed.retain(|(part, _)| *part != moved.part);
            self.keyed.push((moved.part, moved.to));
        }
        written
    }
}
