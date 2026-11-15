//! A reply as the app keeps it: its text, and the references that resolved in it.
//!
//! The text holds each handle as written (`[s12]`). A message is saved as that
//! text and the places of its references, and everything that needs to know
//! what a reply cites reads those places, never the prose.

use std::ops::Range;

use lattice_core::application::contracts::sources::SourceDto;
use lattice_core::application::services::model_contract::kept::TurnNames;
use lattice_core::application::services::model_contract::reply::{
    self, Lines, Markers, Piece, Place, Reference,
};
use lattice_core::application::services::model_contract::shown::{Drawn, Kind};
use lattice_core::application::services::tool_handles::{FolderFile, Source};
use lattice_core::domain::conversation::{ConversationMessage, MessageRole};
use lattice_core::shared::citation_markers::old_markers;
use lattice_core::shared::stored_references::{cut, stored_places};
use serde_json::{json, Value};

use super::dto::{LinesDto, ReplyPieceDto, ReplyReferenceDto};
use super::retrieval::source_names::name_sources;
use super::TurnSource;

#[cfg(test)]
mod tests;

/// A resolved marker and where it sits in the reply's text, in bytes.
#[derive(Debug, Clone, PartialEq)]
pub struct Placed {
    pub range: Range<usize>,
    pub reference: Reference,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ReplyText {
    pub text: String,
    pub references: Vec<Placed>,
}

/// What a reply may point at: sources, and what the turn's source adds.
pub fn markers(source: Option<&TurnSource>) -> Markers {
    let markers = Markers::new().of::<Source>(Place::Inline);
    match source {
        Some(source) => source.reply_markers(markers),
        None => markers,
    }
}

impl ReplyText {
    /// A reply written in this turn, read against the turn's names.
    pub fn read(names: &TurnNames, markers: Markers, text: &str) -> Self {
        let pieces = names.with(|scope| reply::split(scope, markers, text));
        Self::of_pieces(pieces)
    }

    /// A reply saved with its references; `None` for a message saved before them.
    pub fn stored(content: &str, metadata: &Value) -> Option<Self> {
        let references = stored_places(content, metadata)?
            .into_iter()
            .filter_map(|place| Placed::stored(place.range, place.entry))
            .collect();
        Some(Self {
            text: content.to_owned(),
            references,
        })
    }

    /// A reply saved before references existed: its `[n]` and `[n, m]` named the
    /// sources of that message, and are read here once, as that message's own
    /// sources under their numbers in the conversation. A number that names no
    /// source of the message stays text, and code is left alone.
    pub fn legacy(
        names: &TurnNames,
        source: Option<&TurnSource>,
        content: &str,
        sources: &[SourceDto],
    ) -> Self {
        let mut named = sources.to_vec();
        name_sources(names, &mut named);
        let numbers: Vec<(u32, u32)> = sources
            .iter()
            .zip(&named)
            .filter_map(|(old, new)| Some((old.citation_id?, new.citation_id?)))
            .collect();
        let new_number = |old: u32| {
            let mut found = numbers
                .iter()
                .filter(|(was, _)| *was == old)
                .map(|(_, now)| *now);
            let first = found.next()?;
            found.next().is_none().then_some(first)
        };
        let rewritten = rewrite_old_markers(content, &new_number);
        let rewritten = match source {
            Some(source) => source.old_references(rewritten),
            None => rewritten,
        };
        Self::read(names, markers(source), &rewritten)
    }

    /// The numbers of the sources it cites, in order, repeats kept.
    pub fn cited(&self) -> Vec<u32> {
        self.cited_in(0..self.text.len())
    }

    /// The numbers of the sources whose marker lies in `range` of the text.
    pub fn cited_in(&self, range: Range<usize>) -> Vec<u32> {
        self.within(&range)
            .filter(|placed| placed.reference.prefix == Source::PREFIX)
            .map(|placed| placed.reference.number)
            .collect()
    }

    /// The text with every resolved marker, and the spaces before it, taken out.
    pub fn without_references(&self) -> String {
        self.without_references_in(0..self.text.len())
    }

    /// `range` of the text, as [`without_references`](Self::without_references) reads it.
    pub fn without_references_in(&self, range: Range<usize>) -> String {
        let places: Vec<Range<usize>> = self
            .references
            .iter()
            .map(|placed| placed.range.clone())
            .collect();
        cut(&self.text, range, &places).into_owned()
    }

    /// The reply as the frontend draws it: text between references, in order.
    pub fn pieces(&self) -> Vec<ReplyPieceDto> {
        let mut pieces = Vec::new();
        let mut at = 0;
        for placed in &self.references {
            push_text(&mut pieces, self.text.get(at..placed.range.start));
            pieces.push(ReplyPieceDto::Reference {
                reference: wire(&placed.reference),
            });
            at = placed.range.end;
        }
        push_text(&mut pieces, self.text.get(at..));
        pieces
    }

    /// What is stored under `metadata.references`.
    pub fn metadata(&self) -> Value {
        Value::Array(
            self.references
                .iter()
                .map(|placed| {
                    json!({
                        "start": placed.range.start,
                        "end": placed.range.end,
                        "prefix": placed.reference.prefix,
                        "number": placed.reference.number,
                        "lines": placed.reference.lines,
                        "reference": placed.reference.reference,
                    })
                })
                .collect(),
        )
    }

