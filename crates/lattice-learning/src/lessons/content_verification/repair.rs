//! Rewrites only affected sections, one request for each section's failed
//! sentences, every section durable. A claim a page of the course supported
//! elsewhere needs only its citation rewritten (`citation::cite_where_supported`),
//! with no model call.
use super::claim_ledger::ClaimLedger;
use super::repair_work::{
    claim_ids, context_claims, drop_group, ledger_targets, section_examples, unmade,
    withdraw_resolved,
};
use super::*;
use crate::dto::LearningSourceDto;
use crate::lessons::copied_wording::Wording;
use crate::lessons::rows::{self, words, Held, Left, Quotes, Rows};
use crate::lessons::shape::LessonShape;
use crate::lessons::structured_call::{refusal, Asking};
use crate::lessons::text_edits::{
    self, NEVER_SAY_WHERE, QUOTED_WORDING, STEPS_IN_ORDER, UNQUOTED_REMARKS,
};
use crate::lessons::trace::{self, Cause, Stage};
use crate::review_findings::FindingSite;
use lattice_core::application::services::model_contract::ask::{
    self, ask_each, Alone, Answered, Refusal, Says, Settled,
};
use lattice_core::application::services::model_contract::edit::{consistent, Anchor, Change};
use lattice_core::application::services::model_contract::packed::Packed;
use lattice_core::application::services::model_contract::shape::{
    Authored, Each, Fields, Form, Why,
};
use lattice_core::application::services::model_contract::shown::{
    Drawn, Internal, Kind, Picked, Scope, Showing, Told,
};
use lattice_core::application::services::model_contract::units::{Unit, UnitRef};
use lattice_core::application::services::model_contract::{Contract, ContractId};
use lattice_core::domain::learning::acceptance::{AcceptancePolicy, Goal};
use std::collections::BTreeSet;

mod citation;
mod confine;
mod cut;
mod taking;
pub(super) use citation::{cite_where_supported, number_evidence, uncite, Cited};
#[cfg(test)]
pub(super) use citation::{quotation, Unwritten};
pub(crate) use confine::{confine, Protected};
#[cfg(test)]
pub(super) use cut::last_attempts;
use taking::{section_input, Taking};

fn section_of(candidate: &Value, unit: usize) -> Result<(&'static str, usize)> {
    FindingSite::in_candidate(candidate, unit)
        .array()
        .ok_or_else(|| invalid("Repair referred to an unknown lesson section."))
}

fn observations(context: &Value) -> Result<Vec<execution::Observation>> {
    Ok(serde_json::from_value(
        context
            .get("executions")
            .cloned()
            .unwrap_or_else(|| json!([])),
    )?)
}

fn targets(context: &Value) -> Result<BTreeSet<usize>> {
    let mut targets = BTreeSet::new();
    for finding in context
        .get("failedClaims")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        targets.insert(
            finding
                .get("unit")
                .and_then(Value::as_u64)
                .ok_or_else(|| invalid("A repair finding has no section."))? as usize,
        );
    }
    for example in context
        .get("exampleClaims")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        targets.insert(
            example
                .get("unit")
                .and_then(Value::as_u64)
                .ok_or_else(|| invalid("A failed example has no repairable section."))?
                as usize,
        );
    }
    Ok(targets)
}

/// The failed claims recorded for `unit`, in recorded order.
fn section_failures(context: &Value, unit: usize) -> Vec<Value> {
    context
        .get("failedClaims")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|finding| finding.get("unit").and_then(Value::as_u64) == Some(unit as u64))
        .cloned()
        .collect()
}

/// A failed claim's evidence IDs in recorded order, the cited one first.
fn evidence_ids(failure: &Value) -> Vec<&str> {
    failure
        .get("evidence")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect()
}

/// One failed claim as the model reads it, and the places of its evidence passages among the section's.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct Claim {
    row: Told<Value>,
    evidence: Internal<Vec<usize>>,
}

