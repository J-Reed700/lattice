//! Addressing text the app holds: text is pointed at, never copied.
//!
//! A part under work is shown as rows, one unit to a row: the label, then the
//! text. Only the part under work is addressable; text around it is shown
//! unlabelled, as context. `shared/text/segmentation` stays the single owner
//! of where a unit starts and ends.

use serde::{Deserialize, Serialize};

use super::edit::{Applied, Consistent, Edited, Laid};
use super::shape::Settled;
use super::shown::{Drawn, Kind, Showing, Shown};
use crate::shared::error::Result;
use crate::shared::hash::sha256_prefix;
use crate::shared::text::sentences;

#[cfg(test)]
mod tests;

/// The smallest thing an edit may change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Grain {
    /// The unit's content is the sentence; the spaces and paragraph breaks
    /// between units are the app's.
    Sentence,
    /// The unit's content is the whole line, indentation included; only the
    /// line break is the app's.
    Line,
}

/// One unit of a text: where it was when the text was shown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unit {
    at: UnitRef,
    text: String,
}

impl Unit {
    pub fn text(&self) -> &str {
        &self.text
    }
}

/// A unit's identity: the text it is a unit of, by that text's content, and
/// its place in the text as shown. Units of two texts are never one thing:
/// shown in one request they get handles of their own, and a change to one
/// text cannot be applied to the other.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnitRef {
    text: u64,
    index: usize,
}

impl UnitRef {
    /// The unit's place in the text as shown, counting from zero.
    pub fn index(&self) -> usize {
        self.index
    }
}

impl Kind for Unit {
    const PREFIX: &'static str = "u";
    const DRAWN: Drawn = Drawn::Itself;
    type Ref = UnitRef;

    fn reference(&self) -> UnitRef {
        self.at
    }
}

/// A text as units, with what the app keeps between them.
#[derive(Debug, Clone)]
pub struct Units {
    grain: Grain,
    shown: Vec<Unit>,
    /// The units and every change applied to them.
    edited: Edited<Unit, String>,
    /// What the app wrote before the first unit, between units and after the last, as shown.
    gaps: Vec<String>,
}

impl Units {
    pub fn of(text: &str, grain: Grain) -> Self {
        let (pieces, gaps) = match grain {
            Grain::Sentence => Self::cut_sentences(text),
            Grain::Line => Self::cut_lines(text),
        };
        let of = sha256_prefix(text);
        let shown: Vec<Unit> = pieces
            .into_iter()
            .enumerate()
            .map(|(index, text)| Unit {
                at: UnitRef { text: of, index },
                text,
            })
            .collect();
        let edited = Edited::of(shown.iter().map(|unit| unit.at).collect());
        Self {
            grain,
            shown,
            edited,
            gaps,
        }
    }

    /// The sentences of `text`, and what lies before, between and after them.
    fn cut_sentences(text: &str) -> (Vec<String>, Vec<String>) {
        let mut pieces = Vec::new();
        let mut gaps = Vec::new();
        let mut end = 0;
        for (start, stop) in sentences(text) {
            gaps.push(text.get(end..start).unwrap_or_default().to_owned());
            pieces.push(text.get(start..stop).unwrap_or_default().to_owned());
            end = stop;
        }
        gaps.push(text.get(end..).unwrap_or_default().to_owned());
        (pieces, gaps)
    }

    /// The lines of `text` without their line breaks, and the breaks as written.
    fn cut_lines(text: &str) -> (Vec<String>, Vec<String>) {
        let mut pieces = Vec::new();
        let mut gaps = vec![String::new()];
        for line in text.split_inclusive('\n') {
            let (content, ending) = match line.strip_suffix('\n') {
                Some(rest) => match rest.strip_suffix('\r') {
                    Some(bare) => (bare, "\r\n"),
                    None => (rest, "\n"),
                },
                None => (line, ""),
            };
            pieces.push(content.to_owned());
            gaps.push(ending.to_owned());
        }
        (pieces, gaps)
    }

    /// Shows every unit as pointable.
    pub fn shown<'a>(&'a self, showing: &mut Showing<'_>) -> Result<Shown<'a, Unit>> {
        showing.show(&self.shown)
    }

    /// The units as rows for a prompt, one to a row: the handle, then the text.
    pub fn rows(&self, shown: &Shown<'_, Unit>) -> String {
        shown
            .iter()
            .map(|(handle, unit)| {
                let text = match self.grain {
                    Grain::Sentence => unit.text.replace(['\r', '\n'], " "),
                    Grain::Line => unit.text.clone(),
                };
                format!("{} {text}", handle.bracketed())
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// The text of a shown unit: what a cite of it copies.
    pub fn text_of(&self, unit: UnitRef) -> Option<&str> {
        self.shown
            .get(unit.index)
            .filter(|shown| shown.at == unit)
            .map(Unit::text)
    }

    /// Applies a change whole, or not at all.
    pub fn apply(&mut self, change: Consistent<Unit, String>) -> Settled<Applied<Unit>> {
        self.edited.apply(change)
    }

    /// The text as it stands after the changes applied so far. New content is
    /// segmented again by `shared/text/segmentation`, never by a change.
    pub fn text(&self) -> String {
        // A text with no unit has one gap, its whole content.
        let (lead, tail) = match self.gaps.as_slice() {
            [lead, .., tail] => (lead.as_str(), tail.as_str()),
            [only] => (only.as_str(), ""),
            [] => ("", ""),
        };
        let mut out = String::from(lead);
        let mut previous: Option<Option<usize>> = None;
        for place in self.edited.places() {
            if let Some(before) = previous {
                out.push_str(self.between(before, place.at));
            }
            match place.laid {
                Laid::Kept(unit) => out.push_str(self.text_of(*unit).unwrap_or_default()),
                Laid::Wrote { content, .. } => out.push_str(content),
            }
            previous = Some(place.at);
        }
        out.push_str(tail);
        out
    }

    /// Whether any change has been applied.
    pub fn changed(&self) -> bool {
        self.edited.changed()
    }

    /// What the app writes between two places of the same shown unit, or
    /// where no gap of the text as shown lies between them.
    fn joiner(&self) -> &str {
        match self.grain {
            Grain::Sentence => " ",
            Grain::Line => self
                .gaps
                .iter()
                .map(String::as_str)
                .find(|gap| !gap.is_empty())
                .unwrap_or("\n"),
        }
    }

    /// What separates a place standing at `before` from the next, standing at
    /// `after`: the strongest of the gaps the text showed from just after
    /// `before` up to `after`, so a paragraph break beside a changed unit stays.
    fn between(&self, before: Option<usize>, after: Option<usize>) -> &str {
        let Some(after) = after else {
            return self.joiner();
        };
        if before == Some(after) {
            return self.joiner();
        }
        let first = before.map_or(1, |at| at.saturating_add(1));
        let mut strongest: Option<&str> = None;
        for gap in self.gaps.get(first..=after).unwrap_or_default() {
            let gap = gap.as_str();
            match strongest {
                Some(held) if breaks(gap) <= breaks(held) => {}
                _ => strongest = Some(gap),
            }
        }
        strongest.unwrap_or_else(|| self.joiner())
    }
}

/// How strong a gap is: its line feeds, then its length.
fn breaks(gap: &str) -> (usize, usize) {
    (gap.matches('\n').count(), gap.len())
}
