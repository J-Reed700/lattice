use super::super::*;
use super::confine::Protected;
use crate::lessons::copied_wording::Wording;
use crate::lessons::rows::{self, Held, Left, Quotes, Rows};
use lattice_core::application::services::model_contract::ask::{Alone, Answered, Refusal, Says};
use lattice_core::application::services::model_contract::edit::{consistent, Anchor, Change};
use lattice_core::application::services::model_contract::packed::Packed;
use lattice_core::application::services::model_contract::shape::{Authored, Each, Fields, Form};
use lattice_core::application::services::model_contract::shown::{
    Drawn, Internal, Kind, Picked, Showing, Told,
};
use lattice_core::application::services::model_contract::units::{Unit, UnitRef};
use lattice_core::application::services::model_contract::{Contract, ContractId};
use std::collections::BTreeSet;

/// One failed claim as the model reads it, and the places of its evidence passages among the section's.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(in crate::lessons::content_verification) struct Claim {
    pub(super) row: Told<Value>,
    pub(super) evidence: Internal<Vec<usize>>,
}

/// The failed claims that stand on one sentence of the section, or one failed example: what an entry answers once.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(in crate::lessons::content_verification) struct Failed {
    pub(super) id: String,
    pub(super) claims: Vec<Claim>,
    pub(super) ids: Internal<Vec<String>>,
    /// The sentences the entry may change.
    pub(super) targets: Internal<Vec<Protected>>,
    /// The sentences of live claims it may not.
    pub(super) protected: Internal<Vec<Protected>>,
    /// The places of the passages shown with it; `None`: all it needs.
    pub(super) shown: Internal<Option<Vec<usize>>>,
    /// What running the failed example gave, for an entry that is one.
    pub(super) execution: Option<Told<Value>>,
}

impl Kind for Failed {
    const PREFIX: &'static str = "f";
    const DRAWN: Drawn = Drawn::Itself;
    type Ref = String;

    fn reference(&self) -> String {
        self.id.clone()
    }
}

impl Failed {
    /// The places of the evidence passages its claims name, each once, in recorded order.
    pub(super) fn needs(&self) -> Vec<usize> {
        let mut places: Vec<usize> = Vec::new();
        for place in self.claims.iter().flat_map(|claim| claim.evidence.value()) {
            if !places.contains(place) {
                places.push(*place);
            }
        }
        places
    }

    fn places(&self) -> Vec<usize> {
        self.shown.value().clone().unwrap_or_else(|| self.needs())
    }

    pub(super) fn claim_ids(&self) -> Vec<ClaimId> {
        self.ids.value().iter().cloned().map(ClaimId).collect()
    }
}

/// An evidence passage a failed claim was checked against.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(in crate::lessons::content_verification) struct Passage {
    pub(super) id: String,
    pub(super) title: Option<Told<String>>,
    pub(super) text: Told<String>,
    pub(super) source_index: Internal<Option<u64>>,
}

impl Kind for Passage {
    const PREFIX: &'static str = "e";
    const DRAWN: Drawn = Drawn::Itself;
    type Ref = String;

    fn reference(&self) -> String {
        self.id.clone()
    }
}

impl Passage {
    /// The passage as a source excerpt whose sentences a citation may quote.
    pub(super) fn excerpt(&self) -> Option<Value> {
        let index = (*self.source_index.value())?;
        Some(json!({"sourceIndex": index, "excerpt": self.text.0}))
    }
}

pub(in crate::lessons::content_verification) struct SectionRepair;

impl Contract for SectionRepair {
    const ID: ContractId = ContractId::new("section-repair");
    type Input = Input;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(in crate::lessons::content_verification) struct Input {
    /// The writer's prompt.
    pub(super) requirements: Told<Value>,
    /// The kind and title of each block of the lesson.
    pub(super) sections: Told<Vec<Value>>,
    /// The lesson with only the section at `part` in it, at its own place.
    pub(super) section: Told<Value>,
    /// The whole lesson, which a correction may not repeat.
    pub(super) lesson: Internal<Value>,
    pub(super) part: Internal<String>,
    /// The section is a question.
    pub(super) question: bool,
    /// The lesson's schema takes citations.
    pub(super) cites: bool,
    pub(super) items: Vec<Failed>,
    pub(super) passages: Packed<Passage>,
    /// What earlier corrections of the section replaced or removed.
    pub(super) history: Told<Vec<Value>>,
    pub(super) wording: Internal<Wording>,
    pub(super) output_tokens: usize,
}

/// What the model answers for one failed sentence.
#[derive(Debug, Clone, PartialEq)]
pub(in crate::lessons::content_verification) struct Correction {
    pub(super) edits: rows::Edits,
}

impl Authored for Correction {
    fn form() -> Form {
        Form::new().group::<rows::Edits>("edits")
    }

