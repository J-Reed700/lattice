//! Material as rows: its texts as sentence units, its citations, and the source sentences a citation may quote.

use super::copied_wording::{self, Wording, SAY_IT_YOURSELF};
use super::repeats;
use super::text_edits::{citation_entry, cited_sentence, taught_at};
use crate::generation::long_enough_quote;
use lattice_core::application::services::model_contract::edit::{Anchor, Change, Consistent, Made};
use lattice_core::application::services::model_contract::shape::{
    Authored, Fields, Form, Settled, Why,
};
use lattice_core::application::services::model_contract::shown::{
    Drawn, Handle, Kind, Picked, Showing, Shown,
};
use lattice_core::application::services::model_contract::units::{Grain, Unit, UnitRef, Units};
use lattice_core::shared::error::Result;
use lattice_core::shared::hash::sha256_prefix;
use lattice_core::shared::text::{same_text, sentences};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::collections::BTreeSet;

/// The lists of the material whose elements are parts, and whether a part of one is a question.
const LISTS: [(&str, bool); 2] = [("blocks", false), ("questions", true)];

/// A citation a part holds, by where it is in the material as shown.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct CitationRef {
    pub part: String,
    /// `None`: the part's own `sourceIndex` and `quote`.
    pub index: Option<usize>,
}

/// A citation of a part: its own pair, or an element of its `citations`.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Citation {
    at: CitationRef,
    source_index: Option<u64>,
    quote: String,
    covers: Vec<UnitRef>,
}

impl Kind for Citation {
    const PREFIX: &'static str = "c";
    const DRAWN: Drawn = Drawn::Itself;
    type Ref = CitationRef;

    fn reference(&self) -> CitationRef {
        self.at.clone()
    }
}

impl Citation {
    pub(crate) fn at(&self) -> &CitationRef {
        &self.at
    }

    pub(crate) fn source_index(&self) -> Option<u64> {
        self.source_index
    }

    pub(crate) fn quote(&self) -> &str {
        &self.quote
    }
}

/// A source sentence by its source and its text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct QuotableRef {
    source: u64,
    text: u64,
}

/// A sentence of a source's excerpt long enough to quote.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Quotable {
    source_index: u64,
    text: String,
}

impl Kind for Quotable {
    const PREFIX: &'static str = "q";
    const DRAWN: Drawn = Drawn::Itself;
    type Ref = QuotableRef;

    fn reference(&self) -> QuotableRef {
        QuotableRef {
            source: self.source_index,
            text: sha256_prefix(&self.text),
        }
    }
}

impl Quotable {
    pub(crate) fn source_index(&self) -> u64 {
        self.source_index
    }

    pub(crate) fn text(&self) -> &str {
        &self.text
    }
}

/// The quotable sentences of the excerpts a request may show.
#[derive(Debug, Clone, Default)]
pub(crate) struct Quotes(Vec<Quotable>);

impl Quotes {
    /// From source entries shaped `{"sourceIndex": n, "excerpt": "…", …}`: every sentence of an excerpt
    /// that `generation::long_enough_quote` passes, each distinct `(source, sentence)` once, in order.
    pub(crate) fn of<'a>(sources: impl IntoIterator<Item = &'a Value>) -> Self {
        let mut all: Vec<Quotable> = Vec::new();
        for source in sources {
            let (Some(source_index), Some(excerpt)) = (
                source.get("sourceIndex").and_then(Value::as_u64),
                source.get("excerpt").and_then(Value::as_str),
            ) else {
                continue;
            };
            for (from, to) in sentences(excerpt) {
                let Some(text) = excerpt.get(from..to).filter(|text| long_enough_quote(text))
                else {
                    continue;
                };
                if all
                    .iter()
                    .any(|held| held.source_index == source_index && held.text == text)
                {
                    continue;
                }
                all.push(Quotable {
                    source_index,
                    text: text.to_owned(),
                });
            }
        }
        Self(all)
    }

    pub(crate) fn all(&self) -> &[Quotable] {
        &self.0
    }

    pub(crate) fn get(&self, at: &QuotableRef) -> Option<&Quotable> {
        self.0.iter().find(|held| held.reference() == *at)
    }

    /// `excerpt` (of source `source_index`) with the bracketed handle of each quotable `shown` holds before
    /// its sentence, everything else as it is.
    pub(crate) fn marked(source_index: u64, excerpt: &str, shown: &Shown<'_, Quotable>) -> String {
        let mut out = String::new();
        let mut end = 0;
        for (from, to) in sentences(excerpt) {
            out.push_str(excerpt.get(end..from).unwrap_or_default());
            let text = excerpt.get(from..to).unwrap_or_default();
            let held = shown.iter().find(|(_, quotable)| {
                quotable.source_index == source_index && quotable.text == text
            });
            if let Some((handle, _)) = held {
                out.push_str(&handle.bracketed());
                out.push(' ');
            }
            out.push_str(text);
            end = to;
        }
        out.push_str(excerpt.get(end..).unwrap_or_default());
        out
    }
}