/// The failed claims that stand on one sentence of the section, or one failed example: what an entry answers once.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct Failed {
    id: String,
    claims: Vec<Claim>,
    ids: Internal<Vec<String>>,
    /// The sentences the entry may change.
    targets: Internal<Vec<Protected>>,
    /// The sentences of live claims it may not.
    protected: Internal<Vec<Protected>>,
    /// The places of the passages shown with it; `None`: all it needs.
    shown: Internal<Option<Vec<usize>>>,
    /// What running the failed example gave, for an entry that is one.
    execution: Option<Told<Value>>,
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
    fn needs(&self) -> Vec<usize> {
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

    fn claim_ids(&self) -> Vec<ClaimId> {
        self.ids.value().iter().cloned().map(ClaimId).collect()
    }
}

/// An evidence passage a failed claim was checked against.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct Passage {
    id: String,
    title: Option<Told<String>>,
    text: Told<String>,
    source_index: Internal<Option<u64>>,
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
    fn excerpt(&self) -> Option<Value> {
        let index = (*self.source_index.value())?;
        Some(json!({"sourceIndex": index, "excerpt": self.text.0}))
    }
}

pub(super) struct SectionRepair;

impl Contract for SectionRepair {
    const ID: ContractId = ContractId::new("section-repair");
    type Input = Input;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct Input {
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
pub(super) struct Correction {
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

const NO_ROOM: &str =
    "A failed statement's evidence passage does not fit in this model's context window.";

const NO_CHANGE: &str = "The correction made no change to the sentence it was asked to correct.";

/// The quotable sentences of the passages `input` holds.
fn quotes_of(input: &Input) -> Quotes {
    let excerpts: Vec<Value> = input.passages.iter().filter_map(Passage::excerpt).collect();
    Quotes::of(excerpts.iter())
}

fn units_of(rows: &Rows, part: &str, set: &[Protected]) -> Vec<UnitRef> {
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
struct Narrowed {
    /// What is left of the entry's edits; none when the part kept is refused.
    edits: Option<rows::Edits>,
    /// Why the entry is not taken at all: it changes a sentence of a live claim together with its own.
    put_back: Option<String>,
}

/// `edits` without the changes to any sentence but `item`'s own.
fn narrowed(rows: &Rows, part: &str, item: &Failed, edits: &rows::Edits) -> Narrowed {
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
        cite: edits.cite.clone(),
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
        keys: edits.keys.clone(),
    });
    Narrowed { edits, put_back }
}

impl Narrowed {
    /// The edits that remain, when any do.
    fn remaining(&self) -> Option<&rows::Edits> {
        self.edits.as_ref().filter(|edits| !edits.is_empty())
    }
}

/// Why the entry for the failed sentence `id` is refused, when it is.
fn check(input: &Input, id: &str, correction: &Correction) -> std::result::Result<(), Refusal> {
    let Some(item) = input.items.iter().find(|item| item.id == id) else {
        return Ok(());
    };
    if correction.edits.is_empty() {
        return Ok(());
    }
    let mut rows = Rows::of_part(input.lesson.value(), input.part.value());
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

const CONFINEMENT: &str =
    "Change only each failed claim's `sentence`; a change to any other sentence is discarded.";

const ENTRIES: &str = "Answer with one entry for each failed sentence: a failed claim names its entry (entry) and the labels of its sentence (sentence), and an entry's edits correct that sentence for every failed claim that names the entry.";

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
        let section = shown_section(&rows, &shown, part);
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
        let mut says = Says::new(system(input.question, input.cites), task.to_string());
        says.output_tokens = input.output_tokens;
        Ok(says)
    }

    fn accept_for(
        input: &Input,
        of: &Picked<Failed>,
        correction: &Correction,
    ) -> std::result::Result<(), Refusal> {
        check(input, &of.reference, correction)
    }
}

/// The section `rows` are of, as a request that shows it alone shows it: without its pointer.
pub(super) fn shown_section(rows: &Rows, shown: &rows::ShownRows<'_>, part: &str) -> Value {
    let mut section = rows
        .rendered(shown)
        .pointer(part)
        .cloned()
        .unwrap_or(Value::Null);
    if let Some(fields) = section.as_object_mut() {
        fields.shift_remove("part");
    }
    section
}

/// The material a request shows: the lesson with every part but `part` emptied.
fn only_section(candidate: &Value, part: &str) -> Value {
    let mut out = serde_json::Map::new();
    for list in ["blocks", "questions"] {
        let items: Vec<Value> = candidate
            .get(list)
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .enumerate()
            .map(|(index, item)| {
                if format!("/{list}/{index}") == part {
                    item.clone()
                } else {
                    Value::Null
                }
            })
            .collect();
        out.insert(list.to_owned(), Value::Array(items));
    }
    Value::Object(out)
}

/// What a correction toward `goal` is told its sentence must do: the reading
/// that follows tests exactly that (acceptance design §2.4).
pub(super) fn goal_words(goal: Goal) -> &'static str {
    match goal {
        Goal::WhatThePassageStates => "Say only what the claim's evidence passages state. Where no passage states a detail, keep the sentence's point and leave that detail out.",
        Goal::AgreeWithThePassages => "Make the sentence agree with its evidence passages, correcting what the reason names.",
        Goal::AsTheFieldHasIt => "State it as the field generally has it, correcting what the reason names.",
        Goal::CorrectWhatWasFound => "Correct what the reason found wrong in the sentence, and keep the rest of the sentence as it is.",
        Goal::KeepTheOthers => "Take this claim out of the sentence and keep each statement in alsoStates, said as it is now.",
    }
}