    fn read(fields: &mut Fields<'_, '_>) -> Option<Self> {
        Some(Self {
            edits: fields.group("edits")?,
        })
    }
}

pub(super) const NO_ROOM: &str =
    "A failed statement's evidence passage does not fit in this model's context window.";

pub(super) const NO_CHANGE: &str =
    "The correction made no change to the sentence it was asked to correct.";

/// The quotable sentences of the passages `input` holds.
pub(super) fn quotes_of(input: &Input) -> Quotes {
    let excerpts: Vec<Value> = input.passages.iter().filter_map(Passage::excerpt).collect();
    Quotes::of(excerpts.iter())
}

pub(super) fn units_of(rows: &Rows, part: &str, set: &[Protected]) -> Vec<UnitRef> {
    let mut units: Vec<UnitRef> = Vec::new();
    for held in set {
        for unit in rows.units_over(&format!("{part}/{}", held.field), &held.quote) {
            if !units.contains(&unit) {
                units.push(unit);
            }
        }
    }
    units
}

/// An entry's edits kept to the sentence it is for.
pub(super) struct Narrowed {
    /// What is left of the entry's edits; none when the part kept is refused.
    edits: Option<rows::Edits>,
    /// Why the entry is not taken at all: it changes a sentence of a live claim together with its own.
    pub(super) put_back: Option<String>,
}

/// `edits` without the changes to any sentence but `item`'s own.
pub(super) fn narrowed(rows: &Rows, part: &str, item: &Failed, edits: &rows::Edits) -> Narrowed {
    let own = units_of(rows, part, item.targets.value());
    let passed: Vec<(&str, Vec<UnitRef>)> = item
        .protected
        .value()
        .iter()
        .map(|held| {
            (
                held.quote.as_str(),
                rows.units_over(&format!("{part}/{}", held.field), &held.quote),
            )
        })
        .collect();
    let mut touched: Vec<&str> = Vec::new();
    let mut mine = |unit: &Picked<Unit>| -> bool {
        if !own.contains(&unit.reference) {
            return false;
        }
        for (quote, units) in &passed {
            if units.contains(&unit.reference) && !touched.contains(quote) {
                touched.push(*quote);
            }
        }
        true
    };
    let change = edits.sentences.change();
    let mut kept = Change {
        replace: Vec::new(),
        insert: Vec::new(),
        remove: Vec::new(),
        moved: Vec::new(),
    };
    for (at, with) in &change.replace {
        if mine(at) {
            kept.replace.push((at.clone(), with.clone()));
        }
    }
    for (anchor, with) in &change.insert {
        let stays = match anchor {
            Anchor::Start => false,
            Anchor::After(after) => {
                own.contains(&after.reference)
                    || rows
                        .next_of(after.reference)
                        .is_some_and(|next| own.contains(&next))
            }
        };
        if stays {
            kept.insert.push((anchor.clone(), with.clone()));
        }
    }
    for unit in &change.remove {
        if mine(unit) {
            kept.remove.push(unit.clone());
        }
    }
    for (unit, anchor) in &change.moved {
        if mine(unit) {
            kept.moved.push((unit.clone(), anchor.clone()));
        }
    }
    let put_back = (!touched.is_empty()).then(|| {
        format!(
            "The correction also changed a sentence it was not asked to change (\"{}\"), so it was put back; change only this sentence.",
            touched.join("\", \"")
        )
    });
    let for_its_sentence = |citation: &Picked<rows::Citation>| {
        let covered = rows.covered_by(&citation.reference);
        !covered.is_empty() && covered.iter().all(|unit| own.contains(unit))
    };
    let edits = consistent(kept).ok().map(|sentences| rows::Edits {
        sentences,
        cite: edits
            .cite
            .iter()
            .filter(|cite| own.contains(&cite.sentence.reference))
            .cloned()
            .collect(),
        recite: edits
            .recite
            .iter()
            .filter(|recite| for_its_sentence(&recite.citation))
            .cloned()
            .collect(),
        uncite: edits
            .uncite
            .iter()
            .filter(|citation| for_its_sentence(citation))
            .cloned()
            .collect(),
        keys: edits
            .keys
            .iter()
            .filter(|key| own.contains(&key.right.reference))
            .cloned()
            .collect(),
    });
    Narrowed { edits, put_back }
}

impl Narrowed {
    /// The edits that remain, when any do.
    pub(super) fn remaining(&self) -> Option<&rows::Edits> {
        self.edits.as_ref().filter(|edits| !edits.is_empty())
    }
}

fn touches_other_failed(
    rows: &Rows,
    part: &str,
    input: &Input,
    id: &str,
    own: &[UnitRef],
    edits: &rows::Edits,
) -> bool {
    let mut other_failed: BTreeSet<UnitRef> = BTreeSet::new();
    for item in &input.items {
        if item.id == id {
            continue;
        }
        for unit in units_of(rows, part, item.targets.value()) {
            if !own.contains(&unit) {
                other_failed.insert(unit);
            }
        }
    }
    if other_failed.is_empty() {
        return false;
    }
    let change = edits.sentences.change();
    change
        .replace
        .iter()
        .any(|(at, _)| other_failed.contains(&at.reference))
        || change
            .remove
            .iter()
            .any(|unit| other_failed.contains(&unit.reference))
        || change
            .moved
            .iter()
            .any(|(unit, _)| other_failed.contains(&unit.reference))
        || edits
            .cite
            .iter()
            .any(|cite| other_failed.contains(&cite.sentence.reference))
        || edits.recite.iter().any(|recite| {
            rows.covered_by(&recite.citation.reference)
                .into_iter()
                .any(|unit| other_failed.contains(&unit))
        })
        || edits.uncite.iter().any(|citation| {
            rows.covered_by(&citation.reference)
                .into_iter()
                .any(|unit| other_failed.contains(&unit))
        })
        || edits
            .keys
            .iter()
            .any(|key| other_failed.contains(&key.right.reference))
}

/// Why the entry for the failed sentence `id` (shown to the model as `entry`) is refused, when it is.
fn check(
    input: &Input,
    id: &str,
    entry: &str,
    correction: &Correction,
) -> std::result::Result<(), Refusal> {
    let Some(item) = input.items.iter().find(|item| item.id == id) else {
        return Ok(());
    };
    let mut rows = Rows::of_part(input.lesson.value(), input.part.value());
    let own = units_of(&rows, input.part.value(), item.targets.value());
    if correction.edits.is_empty() {
        if !own.is_empty() {
            return Err(Refusal(format!(
                "Your entry for {entry} made no edits to its failed sentence; edit or remove only that sentence."
            )));
        }
        return Ok(());
    }
    if touches_other_failed(
        &rows,
        input.part.value(),
        input,
        id,
        &own,
        &correction.edits,
    ) {
        return Err(Refusal(format!(
            "Your edits for {entry} also change another failed claim's sentence; put each failed claim's edits under its own entry id."
        )));
    }
    let narrowed = narrowed(&rows, input.part.value(), item, &correction.edits);
    let Some(remaining) = narrowed.remaining().filter(|_| narrowed.put_back.is_none()) else {
        return Ok(());
    };
    let held = Held {
        wording: input.wording.value(),
    };
    match rows.take(remaining, &quotes_of(input), &held) {
        Ok(_) => Ok(()),
        Err(Left::Refused(reasons)) => Err(Refusal(reasons.join(" "))),
        Err(Left::Met(words)) => Err(Refusal(words)),
    }
}

pub(super) const CONFINEMENT: &str =
    "Change only each failed claim's `sentence`; a change to any other sentence is discarded.";

pub(super) const ENTRIES: &str = "Answer with one entry for each failed sentence: a failed claim names its entry (entry) and the labels of its sentence (sentence), and an entry's edits correct that sentence for every failed claim that names the entry.";

impl Answered for SectionRepair {
    type Answer = Each<Failed, Correction>;
    const ALONE: Alone = Alone::Pending;