/// New text, and the source sentences it rests on.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Statement {
    pub text: String,
    pub from: Vec<Picked<Quotable>>,
}

impl Authored for Statement {
    fn form() -> Form {
        Form::new().text("text").cite::<Quotable>("from")
    }

    fn read(fields: &mut Fields<'_, '_>) -> Option<Self> {
        let text = fields.text("text")?;
        let from = fields.cite::<Quotable>("from");
        Some(Self { text, from })
    }
}

/// A citation for a sentence the material has.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Cite {
    pub sentence: Picked<Unit>,
    pub quote: Picked<Quotable>,
}

impl Authored for Cite {
    fn form() -> Form {
        Form::new()
            .point::<Unit>("sentence")
            .point::<Quotable>("quote")
    }

    fn read(fields: &mut Fields<'_, '_>) -> Option<Self> {
        let sentence = fields.point::<Unit>("sentence")?;
        let quote = fields.point::<Quotable>("quote")?;
        Some(Self { sentence, quote })
    }
}

/// A listed citation given another quote.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Recite {
    pub citation: Picked<Citation>,
    pub quote: Picked<Quotable>,
}

impl Authored for Recite {
    fn form() -> Form {
        Form::new()
            .point::<Citation>("citation")
            .point::<Quotable>("quote")
    }

    fn read(fields: &mut Fields<'_, '_>) -> Option<Self> {
        let citation = fields.point::<Citation>("citation")?;
        let quote = fields.point::<Quotable>("quote")?;
        Some(Self { citation, quote })
    }
}

/// The changes one answer entry makes to the material.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Edits {
    pub sentences: Consistent<Unit, Made<Statement>>,
    pub cite: Vec<Cite>,
    pub recite: Vec<Recite>,
    pub uncite: Vec<Picked<Citation>>,
}

impl Authored for Edits {
    fn form() -> Form {
        Form::new()
            .change_after::<Unit, Made<Statement>>("sentences")
            .list::<Cite>("cite")
            .list::<Recite>("recite")
            .cite::<Citation>("uncite")
    }

    fn read(fields: &mut Fields<'_, '_>) -> Option<Self> {
        let sentences = fields.change_after::<Unit, Made<Statement>>("sentences")?;
        let cite = fields.list::<Cite>("cite")?;
        let recite = fields.list::<Recite>("recite")?;
        let uncite = fields.cite::<Citation>("uncite");
        Some(Self {
            sentences,
            cite,
            recite,
            uncite,
        })
    }
}

impl Edits {
    /// No change at all.
    pub(crate) fn is_empty(&self) -> bool {
        self.sentences.change().is_empty()
            && self.cite.is_empty()
            && self.recite.is_empty()
            && self.uncite.is_empty()
    }
}

/// One text of the material: where it is, the part it is in, and its sentences.
#[derive(Debug, Clone)]
struct Field {
    pointer: String,
    part: String,
    units: Units,
}