const OPENING: &str = "Repair verified lesson defects. Correct the listed factual defects using the recorded evidence and execution results. Preserve valid teaching, practice demands and verbatim citations. Each failed claim lists the handles of its evidence passages. repairHistory lists sentences earlier corrections replaced or removed because a check failed them; do not reintroduce what they asserted, or paraphrases of it. A failed claim's alsoStates lists other statements its sentence makes that the lesson keeps: write the corrected sentence so that each of them is still said, and true. Each failed claim's goal says what its corrected sentence must do; the check that follows tests exactly that. A failed claim with notApplied was sent before and its correction could not be applied, for the reason given: correct it so that it can be. A failed claim with groundReason was refuted by a strict reading whose reason is the one given, and its evidence passages did not address it (groundReason): correct what that reason found wrong and keep the rest of the sentence as it is, without searching the passages for support. When evidence cannot support a detail, remove that detail cleanly instead of replacing it with another unsupported explanation. Label Markdown code/output fences. Read lessonSections alongside candidate so every correction agrees with the rest of the lesson: never introduce a claim that contradicts another section, contrast, or unflagged statement in the lesson. Never extrapolate a general protocol rule, field meaning, or architecture-wide mechanism from an unlabelled sample payload or a single product's internal feature unless an evidence passage explicitly states that rule; when passages only show a partial case or omit a detail, leave out only the unsupported detail rather than substituting a false or scope-widened rule.";

/// The instructions of a section repair, with the rules of citing when the lesson takes citations.
fn system(question: bool, cites: bool) -> String {
    let mut parts = vec![
        OPENING.to_owned(),
        CONFINEMENT.to_owned(),
        ENTRIES.to_owned(),
        lattice_core::shared::markdown_math::MATH_GUIDANCE.to_owned(),
        words::SENTENCES.to_owned(),
    ];
    if question {
        parts.push(words::KEYS.to_owned());
    }
    parts.push(words::EDITING.to_owned());
    if cites {
        parts.push(words::citing_evidence());
        parts.extend(
            [
                NEVER_SAY_WHERE,
                QUOTED_WORDING,
                UNQUOTED_REMARKS,
                STEPS_IN_ORDER,
            ]
            .map(str::to_owned),
        );
    } else {
        parts.push(words::NO_CITATIONS.to_owned());
    }
    parts.join(" ")
}

