//! The sentences of a lesson and of what the learner knows, as the check
//! shows them: each under a name the model points at, and the app reads the
//! words from its own copy.
use super::Part;
use crate::review_findings::FindingSite;
use lattice_core::application::services::model_contract::shown::{
    Drawn, Handle, Internal, Kind, Told,
};
use lattice_core::shared::text::sentences;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

/// Where a shown sentence stands.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) enum Place {
    /// The `index`th sentence of the `text`th text of the `part`th part, in the order the learner meets the parts.
    Lesson {
        part: usize,
        text: usize,
        index: usize,
    },
    /// The `given`th thing the `lesson`th earlier lesson gives.
    Earlier { lesson: usize, given: usize },
    /// The `index`th sentence of the learner's prior knowledge.
    Prior { index: usize },
}

/// One sentence the model may cite.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct Sentence {
    pub(super) place: Place,
    pub(super) words: Told<String>,
}

impl Sentence {
    pub(super) fn new(place: Place, words: &str) -> Self {
        Self {
            place,
            words: Told(words.to_owned()),
        }
    }
}

impl Kind for Sentence {
    const PREFIX: &'static str = "u";
    const DRAWN: Drawn = Drawn::Itself;
    type Ref = Place;

    fn reference(&self) -> Place {
        self.place.clone()
    }
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

/// The byte spans of the sentences of `text`; a text the segmenter does not split is one sentence.
pub(super) fn spans(text: &str) -> Vec<(usize, usize)> {
    let found = sentences(text);
    if !found.is_empty() {
        return found;
    }
    let start = text.len() - text.trim_start().len();
    let end = text.trim_end().len();
    if start < end {
        vec![(start, end)]
    } else {
        Vec::new()
    }
}

/// `sentences` under the names `names` gives them, one to a line; plain when there are none.
pub(super) fn rows(names: Option<&[Handle<Sentence>]>, sentences: &[Sentence]) -> String {
    sentences
        .iter()
        .enumerate()
        .map(|(at, sentence)| match names.and_then(|names| names.get(at)) {
            Some(name) => format!("{} {}", name.bracketed(), sentence.words.0),
            None => sentence.words.0.clone(),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// One text of a part, cut into sentences.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct Text {
    pub(super) at: Internal<String>,
    pub(super) sentences: Vec<Sentence>,
}

/// A section or question as the request shows it: its fields but for the texts, which stand as sentences.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct View {
    pub(super) frame: Told<Value>,
    pub(super) texts: Vec<Text>,
    pub(super) spot: Option<Spot>,
}

impl View {
    fn of(order: usize, part: &Part, named: bool) -> Self {
        let mut frame = part.shown.clone();
        if let Some(fields) = frame.as_object_mut() {
            fields.remove("id");
        }
        let mut texts = Vec::new();
        for (text, (field, words)) in part.fields.iter().zip(&part.texts).enumerate() {
            let pointer = format!("/{field}");
            let Some(slot) = frame.pointer_mut(&pointer) else {
                continue;
            };
            *slot = Value::Null;
            let sentences = spans(words)
                .into_iter()
                .filter_map(|(start, end)| words.get(start..end))
                .enumerate()
                .map(|(index, said)| {
                    Sentence::new(
                        Place::Lesson {
                            part: order,
                            text,
                            index,
                        },
                        said,
                    )
                })
                .collect();
            texts.push(Text {
                at: Internal::of(pointer),
                sentences,
            });
        }
        Self {
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

    /// Every sentence, in the order `said` writes them.
    pub(super) fn sentences(&self) -> Vec<Sentence> {
        self.views()
            .flat_map(|view| &view.texts)
            .flat_map(|text| text.sentences.iter().cloned())
            .collect()
    }

    /// The parts that can be named, in the order `said` writes them.
    pub(super) fn spots(&self) -> Vec<Spot> {
        self.views().filter_map(|view| view.spot.clone()).collect()
    }

    /// The sections and the questions as the request writes them, given the names of `sentences()` and of `spots()`.
    pub(super) fn said(
        &self,
        names: Option<&[Handle<Sentence>]>,
        spots: &[Handle<Spot>],
    ) -> (Vec<Value>, Vec<Value>) {
        let (mut next, mut spot) = (0, 0);
        let mut say = |view: &View| -> Value {
            let mut frame = view.frame.0.clone();
            for text in &view.texts {
                let held = names.map(|names| names.get(next..).unwrap_or_default());
                let said = rows(held, &text.sentences);
                next += text.sentences.len();
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