/// A citation an entry brought, by what it stands on.
#[derive(Debug, Clone)]
enum Added {
    /// For a sentence the material had.
    Cite {
        unit: UnitRef,
        part: String,
        source: u64,
        quote: String,
    },
    /// For new text.
    Text {
        part: String,
        sentence: String,
        source: u64,
        quote: String,
    },
}

/// The material as shown, its fields, its citations, and what was taken.
#[derive(Debug, Clone)]
pub(crate) struct Rows {
    base: Value,
    fields: Vec<Field>,
    citations: Vec<Citation>,
    recited: Vec<(CitationRef, Option<(u64, String)>)>,
    added: Vec<Added>,
}

/// What `Rows::shown` showed.
pub(crate) struct ShownRows<'a> {
    units: Vec<Shown<'a, Unit>>,
    citations: Shown<'a, Citation>,
}

impl<'a> ShownRows<'a> {
    /// The citations as shown, in order.
    pub(crate) fn citations(&self) -> impl Iterator<Item = (&Handle<Citation>, &'a Citation)> + '_ {
        self.citations.iter()
    }
}

/// Source wording new text may not copy.
pub(crate) struct Held<'a> {
    pub wording: &'a Wording,
}

/// What an entry's changes did.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Took {
    /// The parts changed, by pointer; `""` for a text outside every part.
    pub parts: BTreeSet<String>,
    /// The sentences its changes wrote (`repeats::added`, each changed field before and after).
    pub written: Vec<String>,
}

/// Why an entry's changes were not made.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Left {
    /// It changes, or is anchored on, what an earlier entry changed.
    Met(String),
    /// What is wrong with it, each in words a correction tells the model.
    Refused(Vec<String>),
}

/// The pointer segment of an object key.
fn escaped(key: &str) -> String {
    key.replace('~', "~0").replace('/', "~1")
}

/// The pointer of every text of `value` that is a field, in document order.
fn string_fields(value: &Value, pointer: &str, out: &mut Vec<String>) {
    match value {
        Value::String(_) => out.push(pointer.to_owned()),
        Value::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                string_fields(item, &format!("{pointer}/{index}"), out);
            }
        }
        Value::Object(fields) => {
            for (key, item) in fields {
                if key == "citations"
                    || (matches!(key.as_str(), "kind" | "quote") && item.is_string())
                {
                    continue;
                }
                string_fields(item, &format!("{pointer}/{}", escaped(key)), out);
            }
        }
        _ => {}
    }
}

/// The pointer of every part of `material` and whether it is a question.
fn part_pointers(material: &Value) -> Vec<(String, bool)> {
    let mut parts = Vec::new();
    for (list, question) in LISTS {
        let held = material.get(list).and_then(Value::as_array);
        for index in 0..held.map_or(0, Vec::len) {
            parts.push((format!("/{list}/{index}"), question));
        }
    }
    parts
}

/// The part a text at `pointer` is in; empty for a text outside every part.
fn part_of(pointer: &str) -> String {
    let mut segments = pointer.split('/').skip(1);
    match (segments.next(), segments.next()) {
        (Some(list @ ("blocks" | "questions")), Some(index)) if index.parse::<usize>().is_ok() => {
            format!("/{list}/{index}")
        }
        _ => String::new(),
    }
}

/// The units a citation's `sentence` stands on in `part`, found as the app anchors a citation's sentence.
fn covered(
    fields: &[Field],
    part: &Value,
    part_pointer: &str,
    question: bool,
    sentence: &str,
) -> Vec<UnitRef> {
    let Some((path, _, span)) = cited_sentence(part, question, sentence) else {
        return Vec::new();
    };
    let pointer = format!("{part_pointer}/{path}");
    let Some(field) = fields.iter().find(|field| field.pointer == pointer) else {
        return Vec::new();
    };
    field
        .units
        .spans()
        .into_iter()
        .filter(|(_, range)| same_text::overlaps((range.start, range.end), span))
        .map(|(unit, _)| unit)
        .collect()
}

fn anchor_unit(anchor: &Anchor<Unit>) -> Option<&Picked<Unit>> {
    match anchor {
        Anchor::Start => None,
        Anchor::After(thing) => Some(thing),
    }
}

