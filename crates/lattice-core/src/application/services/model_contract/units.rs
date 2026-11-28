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
use super::shown::{Drawn, Handle, Kind, Picked, Showing, Shown};
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

/// A unit of a text shown to be quoted: named apart from the units of the material a request works on,
/// so an answer cannot give one where the other is asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Quoted(Unit);

impl Quoted {
    pub fn text(&self) -> &str {
        &self.0.text
    }
}

impl Kind for Quoted {
    const PREFIX: &'static str = "q";
    const DRAWN: Drawn = Drawn::Itself;
    type Ref = UnitRef;

    fn reference(&self) -> UnitRef {
        self.0.at
    }
}

/// A kind a unit is shown as.
pub trait Held: Kind<Ref = UnitRef> {
    fn unit(&self) -> &Unit;
}

impl Held for Unit {
    fn unit(&self) -> &Unit {
        self
    }
}

impl Held for Quoted {
    fn unit(&self) -> &Unit {
        &self.0
    }
}

/// A unit's label in a row and in a marked text: its bracketed handle, then a space.
fn label<K>(handle: &Handle<K>) -> String {
    format!("{} ", handle.bracketed())
}

/// A replacement as text. One written as its row, the replaced unit's own label before it, is the text.
fn unlabelled(unit: &Picked<Unit>, with: String) -> String {
    let text = with.trim_start();
    let Some(rest) = text.strip_prefix(&label(&unit.handle)) else {
        return with;
    };
    let lead = with.strip_suffix(text).unwrap_or_default();
    format!("{lead}{rest}")
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

    /// The units as things to quote, in order.
    pub fn quoted(&self) -> Vec<Quoted> {
        self.shown.iter().cloned().map(Quoted).collect()
    }

    /// The units as rows for a prompt, one to a row: the handle, then the text.
    pub fn rows<K: Held>(&self, shown: &Shown<'_, K>) -> String {
        shown
            .iter()
            .map(|(handle, held)| {
                let unit = held.unit();
                let text = match self.grain {
                    Grain::Sentence => unit.text.replace(['\r', '\n'], " "),
                    Grain::Line => unit.text.clone(),
                };
                format!("{}{text}", label(handle))
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
        self.edited.apply(change.map_replaced(unlabelled))
    }

    /// The text as it stands after the changes applied so far. New content is
    /// segmented again by `shared/text/segmentation`, never by a change.
    pub fn text(&self) -> String {
        self.laid(&[]).0
    }

    /// The units as shown, in order.
    pub fn units(&self) -> &[Unit] {
        &self.shown
    }

    /// [`Units::text`] with the shown units at `positions` (their places in
    /// [`Units::units`]) left out.
    pub fn text_without(&self, positions: &[usize]) -> String {
        self.laid(positions).0
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
    pub fn marked<K: Held>(&self, shown: &Shown<'_, K>) -> String {
        let mut out = self.gaps.first().cloned().unwrap_or_default();
        for (index, unit) in self.shown.iter().enumerate() {
            if let Some((handle, _)) = shown.iter().find(|(_, held)| held.unit().at == unit.at) {
                out.push_str(&label(handle));
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
        let (out, lays) = self.laid(&[]);
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

    /// The text as it stands without the shown units at `positions`, and for each place the shown unit
    /// it holds and its bytes of that text.
    fn laid(&self, positions: &[usize]) -> Laying {
        // A text with no unit has one gap, its whole content.
        let (lead, tail) = match self.gaps.as_slice() {
            [lead, .., tail] => (lead.as_str(), tail.as_str()),
            [only] => (only.as_str(), ""),
            [] => ("", ""),
        };
        let mut out = String::from(lead);
        let mut lays = Vec::new();
        let mut previous: Option<(Option<usize>, bool, bool, bool)> = None;
        for place in self.edited.places() {
            let (held, text, wrote, inserted_item, wrote_block_end) = match place.laid {
                Laid::Kept(unit) if positions.contains(&unit.index) => continue,
                Laid::Kept(unit) => (
                    Some(*unit),
                    self.text_of(*unit).unwrap_or_default(),
                    false,
                    false,
                    false,
                ),
                Laid::Wrote { content, replaces } => (
                    replaces.copied(),
                    content.as_str(),
                    true,
                    replaces.is_none() && starts_list_item(content),
                    ends_markdown_block(content),
                ),
            };
            if let Some((before, ended, prev_inserted_item, prev_wrote_block_end)) = previous {
                let begins = wrote && (text.starts_with('\n') || text.starts_with("\r\n"));
                if !begins && !ended {
                    out.push_str(self.separator(
                        before,
                        place.at,
                        prev_inserted_item,
                        prev_wrote_block_end || (wrote && starts_markdown_block(text)),
                        text,
                    ));
                }
            }
            let start = out.len();
            out.push_str(text);
            lays.push((held, start..out.len()));
            previous = Some((
                place.at,
                wrote && text.ends_with('\n'),
                inserted_item,
                wrote_block_end,
            ));
        }
        out.push_str(tail);
        (out, lays)
    }

    /// Makes the text `text` whole, as one change that replaces every change applied before.
    pub fn rewrite(&mut self, text: &str) {
        self.edited.rewrite(text.to_owned());
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

    fn separator(
        &self,
        before: Option<usize>,
        after: Option<usize>,
        prev_inserted_item: bool,
        markdown_block_boundary: bool,
        next_text: &str,
    ) -> &str {
        if let Some(gap) = self.between(before, after) {
            if self.grain == Grain::Sentence && markdown_block_boundary && !gap.contains('\n') {
                return "\n\n";
            }
            return gap;
        }
        if self.grain == Grain::Sentence && markdown_block_boundary {
            return "\n\n";
        }
        if self.grain == Grain::Sentence && (prev_inserted_item || starts_list_item(next_text)) {
            return "\n";
        }
        self.joiner()
    }

    /// What separates a place standing at `before` from a later shown unit at
    /// `after` in the original text, when `before < after`.
    fn between(&self, before: Option<usize>, after: Option<usize>) -> Option<&str> {
        let before = before?;
        let after = after?;
        if before >= after {
            return None;
        }
        let first = before.saturating_add(1);
        let mut strongest: Option<&str> = None;
        for gap in self.gaps.get(first..=after).unwrap_or_default() {
            let gap = gap.as_str();
            match strongest {
                Some(held) if breaks(gap) <= breaks(held) => {}
                _ => strongest = Some(gap),
            }
        }
        strongest
    }
}

fn starts_list_item(text: &str) -> bool {
    let first = text.trim_start_matches(['\r', '\n']).lines().next();
    first.is_some_and(|line| crate::shared::text::item_marker_end(line).is_some())
}

fn is_markdown_block_line(line: &str) -> bool {
    let trimmed = line.trim_start();
    if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
        return true;
    }
    let hashes = trimmed.bytes().take_while(|&b| b == b'#').count();
    (1..=6).contains(&hashes)
        && trimmed
            .as_bytes()
            .get(hashes)
            .is_some_and(|&b| b == b' ' || b == b'\t')
}

fn starts_markdown_block(text: &str) -> bool {
    let first = text.trim_start_matches(['\r', '\n']).lines().next();
    first.is_some_and(is_markdown_block_line)
}

fn ends_markdown_block(text: &str) -> bool {
    let last = text.trim_end_matches(['\r', '\n']).lines().next_back();
    last.is_some_and(is_markdown_block_line)
}

/// Several texts shown in one request, each as units, and what each is a text of.
#[derive(Debug, Clone)]
pub struct Texts<P> {
    texts: Vec<(P, Units)>,
}

impl<P> Texts<P> {
    pub fn new() -> Self {
        Self { texts: Vec::new() }
    }

    /// Adds a text of `of`. `place` tells this text from an equal text elsewhere in the request.
    pub fn push(&mut self, of: P, place: &str, text: &str, grain: Grain) {
        self.texts.push((of, Units::of_at(text, grain, place)));
    }

    /// Each text and what it is a text of, in the order added.
    pub fn iter(&self) -> impl Iterator<Item = (&P, &Units)> {
        self.texts.iter().map(|(of, units)| (of, units))
    }

    /// What `unit` is a unit of, and its words. `None` when no text here holds it.
    pub fn found(&self, unit: UnitRef) -> Option<(&P, &str)> {
        self.texts
            .iter()
            .find_map(|(of, units)| units.text_of(unit).map(|text| (of, text)))
    }
}

impl<P> Default for Texts<P> {
    fn default() -> Self {
        Self::new()
    }
}

/// How strong a gap is: its line feeds, then its length.
fn breaks(gap: &str) -> (usize, usize) {
    (gap.matches('\n').count(), gap.len())
}
