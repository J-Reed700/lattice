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
use lattice_core::application::services::model_contract::units::{
    Grain, Quoted, Texts, Unit, UnitRef, Units,
};
use lattice_core::shared::error::Result;
use lattice_core::shared::text::same_text;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::collections::BTreeSet;

mod keys;
mod located;
mod part;
mod rendering;
mod value;
pub(crate) mod words;

pub(crate) use keys::Rekey;

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
    #[cfg(test)]
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

/// The excerpts a request may show, and the sentences of them a citation may quote.
#[derive(Debug, Clone, Default)]
pub(crate) struct Quotes {
    texts: Texts<u64>,
    quotable: Vec<Quoted>,
}

/// Where the excerpt of a source stands among the texts of a request.
fn place_of(source_index: u64) -> String {
    source_index.to_string()
}

impl Quotes {
    /// From source entries shaped `{"sourceIndex": n, "excerpt": "…", …}`.
    pub(crate) fn of<'a>(sources: impl IntoIterator<Item = &'a Value>) -> Self {
        let mut quotes = Self::default();
        for source in sources {
            if let (Some(source_index), Some(excerpt)) = (
                source.get("sourceIndex").and_then(Value::as_u64),
                source.get("excerpt").and_then(Value::as_str),
            ) {
                quotes.push(source_index, excerpt);
            }
        }
        quotes
    }

    /// Adds an excerpt of source `source_index`. One already held for that source is not added again.
    pub(crate) fn push(&mut self, source_index: u64, excerpt: &str) {
        let held = self
            .texts
            .iter()
            .any(|(of, units)| *of == source_index && units.text() == excerpt);
        if held {
            return;
        }
        self.texts.push(
            source_index,
            &place_of(source_index),
            excerpt,
            Grain::Sentence,
        );
        if let Some((_, units)) = self.texts.iter().last() {
            self.quotable.extend(
                units
                    .quoted()
                    .into_iter()
                    .filter(|sentence| long_enough_quote(sentence.text())),
            );
        }
    }

    /// The sentences a citation may quote, in the order added.
    pub(crate) fn all(&self) -> &[Quoted] {
        &self.quotable
    }

    /// The source a quotable sentence is of, and its words. `None` for a sentence that is not quotable.
    pub(crate) fn get(&self, at: UnitRef) -> Option<(u64, &str)> {
        if !self.quotable.iter().any(|held| held.reference() == at) {
            return None;
        }
        self.texts.found(at).map(|(source, text)| (*source, text))
    }

    /// `excerpt` (of source `source_index`) as it is, the bracketed handle of each quotable `shown` holds
    /// before its sentence.
    pub(crate) fn marked(
        &self,
        source_index: u64,
        excerpt: &str,
        shown: &Shown<'_, Quoted>,
    ) -> String {
        Units::of_at(excerpt, Grain::Sentence, &place_of(source_index)).marked(shown)
    }
}

/// New text, and the source sentences it rests on.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Statement {
    pub text: String,
    pub from: Vec<Picked<Quoted>>,
}

impl Authored for Statement {
    fn form() -> Form {
        Form::new().text("text").cite::<Quoted>("from")
    }

    fn read(fields: &mut Fields<'_, '_>) -> Option<Self> {
        let text = fields.text("text")?;
        let from = fields.cite::<Quoted>("from");
        Some(Self { text, from })
    }
}

/// A citation for a sentence the material has.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Cite {
    pub sentence: Picked<Unit>,
    pub quote: Picked<Quoted>,
}

impl Authored for Cite {
    fn form() -> Form {
        Form::new()
            .point::<Unit>("sentence")
            .point::<Quoted>("quote")
    }

    fn read(fields: &mut Fields<'_, '_>) -> Option<Self> {
        let sentence = fields.point::<Unit>("sentence")?;
        let quote = fields.point::<Quoted>("quote")?;
        Some(Self { sentence, quote })
    }
}

/// A listed citation given another quote.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Recite {
    pub citation: Picked<Citation>,
    pub quote: Picked<Quoted>,
}