/// Every unit a change names, as written: replaced, anchors, removed, moved.
fn units_named(change: &Change<Unit, Made<Statement>>) -> Vec<&Picked<Unit>> {
    let mut named: Vec<&Picked<Unit>> = change.replace.iter().map(|(thing, _)| thing).collect();
    named.extend(
        change
            .insert
            .iter()
            .filter_map(|(anchor, _)| anchor_unit(anchor)),
    );
    named.extend(change.remove.iter());
    for (thing, anchor) in &change.moved {
        named.push(thing);
        named.extend(anchor_unit(anchor));
    }
    named
}

/// The new text of a statement, without the spaces and tabs around it.
fn trimmed(statement: &Statement) -> &str {
    statement.text.trim_matches([' ', '\t'])
}

fn push_once(problems: &mut Vec<String>, problem: String) {
    if !problems.contains(&problem) {
        problems.push(problem);
    }
}

impl Rows {
    pub(crate) fn of(material: &Value) -> Self {
        let mut pointers = Vec::new();
        string_fields(material, "", &mut pointers);
        let fields: Vec<Field> = pointers
            .into_iter()
            .filter_map(|pointer| {
                let text = material.pointer(&pointer)?.as_str()?;
                Some(Field {
                    part: part_of(&pointer),
                    units: Units::of_at(text, Grain::Sentence, &pointer),
                    pointer,
                })
            })
            .collect();
        let mut citations = Vec::new();
        for (part_pointer, question) in part_pointers(material) {
            let Some(part) = material.pointer(&part_pointer) else {
                continue;
            };
            let own_index = part.get("sourceIndex").filter(|index| !index.is_null());
            let own_quote = part
                .get("quote")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if own_index.is_some() || !own_quote.trim().is_empty() {
                citations.push(Citation {
                    at: CitationRef {
                        part: part_pointer.clone(),
                        index: None,
                    },
                    source_index: own_index.and_then(Value::as_u64),
                    quote: own_quote.to_owned(),
                    covers: Vec::new(),
                });
            }
            let listed = part.get("citations").and_then(Value::as_array);
            for (index, entry) in listed.into_iter().flatten().enumerate() {
                let sentence = entry
                    .get("sentence")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                citations.push(Citation {
                    at: CitationRef {
                        part: part_pointer.clone(),
                        index: Some(index),
                    },
                    source_index: entry.get("sourceIndex").and_then(Value::as_u64),
                    quote: entry
                        .get("quote")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_owned(),
                    covers: covered(&fields, part, &part_pointer, question, sentence),
                });
            }
        }
        Self {
            base: material.clone(),
            fields,
            citations,
            recited: Vec::new(),
            added: Vec::new(),
        }
    }

    /// Shows every unit, field by field in document order, then every citation.
    pub(crate) fn shown<'a>(&'a self, showing: &mut Showing<'_>) -> Result<ShownRows<'a>> {
        let mut units = Vec::with_capacity(self.fields.len());
        for field in &self.fields {
            units.push(field.units.shown(showing)?);
        }
        let citations = showing.show(&self.citations)?;
        Ok(ShownRows { units, citations })
    }

    /// The material as a request shows it.
    pub(crate) fn rendered(&self, shown: &ShownRows<'_>) -> Value {
        let mut out = self.base.clone();
        for (at, field) in self.fields.iter().enumerate() {
            let text = match shown.units.get(at) {
                Some(units) => field.units.marked(units),
                None => field.units.text(),
            };
            if let Some(slot) = out.pointer_mut(&field.pointer) {
                *slot = Value::String(text);
            }
        }
        for (part_pointer, _) in part_pointers(&self.base) {
            if let Some(Value::Object(map)) = out.pointer_mut(&part_pointer) {
                *map = self.part_rendered(&part_pointer, std::mem::take(map), shown);
            }
        }
        out
    }