    fn within(input: &Input, room: usize) -> Option<Input> {
        let needs: Vec<Vec<usize>> = input.items.iter().map(Failed::needs).collect();
        let items = input
            .passages
            .for_owners(room, &needs)
            .into_iter()
            .filter_map(|(owner, places)| {
                let item = input.items.get(owner)?;
                let needs = needs.get(owner)?;
                if !needs.is_empty() && places.is_empty() {
                    return None;
                }
                if places.len() < needs.len() {
                    tracing::warn!(
                        kept = places.len(),
                        recorded = needs.len(),
                        "A failed claim's evidence exceeds the repair budget; sending the passages that fit"
                    );
                }
                Some(Failed {
                    shown: Internal::of(Some(places)),
                    ..item.clone()
                })
            })
            .collect();
        Some(Input {
            items,
            ..input.clone()
        })
    }

    fn says(input: &Input, showing: &mut Showing<'_>) -> Result<Says> {
        let items = showing.show(&input.items)?;
        let places: BTreeSet<usize> = input.items.iter().flat_map(Failed::places).collect();
        let held: Vec<(usize, &Passage)> = input
            .passages
            .iter()
            .enumerate()
            .filter(|(place, _)| places.contains(place))
            .collect();
        let shown_passages: Vec<Passage> =
            held.iter().map(|(_, passage)| (*passage).clone()).collect();
        let passages = showing.show(&shown_passages)?;
        let handles: Vec<(usize, String)> = held
            .iter()
            .zip(passages.iter())
            .map(|((place, _), (handle, _))| (*place, handle.written()))
            .collect();
        let excerpts: Vec<Value> = shown_passages.iter().filter_map(Passage::excerpt).collect();
        let quotes = Quotes::of(excerpts.iter());
        let quotables = showing.show(quotes.all())?;
        let part = input.part.value();
        let rows = Rows::of_part(&input.section.0, part);
        let shown = rows.shown(showing)?;
        let mut claims: Vec<Value> = Vec::new();
        for (handle, item) in items.iter() {
            let labels: Vec<String> = units_of(&rows, part, item.targets.value())
                .into_iter()
                .filter_map(|unit| shown.handle_of(unit))
                .map(|handle| handle.written())
                .collect();
            let shown_places = item.places();
            for claim in &item.claims {
                let evidence: Vec<&str> = claim
                    .evidence
                    .value()
                    .iter()
                    .filter(|place| shown_places.contains(place))
                    .filter_map(|place| {
                        handles
                            .iter()
                            .find(|(held, _)| held == place)
                            .map(|(_, written)| written.as_str())
                    })
                    .collect();
                let mut row = claim.row.0.clone();
                if let Some(fields) = row.as_object_mut() {
                    fields.insert("entry".into(), json!(handle.written()));
                    fields.insert("sentence".into(), json!(labels));
                    fields.insert("evidence".into(), json!(evidence));
                }
                claims.push(row);
            }
            if item.claims.is_empty() {
                claims.push(json!({
                    "entry": handle.written(),
                    "sentence": labels,
                    "execution": item.execution,
                }));
            }
        }
        let table: Vec<Value> = passages
            .iter()
            .map(|(handle, passage)| {
                let text = match *passage.source_index.value() {
                    Some(index) => quotes.marked(index, &passage.text.0, &quotables),
                    None => passage.text.0.clone(),
                };
                json!({"passage": handle.written(), "sourceTitle": passage.title, "text": text})
            })
            .collect();
        let section = super::shown_section(&rows, &shown, part);
        let candidate = if input.question {
            json!({"blocks": [], "questions": [section]})
        } else {
            json!({"blocks": [section], "questions": []})
        };
        let task = json!({
            "requirements": input.requirements.0,
            "lessonSections": input.sections.0,
            "candidate": candidate,
            "failedClaims": claims,
            "evidence": table,
            "repairHistory": input.history.0,
        });
        let mut says = Says::new(super::system(input.question, input.cites), task.to_string());
        says.output_tokens = input.output_tokens;
        Ok(says)
    }

    fn accept_for(
        input: &Input,
        of: &Picked<Failed>,
        correction: &Correction,
    ) -> std::result::Result<(), Refusal> {
        let entry = of.handle.written();
        check(input, &of.reference, &entry, correction)
    }
}