    fn within<'a>(&'a self, range: &'a Range<usize>) -> impl Iterator<Item = &'a Placed> + 'a {
        self.references
            .iter()
            .filter(|placed| placed.range.start >= range.start && placed.range.end <= range.end)
    }

    fn of_pieces(pieces: Vec<Piece>) -> Self {
        let mut text = String::new();
        let mut references = Vec::new();
        for piece in pieces {
            match piece {
                Piece::Text(chunk) => text.push_str(&chunk),
                Piece::Reference(reference) => {
                    let start = text.len();
                    text.push_str(&written(&reference));
                    references.push(Placed {
                        range: start..text.len(),
                        reference,
                    });
                }
            }
        }
        Self { text, references }
    }
}

impl Placed {
    fn stored(range: Range<usize>, entry: &Value) -> Option<Self> {
        let (prefix, drawn) = known(entry.get("prefix")?.as_str()?)?;
        let number = u32::try_from(entry.get("number")?.as_u64()?).ok()?;
        let lines = match entry.get("lines") {
            Some(lines) if !lines.is_null() => {
                Some(serde_json::from_value::<Lines>(lines.clone()).ok()?)
            }
            _ => None,
        };
        let reference = entry.get("reference").cloned().unwrap_or(Value::Null);
        Some(Self {
            range,
            reference: Reference {
                prefix,
                number,
                reference,
                drawn,
                lines,
            },
        })
    }
}

fn written(reference: &Reference) -> String {
    reply::written(&[Piece::Reference(reference.clone())])
}

fn push_text(pieces: &mut Vec<ReplyPieceDto>, text: Option<&str>) {
    if let Some(text) = text.filter(|text| !text.is_empty()) {
        pieces.push(ReplyPieceDto::Text {
            text: text.to_owned(),
        });
    }
}

/// The kinds a reply points at: prefix, how it is drawn, and the kind the wire names it.
const KINDS: [(&str, Drawn, &str); 2] = [
    (Source::PREFIX, Source::DRAWN, "source"),
    (FolderFile::PREFIX, FolderFile::DRAWN, "file"),
];

/// The kinds a saved reference may name, with their prefix and how they are drawn.
fn known(prefix: &str) -> Option<(&'static str, Drawn)> {
    KINDS
        .into_iter()
        .find(|(known, _, _)| *known == prefix)
        .map(|(prefix, drawn, _)| (prefix, drawn))
}

/// A reference as the frontend draws it.
pub fn wire(reference: &Reference) -> ReplyReferenceDto {
    let kind = KINDS
        .into_iter()
        .find(|(prefix, _, _)| *prefix == reference.prefix)
        .map_or("source", |(_, _, kind)| kind);
    let name = match reference.drawn {
        Drawn::Name => reference.reference.as_str().map(str::to_owned),
        Drawn::Number | Drawn::Itself => None,
    };
    ReplyReferenceDto {
        written: written(reference),
        kind: kind.to_owned(),
        number: reference.number,
        lines: reference
            .lines
            .map(|Lines { first, last }| LinesDto { first, last }),
        name,
    }
}

/// `content` with each old `[n]` and `[n, m]` that names a source written as
/// the handle of that source. A number `new_number` does not know stays as it was.
fn rewrite_old_markers(content: &str, new_number: &dyn Fn(u32) -> Option<u32>) -> String {
    let mut out = String::with_capacity(content.len());
    let mut at = 0;
    for marker in old_markers(content) {
        out.push_str(content.get(at..marker.range.start).unwrap_or_default());
        let written = content.get(marker.range.clone()).unwrap_or_default();
        out.push_str(&handles(written, &marker.labels, new_number));
        at = marker.range.end;
    }
    out.push_str(content.get(at..).unwrap_or_default());
    out
}

/// Each label that names a source as that source's handle, the others as they were
/// written; the marker as written when none names one.
fn handles(written: &str, labels: &[&str], new_number: &dyn Fn(u32) -> Option<u32>) -> String {
    let parts: Vec<(&str, Option<u32>)> = labels
        .iter()
        .map(|label| (*label, label.parse::<u32>().ok().and_then(new_number)))
        .collect();
    if parts.iter().all(|(_, number)| number.is_none()) {
        return written.to_owned();
    }
    parts
        .iter()
        .map(|(label, number)| match number {
            Some(number) => format!("[{}{number}]", Source::PREFIX),
            None => format!("[{label}]"),
        })
        .collect()
}

/// The pieces of an assistant message saved with its references; `None` for any
/// other message, and for an assistant message saved before references.
pub fn saved_pieces(message: &ConversationMessage) -> Option<Vec<ReplyPieceDto>> {
    if !matches!(message.role, MessageRole::Assistant) {
        return None;
    }
    let metadata: Value = serde_json::from_str(message.metadata.as_deref()?).ok()?;
    ReplyText::stored(&message.content, &metadata).map(|reply| reply.pieces())
}