    /// A part with its pointer first and its citations as handles.
    fn part_rendered(
        &self,
        part_pointer: &str,
        map: Map<String, Value>,
        shown: &ShownRows<'_>,
    ) -> Map<String, Value> {
        let has_index = map.contains_key("sourceIndex");
        let mut rebuilt = Map::new();
        rebuilt.insert("part".to_owned(), Value::String(part_pointer.to_owned()));
        let mut placed = false;
        for (key, value) in map {
            match key.clone().as_str() {
                "sourceIndex" | "quote" => {
                    let here = key == "sourceIndex" || !has_index;
                    if here && !placed {
                        placed = true;
                        rebuilt.insert(
                            "citation".to_owned(),
                            self.own_rendered(part_pointer, shown),
                        );
                    }
                }
                "citations" => {
                    let value = match value {
                        Value::Array(entries) => Value::Array(
                            entries
                                .into_iter()
                                .enumerate()
                                .map(|(index, entry)| {
                                    self.listed_rendered(part_pointer, index, entry, shown)
                                })
                                .collect(),
                        ),
                        other => other,
                    };
                    rebuilt.insert(key, value);
                }
                _ => {
                    rebuilt.insert(key, value);
                }
            }
        }
        rebuilt
    }

    fn own_rendered(&self, part_pointer: &str, shown: &ShownRows<'_>) -> Value {
        let at = CitationRef {
            part: part_pointer.to_owned(),
            index: None,
        };
        match shown.citations.iter().find(|(_, held)| held.at == at) {
            Some((handle, held)) => json!({
                "at": handle.written(),
                "sourceIndex": held.source_index,
                "quote": held.quote,
            }),
            None => Value::Null,
        }
    }

    fn listed_rendered(
        &self,
        part_pointer: &str,
        index: usize,
        entry: Value,
        shown: &ShownRows<'_>,
    ) -> Value {
        let at = CitationRef {
            part: part_pointer.to_owned(),
            index: Some(index),
        };
        let Some((handle, held)) = shown.citations.iter().find(|(_, held)| held.at == at) else {
            return entry;
        };
        let sentences: Vec<String> = held
            .covers
            .iter()
            .filter_map(|unit| {
                shown
                    .units
                    .iter()
                    .flat_map(|units| units.iter())
                    .find(|(_, shown_unit)| shown_unit.reference() == *unit)
                    .map(|(handle, _)| handle.written())
            })
            .collect();
        json!({
            "at": handle.written(),
            "sentences": sentences,
            "sourceIndex": held.source_index,
            "quote": held.quote,
        })
    }

    fn field_of(&self, unit: &UnitRef) -> Option<&Field> {
        self.fields.iter().find(|field| field.units.holds(*unit))
    }

    /// The part each unit of `edits` and each citation it names is in, each once.
    pub(crate) fn parts_of(&self, edits: &Edits) -> BTreeSet<String> {
        let mut parts = BTreeSet::new();
        let named = units_named(edits.sentences.change())
            .into_iter()
            .chain(edits.cite.iter().map(|cite| &cite.sentence));
        for unit in named {
            if let Some(field) = self.field_of(&unit.reference) {
                parts.insert(field.part.clone());
            }
        }
        let citations = edits
            .recite
            .iter()
            .map(|recite| &recite.citation)
            .chain(edits.uncite.iter());
        for citation in citations {
            parts.insert(citation.reference.part.clone());
        }
        parts
    }

