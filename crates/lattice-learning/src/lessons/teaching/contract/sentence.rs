//! The sentences of a lesson and of what the learner knows, as the check
//! shows them: each under a name the model points at, and the app reads the
//! words from its own copy.
use super::Part;
use crate::review_findings::FindingSite;
use lattice_core::application::services::model_contract::shown::{
    Drawn, Handle, Internal, Kind, Showing, Shown, Told,
};
use lattice_core::application::services::model_contract::units::{Grain, Texts, Unit, UnitRef};
use lattice_core::shared::error::Result;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::ops::Range;

/// What a text of a request is a text of.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Whose {
    /// The `text`th text of the `part`th part, in the order the learner meets the parts.
    Lesson { part: usize, text: usize },
    /// The learner's prior knowledge.
    Prior,
}

/// A part of the lesson the model may name as the place something belongs.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct Spot {
    pub(super) at: usize,
}

impl Kind for Spot {
    const PREFIX: &'static str = "p";
    const DRAWN: Drawn = Drawn::Itself;
    type Ref = usize;

    fn reference(&self) -> usize {
        self.at
    }
}

/// Shows every unit of every text of `texts`, in the order the texts were added.
pub(super) fn show<'a>(
    texts: &'a Texts<Whose>,
    showing: &mut Showing<'_>,
) -> Result<Vec<Shown<'a, Unit>>> {
    texts
        .iter()
        .map(|(_, units)| units.shown(showing))
        .collect()
}

/// Each text of `texts` as one row to a unit, under the names `shown` gives them.
pub(super) fn labelled(texts: &Texts<Whose>, shown: &[Shown<'_, Unit>]) -> Vec<String> {
    texts
        .iter()
        .zip(shown)
        .map(|((_, units), shown)| units.rows(shown))
        .collect()
}

/// Each text of `texts` as one unit to a line, with no names.
pub(super) fn plain(texts: &Texts<Whose>) -> Vec<String> {
    texts
        .iter()
        .map(|(_, units)| {
            units
                .units()
                .iter()
                .map(Unit::text)
                .collect::<Vec<_>>()
                .join("\n")
        })
        .collect()
}

/// The bytes `unit` takes of its text.
pub(super) fn span_of(texts: &Texts<Whose>, unit: UnitRef) -> Option<Range<usize>> {
    texts.iter().find_map(|(_, units)| {
        units
            .spans()
            .into_iter()
            .find(|(held, _)| *held == unit)
            .map(|(_, range)| range)
    })
}

/// The unit that takes the bytes `span` of the `text`th text of the `part`th part.
pub(super) fn unit_at(
    texts: &Texts<Whose>,
    part: usize,
    text: usize,
    span: (usize, usize),
) -> Option<UnitRef> {
    let wanted = Whose::Lesson { part, text };
    texts
        .iter()
        .filter(|(of, _)| **of == wanted)
        .find_map(|(_, units)| {
            units
                .spans()
                .into_iter()
                .find(|(_, range)| (range.start, range.end) == span)
                .map(|(unit, _)| unit)
        })
}

/// One text of a part.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct Text {
    pub(super) at: Internal<String>,
    pub(super) of: Internal<usize>,
    pub(super) words: Told<String>,
}

/// A section or question as the request shows it: its fields but for the texts, which stand as sentences.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct View {
    pub(super) order: Internal<usize>,
    pub(super) frame: Told<Value>,
    pub(super) texts: Vec<Text>,
    pub(super) spot: Option<Spot>,
}

impl View {
    fn of(order: usize, part: &Part, named: bool) -> Self {
        let mut frame = part.shown.clone();
        if let Some(fields) = frame.as_object_mut() {
            fields.shift_remove("id");
        }
        let mut texts = Vec::new();
        for (text, (field, words)) in part.fields.iter().zip(&part.texts).enumerate() {
            let pointer = format!("/{field}");
            let Some(slot) = frame.pointer_mut(&pointer) else {
                continue;
            };
            *slot = Value::Null;
            texts.push(Text {
                at: Internal::of(pointer),
                of: Internal::of(text),
                words: Told(words.clone()),
            });
        }
        Self {
            order: Internal::of(order),
            frame: Told(frame),
            texts,
            spot: named.then_some(Spot { at: order }),
        }
    }
}

/// The sections and the questions of a lesson, as the check shows them.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct Lesson {
    pub(super) sections: Vec<View>,
    pub(super) questions: Vec<View>,
}

impl Lesson {
    /// The first `upto` of `parts`; those `named` says can be named as a place carry a name.
    pub(super) fn of(parts: &[Part], upto: usize, named: impl Fn(&Part) -> bool) -> Self {
        let mut lesson = Self {
            sections: Vec::new(),
            questions: Vec::new(),
        };
        for (order, part) in parts.iter().take(upto).enumerate() {
            let view = View::of(order, part, named(part));
            if matches!(part.site, FindingSite::Section(_)) {
                lesson.sections.push(view);
            } else {
                lesson.questions.push(view);
            }
        }
        lesson
    }

    fn views(&self) -> impl Iterator<Item = &View> {
        self.sections.iter().chain(&self.questions)
    }

    /// Every text, in the order `said` writes them.
    pub(super) fn texts(&self) -> Texts<Whose> {
        let mut texts = Texts::new();
        self.add_to(&mut texts);
        texts
    }

    /// Adds every text to `texts`, in the order `said` writes them.
    pub(super) fn add_to(&self, texts: &mut Texts<Whose>) {
        for view in self.views() {
            for text in &view.texts {
                let (part, at) = (*view.order.value(), *text.of.value());
                texts.push(
                    Whose::Lesson { part, text: at },
                    &format!("lesson/{part}/{at}"),
                    &text.words.0,
                    Grain::Sentence,
                );
            }
        }
    }

    /// The parts that can be named, in the order `said` writes them.
    pub(super) fn spots(&self) -> Vec<Spot> {
        self.views().filter_map(|view| view.spot.clone()).collect()
    }

    /// The sections and the questions as the request writes them, given what each text stands as, in the order of `texts()`, and the names of `spots()`.
    pub(super) fn said(&self, rows: &[String], spots: &[Handle<Spot>]) -> (Vec<Value>, Vec<Value>) {
        let (mut next, mut spot) = (0, 0);
        let mut say = |view: &View| -> Value {
            let mut frame = view.frame.0.clone();
            for text in &view.texts {
                let said = rows.get(next).cloned().unwrap_or_default();
                next += 1;
                if let Some(slot) = frame.pointer_mut(text.at.value()) {
                    *slot = Value::String(said);
                }
            }
            if view.spot.is_some() {
                if let (Some(name), Some(fields)) = (spots.get(spot), frame.as_object_mut()) {
                    let mut named = Map::new();
                    named.insert("id".into(), json!(name.written()));
                    named.extend(std::mem::take(fields));
                    *fields = named;
                }
                spot += 1;
            }
            frame
        };
        let sections = self.sections.iter().map(&mut say).collect();
        let questions = self.questions.iter().map(&mut say).collect();
        (sections, questions)
    }
}