impl Authored for Recite {
    fn form() -> Form {
        Form::new()
            .point::<Citation>("citation")
            .point::<Quoted>("quote")
    }

    fn read(fields: &mut Fields<'_, '_>) -> Option<Self> {
        let citation = fields.point::<Citation>("citation")?;
        let quote = fields.point::<Quoted>("quote")?;
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
    pub keys: Vec<Rekey>,
}

impl Authored for Edits {
    fn form() -> Form {
        Form::new()
            .change_after::<Unit, Made<Statement>>("sentences")
            .list::<Cite>("cite")
            .list::<Recite>("recite")
            .cite::<Citation>("uncite")
            .list::<Rekey>("keys")
    }

    fn read(fields: &mut Fields<'_, '_>) -> Option<Self> {
        let sentences = fields.change_after::<Unit, Made<Statement>>("sentences")?;
        let cite = fields.list::<Cite>("cite")?;
        let recite = fields.list::<Recite>("recite")?;
        let uncite = fields.cite::<Citation>("uncite");
        let keys = fields.list::<Rekey>("keys")?;
        Some(Self {
            sentences,
            cite,
            recite,
            uncite,
            keys,
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
            && self.keys.is_empty()
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
    /// A question's part and the option its key names now.
    keyed: Vec<(String, usize)>,
    /// The one part these rows are of, when they are of a part.
    only: Option<String>,
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
        Self::build(material, None)
    }

    fn build(material: &Value, only: Option<&str>) -> Self {
        let mut pointers = Vec::new();
        string_fields(material, "", &mut pointers);
        let fields: Vec<Field> = pointers
            .into_iter()
            .filter_map(|pointer| {
                let text = material.pointer(&pointer)?.as_str()?;
                if only.is_some_and(|only| part_of(&pointer) != only) {
                    return None;
                }
                Some(Field {
                    part: part_of(&pointer),
                    units: Units::of_at(text, Grain::Sentence, &pointer),
                    pointer,
                })
            })
            .collect();
        let mut citations = Vec::new();
        for (part_pointer, question) in part_pointers(material) {
            if only.is_some_and(|only| part_pointer != only) {
                continue;
            }
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
            keyed: Vec::new(),
            only: only.map(str::to_owned),
        }
    }

    fn field_of(&self, unit: &UnitRef) -> Option<&Field> {
        self.fields.iter().find(|field| field.units.holds(*unit))
    }

    /// The part each unit of `edits` and each citation it names is in, each once.
    pub(crate) fn parts_of(&self, edits: &Edits) -> BTreeSet<String> {
        let mut parts = BTreeSet::new();
        let named = units_named(edits.sentences.change())
            .into_iter()
            .chain(edits.cite.iter().map(|cite| &cite.sentence))
            .chain(edits.keys.iter().map(|key| &key.right));
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
        let mut brought: Vec<&Picked<Quoted>> = Vec::new();
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
            .filter_map(|picked| quotes.get(picked.reference))
            .map(|(_, text)| text)
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
            if quotes.get(quote.reference).is_none() {
                push_once(
                    &mut problems,
                    format!(
                        "{} is not a sentence of the excerpts shown.",
                        quote.handle.written()
                    ),
                );
            }
        }

        let moves = self.key_moves(&edits.keys, &current, &mut problems);

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
        written.extend(self.move_keys(moves));
        for recite in &edits.recite {
            if let Some((source, text)) = quotes.get(recite.quote.reference) {
                self.recited.push((
                    recite.citation.reference.clone(),
                    Some((source, text.to_owned())),
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
            if let (Some(part), Some((source, text))) = (part, quotes.get(cite.quote.reference)) {
                self.added.push(Added::Cite {
                    unit: cite.sentence.reference,
                    part,
                    source,
                    quote: text.to_owned(),
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
                if let Some((source, text)) = quotes.get(from.reference) {
                    self.added.push(Added::Text {
                        part: part.clone(),
                        sentence: trimmed(&with.0).to_owned(),
                        source,
                        quote: text.to_owned(),
                    });
                }
            }
        }
        Ok(Took { parts, written })
    }
}