/// The sentences the teaching review after a claim repair or a user edit leaves
/// as they are: every live claim's but the `targets`', except those of the
/// machine's claims with rewrites left, which it may repair and which are
/// recorded as waiting for that (claim sources design §10.3).
pub(super) async fn teaching_protected(
    ledger: &mut ClaimLedger,
    policy: &dyn AcceptancePolicy,
    candidate: &Value,
    targets: &[ClaimId],
) -> Result<Vec<Protected>> {
    let released = ledger.releasable(policy, targets);
    ledger.begin_teaching(policy, candidate, &released).await?;
    Ok(Protected::releasing(ledger, targets, &released))
}

pub(crate) async fn candidate(
    llm: &dyn LLMPort,
    policy: &dyn AcceptancePolicy,
    prompt: &str,
    shape: LessonShape<'_>,
    mut context: Value,
    output_tokens: usize,
    (references, numbered): (&ReferenceCollection<'_>, &[LearningSourceDto]),
    ledger: &mut ClaimLedger,
) -> Result<String> {
    let sent = context_claims(&context);
    // A claim on its last attempt has its sentence taken out in code; the
    // model is asked only for the others, and for a statement whose cut is a
    // narrowing (`ClaimLedger::cut_of`).
    let before_cuts = context
        .get("candidate")
        .map(|lesson| lesson.to_string())
        .unwrap_or_default();
    let handled = cut::last_attempts(policy, shape.policy, &mut context, ledger).await?;
    if handled > 0 {
        if let Some(after) = context.get("candidate") {
            trace::changed(
                Stage::ClaimRepair,
                &Cause::none().claims(&sent),
                &before_cuts,
                &after.to_string(),
            );
        }
    }
    let candidate = if handled > 0 && targets(&context)?.is_empty() {
        let candidate = context
            .get("candidate")
            .cloned()
            .ok_or_else(|| invalid("Missing repair candidate."))?;
        crate::lesson_drafts::save_repair(&candidate.to_string(), None).await?;
        candidate
    } else {
        repair_sections(
            llm,
            policy,
            prompt,
            shape.value(),
            context,
            output_tokens,
            references,
            ledger,
        )
        .await?
    };
    // What this repair left as it was: the teaching review after it leaves
    // it too (claim sources design §10.3), as the synced ledger has it now.
    let protected = teaching_protected(ledger, policy, &candidate, &sent).await?;
    crate::lesson_progress::outside_checks(crate::teaching::review_lesson(
        llm,
        crate::teaching::LESSON_REPAIR_SYSTEM,
        prompt,
        shape,
        candidate.to_string(),
        output_tokens,
        (references, numbered),
        &protected,
    ))
    .await
}

/// Repairs every affected section in turn, one request for all of its failed
/// sentences, each on the section as the previous left it, saving every
/// section. A claim the user resolved meanwhile is not sent (§9); an entry the
/// model leaves unusable, or that changes no sentence it was asked to change,
/// costs its own claims an attempt and leaves the section as it was.
async fn repair_sections(
    llm: &dyn LLMPort,
    policy: &dyn AcceptancePolicy,
    prompt: &str,
    schema: &Value,
    mut context: Value,
    output_tokens: usize,
    references: &ReferenceCollection<'_>,
    ledger: &mut ClaimLedger,
) -> Result<Value> {
    let pending = targets(&context)?;
    if pending.is_empty() {
        return Err(invalid("No affected sections were recorded for repair."));
    }
    let wording = Wording::of(references);
    let cites = text_edits::takes_citations(schema);
    let options = crate::model_call::answer_options();
    let mut candidate = context
        .get("candidate")
        .cloned()
        .ok_or_else(|| invalid("Missing repair candidate."))?;
    for (position, unit) in pending.iter().copied().enumerate() {
        let (field, index) = section_of(&candidate, unit)?;
        let part = format!("/{field}/{index}");
        ledger.sync().await?;
        let dropped = withdraw_resolved(&mut context, ledger, policy);
        if !dropped.is_empty() {
            ledger.supersede(&dropped).await?;
        }
        let failures = section_failures(&context, unit);
        let examples = section_examples(&context, unit);
        if failures.is_empty() && examples.is_empty() {
            continue;
        }
        let mut claims = claim_ids(&failures);
        claims.extend(claim_ids(&examples));
        let input = section_input(
            llm,
            ledger,
            (&context, &candidate),
            (unit, &part),
            (&wording, cites, output_tokens),
        )?;
        let bare = Input {
            items: Vec::new(),
            ..input.clone()
        };
        if ask::room::<SectionRepair>(llm, &bare, &options)? == 0 {
            let error = crate::model_call::window_too_small(llm, "A section repair does not fit");
            ledger.reject(policy, &claims, &error.to_string()).await?;
            continue;
        }
        let own = Rows::of_part(&candidate, &part);
        for item in &input.items {
            if units_of(&own, &part, item.targets.value()).is_empty() {
                tracing::warn!(
                    unit,
                    claim = item.id.as_str(),
                    "A failed claim's sentence was not found in its section; its correction cannot be applied"
                );
            }
        }
        crate::lesson_progress::phase(
            crate::lesson_progress::Phase::Repair,
            format!(
                "Correcting affected section {} of {}",
                position + 1,
                pending.len()
            ),
        );
        let answered = {
            let _model_call =
                crate::lesson_progress::model_call(crate::lesson_progress::Term::Rewrite);
            ask_each::<SectionRepair>(
                llm,
                &Asking {
                    progress: None,
                    reasoning: None,
                },
                &mut Scope::for_question(),
                &input.items,
                |open, _| Input {
                    items: open.iter().map(|item| (*item).clone()).collect(),
                    ..input.clone()
                },
                &options,
            )
            .await
        };
        let before = candidate.to_string();
        let mut made: Vec<ClaimId> = Vec::new();
        match answered {
            Err(error) if !unmade(&error) => return Err(error),
            Err(error) => {
                let sent: Vec<ClaimId> = input.items.iter().flat_map(Failed::claim_ids).collect();
                ledger.reject(policy, &sent, &error.to_string()).await?;
            }
            Ok(settled) => {
                let mut taking = Taking {
                    input: &input,
                    schema,
                    field,
                    quotes: quotes_of(&input),
                    rows: Rows::of_part(&candidate, &part),
                };
                for (item, settled) in input.items.iter().zip(settled) {
                    match taking.settle(item, settled) {
                        Ok(()) => made.extend(item.claim_ids()),
                        Err(reason) => {
                            ledger.reject(policy, &item.claim_ids(), &reason).await?;
                        }
                    }
                }
                let repaired = taking.rows.value().pointer(&part).cloned();
                if let (Some(section), Some(slot)) = (repaired, candidate.pointer_mut(&part)) {
                    *slot = section;
                }
            }
        }
        if !made.is_empty() {
            trace::changed(
                Stage::ClaimRepair,
                &Cause::none().claims(&made),
                &before,
                &candidate.to_string(),
            );
        }
        let saved = candidate.to_string();
        drop_group(&mut context, unit, failures.len(), true);
        if let Some(object) = context.as_object_mut() {
            object.insert("candidate".into(), candidate.clone());
        }
        let checkpoint = PendingRepair {
            fingerprint: repair_fingerprint(llm, prompt, schema, &candidate, references),
            defects: ledger_targets(&context),
            context: context.clone(),
        };
        let remaining = if position + 1 < pending.len() {
            Some(serde_json::to_string(&checkpoint)?)
        } else {
            None
        };
        crate::lesson_drafts::save_repair(&saved, remaining).await?;
    }
    Ok(candidate)
}

#[cfg(test)]
#[path = "repair_tests.rs"]
mod tests;
