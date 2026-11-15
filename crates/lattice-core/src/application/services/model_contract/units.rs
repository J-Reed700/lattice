//! Addressing text the app holds: text is pointed at, never copied.
//!
//! A part under work is shown as rows, one unit to a row: the label, then the
//! text. Only the part under work is addressable; text around it is shown
//! unlabelled, as context. `shared/text/segmentation` stays the single owner
//! of where a unit starts and ends.

use std::ops::Range;

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

type Laying = (String, Vec<(Option<UnitRef>, Range<usize>)>);

impl Units {
    pub fn of(text: &str, grain: Grain) -> Self {
        Self::cut(text, grain, sha256_prefix(text))
    }

    /// [`Units::of`] for a text that is one of several shown in one request: `place` says which (a JSON
    /// pointer, a field name), so units of equal texts in two places are not one thing.
    pub fn of_at(text: &str, grain: Grain, place: &str) -> Self {
        Self::cut(text, grain, sha256_prefix(format!("{place}\u{0}{text}")))
    }

    fn cut(text: &str, grain: Grain, of: u64) -> Self {
        let (pieces, gaps) = match grain {
            Grain::Sentence => Self::cut_sentences(text),
            Grain::Line => Self::cut_lines(text),
        };
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
        self.laid().0
    }

    /// Whether `unit` is one of this text's units as shown.
    pub fn holds(&self, unit: UnitRef) -> bool {
        self.text_of(unit).is_some()
    }

    /// Each unit as shown and the bytes of the text as shown that it takes, in order.
    pub fn spans(&self) -> Vec<(UnitRef, Range<usize>)> {
        let mut at = self.gaps.first().map_or(0, String::len);
        let mut spans = Vec::with_capacity(self.shown.len());
        for (index, unit) in self.shown.iter().enumerate() {
            let end = at.saturating_add(unit.text.len());
            spans.push((unit.at, at..end));
            let gap = self
                .gaps
                .get(index.saturating_add(1))
                .map_or(0, String::len);
            at = end.saturating_add(gap);
        }
        spans
    }

    /// The text as shown with each unit's bracketed handle and a space before it; everything between
    /// units as it is. A unit `shown` lacks is written without a handle.
    pub fn marked(&self, shown: &Shown<'_, Unit>) -> String {
        let mut out = self.gaps.first().cloned().unwrap_or_default();
        for (index, unit) in self.shown.iter().enumerate() {
            if let Some((handle, _)) = shown.iter().find(|(_, held)| held.at == unit.at) {
                out.push_str(&handle.bracketed());
                out.push(' ');
            }
            out.push_str(&unit.text);
            out.push_str(
                self.gaps
                    .get(index.saturating_add(1))
                    .map_or("", String::as_str),
            );
        }
        out
    }

    /// Whether an applied change replaced, removed or moved one of `units`, or anchored anything after one.
    pub fn touched(&self, units: &[UnitRef]) -> bool {
        units.iter().any(|unit| self.edited.touches(unit))
    }

    /// The text that now stands where `units` stood: from the first to the last place that holds one of
    /// them, kept or replaced, and whatever lies between. `None` when none of them is left.
    pub fn stands(&self, units: &[UnitRef]) -> Option<String> {
        let (out, lays) = self.laid();
        let mut from: Option<usize> = None;
        let mut to: Option<usize> = None;
        for (held, range) in &lays {
            if !held.is_some_and(|held| units.contains(&held)) {
                continue;
            }
            from = Some(from.map_or(range.start, |at| at.min(range.start)));
            to = Some(to.map_or(range.end, |at| at.max(range.end)));
        }
        out.get(from?..to?).map(str::to_owned)
    }

    /// The text as it stands, and for each place the shown unit it holds and its bytes of that text.
    fn laid(&self) -> Laying {
        // A text with no unit has one gap, its whole content.
        let (lead, tail) = match self.gaps.as_slice() {
            [lead, .., tail] => (lead.as_str(), tail.as_str()),
            [only] => (only.as_str(), ""),
            [] => ("", ""),
        };
        let mut out = String::from(lead);
        let mut lays = Vec::new();
        let mut previous: Option<(Option<usize>, bool)> = None;
        for place in self.edited.places() {
            let (held, text, wrote) = match place.laid {
                Laid::Kept(unit) => (Some(*unit), self.text_of(*unit).unwrap_or_default(), false),
                Laid::Wrote { content, replaces } => (replaces.copied(), content.as_str(), true),
            };
            if let Some((before, ended)) = previous {
                let begins = wrote && (text.starts_with('\n') || text.starts_with("\r\n"));
                if !begins && !ended {
                    out.push_str(self.between(before, place.at));
                }
            }
            let start = out.len();
            out.push_str(text);
            lays.push((held, start..out.len()));
            previous = Some((place.at, wrote && text.ends_with('\n')));
        }
        out.push_str(tail);
        (out, lays)
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