    /// The text of the quotes `edits` brings for `part`: those of its cites, recites and sources of new text.
    fn brought<'q>(&self, edits: &Edits, quotes: &'q Quotes, part: &str) -> Vec<&'q str> {
        let change = edits.sentences.change();
        let mut brought: Vec<&Picked<Quotable>> = Vec::new();
        for cite in &edits.cite {
            if self
                .field_of(&cite.sentence.reference)
                .is_some_and(|field| field.part == part)
            {
                brought.push(&cite.quote);
            }
        }
        for recite in &edits.recite {
            if recite.citation.reference.part == part {
                brought.push(&recite.quote);
            }
        }
        let places = change
            .replace
            .iter()
            .map(|(at, with)| (Some(at), with))
            .chain(
                change
                    .insert
                    .iter()
                    .map(|(anchor, with)| (anchor_unit(anchor), with)),
            );
        for (place, with) in places {
            let here = place
                .and_then(|unit| self.field_of(&unit.reference))
                .is_some_and(|field| field.part == part);
            if here {
                brought.extend(with.0.from.iter());
            }
        }
        brought
            .into_iter()
            .filter_map(|picked| quotes.get(&picked.reference))
            .map(Quotable::text)
            .collect()
    }

    /// Makes one entry's changes, whole or not at all.
    pub(crate) fn take(
        &mut self,
        edits: &Edits,
        quotes: &Quotes,
        held: &Held<'_>,
    ) -> std::result::Result<Took, Left> {
        let change = edits.sentences.change();
        let current = self.value();
        let mut problems: Vec<String> = Vec::new();

        for (at, with) in &change.replace {
            if trimmed(&with.0).is_empty() {
                push_once(
                    &mut problems,
                    format!(
                        "The new text for {} is empty; list a sentence in remove to take it out.",
                        at.handle.written()
                    ),
                );
            }
        }
        for (anchor, with) in &change.insert {
            if let Anchor::After(after) = anchor {
                if trimmed(&with.0).is_empty() {
                    push_once(
                        &mut problems,
                        format!("The new text after {} is empty.", after.handle.written()),
                    );
                }
            }
        }

        let split = edits
            .sentences
            .clone()
            .map(|Made(statement)| trimmed(&statement).to_owned())
            .split_by(|unit| {
                self.fields
                    .iter()
                    .position(|field| field.units.holds(*unit))
            });
        let groups = match split {
            Ok(groups) => groups,
            Err(why) => {
                push_once(&mut problems, why);
                Vec::new()
            }
        };

        let teaches = |unit: &UnitRef| {
            self.field_of(unit)
                .is_some_and(|field| taught_at(&field.pointer))
        };
        let not_taught = |name: String| {
            format!("{name} is not in a block's body or a question's explanation or reasons; only those sentences take a citation.")
        };
        for cite in &edits.cite {
            if !teaches(&cite.sentence.reference) {
                push_once(&mut problems, not_taught(cite.sentence.handle.written()));
            }
        }
        for (at, with) in &change.replace {
            if !with.0.from.is_empty() && !teaches(&at.reference) {
                push_once(&mut problems, not_taught(at.handle.written()));
            }
        }
        for (anchor, with) in &change.insert {
            if let Anchor::After(after) = anchor {
                if !with.0.from.is_empty() && !teaches(&after.reference) {
                    push_once(&mut problems, not_taught(after.handle.written()));
                }
            }
        }

        for cite in &edits.cite {
            if change
                .remove
                .iter()
                .any(|removed| removed.reference == cite.sentence.reference)
            {
                push_once(
                    &mut problems,
                    format!(
                        "{} is removed and cannot be cited.",
                        cite.sentence.handle.written()
                    ),
                );
            }
        }

        let named: Vec<&Picked<Citation>> = edits
            .recite
            .iter()
            .map(|recite| &recite.citation)
            .chain(edits.uncite.iter())
            .collect();
        for (at, citation) in named.iter().enumerate() {
            let again = named
                .iter()
                .enumerate()
                .any(|(other, held)| other != at && held.reference == citation.reference);
            if again {
                push_once(
                    &mut problems,
                    format!("{} is changed more than once.", citation.handle.written()),
                );
            }
        }

        let quoted = edits
            .cite
            .iter()
            .map(|cite| &cite.quote)
            .chain(edits.recite.iter().map(|recite| &recite.quote))
            .chain(
                change
                    .replace
                    .iter()
                    .flat_map(|(_, with)| with.0.from.iter()),
            )
            .chain(
                change
                    .insert
                    .iter()
                    .flat_map(|(_, with)| with.0.from.iter()),
            );
        for quote in quoted {
            if quotes.get(&quote.reference).is_none() {
                push_once(
                    &mut problems,
                    format!(
                        "{} is not a sentence of the excerpts shown.",
                        quote.handle.written()
                    ),
                );
            }
        }

        let mut trials: Vec<(usize, Units)> = Vec::new();
        let mut met: Option<String> = None;
        for (at, part) in groups {
            let Some(field) = self.fields.get(at) else {
                continue;
            };
            let mut units = field.units.clone();
            match units.apply(part) {
                Settled::Answered(_) => trials.push((at, units)),
                Settled::Unanswered(Why::Contradicted(text)) => {
                    if met.is_none() {
                        met = Some(text);
                    }
                }
                Settled::Unanswered(_) => {}
            }
        }

        let counts = repeats::said(&[&current]);
        let names = units_named(change);
        for (at, units) in &trials {
            let Some(field) = self.fields.get(*at) else {
                continue;
            };
            let added = repeats::added(&field.units.text(), &units.text());
            let name = names
                .iter()
                .find(|unit| field.units.holds(unit.reference))
                .map(|unit| unit.handle.written())
                .unwrap_or_default();
            if repeats::says_at(&field.pointer) {
                let repeated = added
                    .iter()
                    .find(|sentence| counts.contains_key(&repeats::key(sentence)));
                if let Some(sentence) = repeated {
                    push_once(
                        &mut problems,
                        format!("The new text at {name} adds a sentence the lesson already has: \"{sentence}\". Reword it or leave it out."),
                    );
                }
            }
            if taught_at(&field.pointer) {
                let appended = self.brought(edits, quotes, &field.part);
                let wording = match current.pointer(&field.part) {
                    Some(part) => Wording::of_part(part, appended),
                    None => Wording::default(),
                };
                let copied = added.iter().find_map(|sentence| {
                    copied_wording::copies(sentence, &wording, held.wording)
                        .into_iter()
                        .next()
                });
                if let Some(copied_wording::Copied { sentence, words }) = copied {
                    push_once(
                        &mut problems,
                        format!("The new text at {name} writes the source's wording as the lesson's own sentence: \"{sentence}\" (the source's words are \"{words}\"). {SAY_IT_YOURSELF}"),
                    );
                }
            }
        }
        if !problems.is_empty() {
            return Err(Left::Refused(problems));
        }
        if let Some(text) = met {
            return Err(Left::Met(text));
        }
        let changed_before = edits
            .recite
            .iter()
            .map(|recite| &recite.citation)
            .chain(edits.uncite.iter())
            .find(|citation| {
                self.recited
                    .iter()
                    .any(|(done, _)| *done == citation.reference)
            });
        if let Some(citation) = changed_before {
            return Err(Left::Met(format!(
                "{} was already changed by an earlier edit.",
                citation.handle.written()
            )));
        }

        let parts = self.parts_of(edits);
        let mut written = Vec::new();
        for (at, units) in trials {
            if let Some(field) = self.fields.get_mut(at) {
                written.extend(repeats::added(&field.units.text(), &units.text()));
                field.units = units;
            }
        }
        for recite in &edits.recite {
            if let Some(quote) = quotes.get(&recite.quote.reference) {
                self.recited.push((
                    recite.citation.reference.clone(),
                    Some((quote.source_index, quote.text.clone())),
                ));
            }
        }
        for uncite in &edits.uncite {
            self.recited.push((uncite.reference.clone(), None));
        }
        for cite in &edits.cite {
            let part = self
                .field_of(&cite.sentence.reference)
                .map(|field| field.part.clone());
            if let (Some(part), Some(quote)) = (part, quotes.get(&cite.quote.reference)) {
                self.added.push(Added::Cite {
                    unit: cite.sentence.reference,
                    part,
                    source: quote.source_index,
                    quote: quote.text.clone(),
                });
            }
        }
        let places = change
            .replace
            .iter()
            .map(|(at, with)| (Some(at), with))
            .chain(
                change
                    .insert
                    .iter()
                    .map(|(anchor, with)| (anchor_unit(anchor), with)),
            );
        for (place, with) in places {
            let part = place
                .and_then(|unit| self.field_of(&unit.reference))
                .map(|field| field.part.clone());
            let Some(part) = part else {
                continue;
            };
            for from in &with.0.from {
                if let Some(quote) = quotes.get(&from.reference) {
                    self.added.push(Added::Text {
                        part: part.clone(),
                        sentence: trimmed(&with.0).to_owned(),
                        source: quote.source_index,
                        quote: quote.text.clone(),
                    });
                }
            }
        }
        Ok(Took { parts, written })
    }

    /// The material as it stands after the entries taken.
    pub(crate) fn value(&self) -> Value {
        let mut out = self.base.clone();
        for field in self.fields.iter().filter(|field| field.units.changed()) {
            if let Some(slot) = out.pointer_mut(&field.pointer) {
                *slot = Value::String(field.units.text());
            }
        }
        for (part_pointer, question) in part_pointers(&self.base) {
            let (own, listed) = self.citations_of(&out, &part_pointer, question);
            if let Some(Value::Object(map)) = out.pointer_mut(&part_pointer) {
                if let Some((index, quote)) = own {
                    map.insert("sourceIndex".to_owned(), index);
                    map.insert("quote".to_owned(), quote);
                }
                if let Some(list) = listed {
                    map.insert("citations".to_owned(), Value::Array(list));
                }
            }
        }
        out
    }

    /// A part's own pair and its listed citations as they now stand; `None` for what is as it was.
    fn citations_of(
        &self,
        out: &Value,
        part_pointer: &str,
        question: bool,
    ) -> (Option<(Value, Value)>, Option<Vec<Value>>) {
        let own_at = CitationRef {
            part: part_pointer.to_owned(),
            index: None,
        };
        let own =
            self.recited.iter().find(|(done, _)| *done == own_at).map(
                |(_, recited)| match recited {
                    Some((source, quote)) => (json!(source), json!(quote)),
                    None => (Value::Null, json!("")),
                },
            );
        let entries = self
            .base
            .pointer(&format!("{part_pointer}/citations"))
            .and_then(Value::as_array);
        let mut list: Vec<Value> = Vec::new();
        for (index, entry) in entries.into_iter().flatten().enumerate() {
            let at = CitationRef {
                part: part_pointer.to_owned(),
                index: Some(index),
            };
            let recited = self.recited.iter().find(|(done, _)| *done == at);
            if let Some((_, None)) = recited {
                continue;
            }
            let mut entry = entry.clone();
            if let (Some((_, Some((source, quote)))), Some(map)) = (recited, entry.as_object_mut())
            {
                map.insert("sourceIndex".to_owned(), json!(source));
                map.insert("quote".to_owned(), json!(quote));
            }
            let covers = self
                .citations
                .iter()
                .find(|held| held.at == at)
                .map(|held| held.covers.as_slice())
                .unwrap_or_default();
            let field = covers.first().and_then(|first| self.field_of(first));
            if let Some(field) = field.filter(|field| field.units.touched(covers)) {
                match field.units.stands(covers) {
                    Some(sentence) => {
                        if let Some(map) = entry.as_object_mut() {
                            map.insert("sentence".to_owned(), Value::String(sentence));
                        }
                    }
                    None => continue,
                }
            }
            list.push(entry);
        }
        for added in &self.added {
            let (part, sentence, source, quote) = match added {
                Added::Cite {
                    unit,
                    part,
                    source,
                    quote,
                } => (
                    part,
                    self.field_of(unit)
                        .and_then(|field| field.units.stands(&[*unit])),
                    *source,
                    quote,
                ),
                Added::Text {
                    part,
                    sentence,
                    source,
                    quote,
                } => (part, Some(sentence.clone()), *source, quote),
            };
            let (Some(sentence), true) = (sentence, part == part_pointer) else {
                continue;
            };
            let Some(target) = out.pointer(part_pointer) else {
                continue;
            };
            if let Some(entry) = citation_entry(target, question, &sentence, json!(source), quote) {
                if !list.contains(&entry) {
                    list.push(entry);
                }
            }
        }
        let listed = (entries.is_some() || !list.is_empty()).then_some(list);
        (own, listed)
    }
}
