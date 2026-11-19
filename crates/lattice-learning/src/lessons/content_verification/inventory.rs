//! Claim locations are selected from lossless, app-owned text passages.
use super::*;
use lattice_core::application::services::model_contract::ask::{Alone, Answered, Refusal, Says};
use lattice_core::application::services::model_contract::shape::{Authored, Each, Fields, Form};
use lattice_core::application::services::model_contract::shown::{
    Drawn, Internal, Kind, Picked, Showing, Told,
};
use lattice_core::application::services::model_contract::units::{Grain, Texts, Unit};
use lattice_core::application::services::model_contract::{Contract, ContractId};
use lattice_core::shared::text::segmentation::{
    partition, Chars, Overflow, Plan, Prose, Rung, Strength,
};
use std::collections::BTreeMap;

pub(super) mod revisions;

mod batches;
use batches::ask;

/// The call that reads a batch of sections for their claims.
pub(crate) struct InventoryExtraction;

impl Contract for InventoryExtraction {
    const ID: ContractId = ContractId::new("inventory-extraction");
    type Input = Input;
}

/// One passage of a section: the field it is of, and its words.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Passage {
    id: Internal<String>,
    field: Told<String>,
    text: Told<String>,
}

/// Text of a section the model reads and locates no claim in.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Part {
    field: Told<String>,
    text: Told<String>,
}

/// One section to read for claims, as the model names it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Section {
    index: Internal<usize>,
    kind: Option<Told<String>>,
    correct_index: Option<Told<u64>>,
    check_role: Option<Told<String>>,
    context: Vec<Part>,
    context_role: Option<Told<String>>,
    passages: Vec<Passage>,
}

impl Kind for Section {
    const PREFIX: &'static str = "s";
    const DRAWN: Drawn = Drawn::Itself;
    type Ref = usize;

    fn reference(&self) -> usize {
        *self.index.value()
    }
}

fn told_text(value: &Value) -> Option<Told<String>> {
    value.as_str().map(|text| Told(text.to_owned()))
}

impl Section {
    /// The section at `index`, from its input `unit`.
    fn of(index: usize, unit: &Value) -> Self {
        let passages = unit["passages"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|passage| {
                Some(Passage {
                    id: Internal::of(passage["id"].as_str()?.to_owned()),
                    field: Told(passage["field"].as_str().unwrap_or_default().to_owned()),
                    text: told_text(&passage["text"])?,
                })
            })
            .collect();
        Self {
            index: Internal::of(index),
            kind: told_text(&unit["kind"]),
            correct_index: unit["correctIndex"].as_u64().map(Told),
            check_role: told_text(&unit["checkRole"]),
            context: unit["context"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|part| Part {
                    field: Told(part["field"].as_str().unwrap_or_default().to_owned()),
                    text: Told(part["text"].as_str().unwrap_or_default().to_owned()),
                })
                .collect(),
            context_role: told_text(&unit["contextRole"]),
            passages,
        }
    }

    /// The passages of the section as sentences, each text of the passage it is of.
    fn texts(&self) -> Texts<&Passage> {
        let mut texts = Texts::new();
        for passage in &self.passages {
            texts.push(
                passage,
                passage.id.value(),
                &passage.text.0,
                Grain::Sentence,
            );
        }
        texts
    }
}

/// The sections still to be read.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Input {
    sections: Vec<Section>,
}

/// A claim of a section: the sentence that states it, and the claim in standalone words.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Located {
    sentence: Picked<Unit>,
    statement: String,
}

impl Authored for Located {
    fn form() -> Form {
        Form::new().own::<Unit>("sentenceId").text("statement")
    }

    fn read(fields: &mut Fields<'_, '_>) -> Option<Self> {
        Some(Self {
            sentence: fields.own::<Unit>("sentenceId")?,
            statement: fields.text("statement")?,
        })
    }
}

/// What the model found in one section.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Claims {
    non_factual_reason: String,
    claims: Vec<Located>,
}

impl Authored for Claims {
    fn form() -> Form {
        Form::new()
            .text("nonFactualReason")
            .list::<Located>("claims")
    }

    fn read(fields: &mut Fields<'_, '_>) -> Option<Self> {
        Some(Self {
            non_factual_reason: fields.text("nonFactualReason")?,
            claims: fields.list("claims")?,
        })
    }
}

impl Answered for InventoryExtraction {
    type Answer = Each<Section, Claims>;
    const ALONE: Alone = Alone::Pending;

    fn says(input: &Input, showing: &mut Showing<'_>) -> Result<Says> {
        let sections = showing.show(&input.sections)?;
        let mut units = Vec::new();
        for (handle, section) in sections.iter() {
            let mut passages: Vec<(&Told<String>, Vec<Value>)> = Vec::new();
            for (passage, units) in section.texts().iter() {
                let own = showing.under(handle, units.units())?;
                if own.is_empty() {
                    continue;
                }
                let listed = own
                    .iter()
                    .map(|(label, unit)| json!({"id": label.written(), "text": unit.text()}))
                    .collect();
                passages.push((&passage.field, listed));
            }
            let mut unit = serde_json::Map::new();
            unit.insert("id".into(), json!(handle.written()));
            unit.insert("kind".into(), json!(section.kind));
            unit.insert("correctIndex".into(), json!(section.correct_index));
            unit.insert(
                "passages".into(),
                passages
                    .into_iter()
                    .map(|(field, sentences)| json!({"field": field, "sentences": sentences}))
                    .collect(),
            );
            if let Some(role) = &section.check_role {
                unit.insert("checkRole".into(), json!(role));
            }
            if !section.context.is_empty() || section.context_role.is_some() {
                unit.insert(
                    "context".into(),
                    section
                        .context
                        .iter()
                        .map(|part| json!({"field": part.field, "text": part.text}))
                        .collect(),
                );
            }
            if let Some(role) = &section.context_role {
                unit.insert("contextRole".into(), json!(role));
            }
            units.push(Value::Object(unit));
        }
        let mut says = Says::new(
            batches::EXTRACTION_SYSTEM,
            json!({"units": units}).to_string(),
        );
        says.output_tokens = batches::EXTRACTION_OUTPUT_TOKENS;
        Ok(says)
    }

    fn accept(_input: &Input, claims: &Claims) -> std::result::Result<(), Refusal> {
        if claims.claims.is_empty() && claims.non_factual_reason.trim().is_empty() {
            return Err(Refusal(
                "Give this unit its claims, or an explicit nonFactualReason when it states no fact."
                    .into(),
            ));
        }
        if claims
            .claims
            .iter()
            .any(|claim| claim.statement.trim().is_empty())
        {
            return Err(Refusal(
                "Each claim needs its statement written out.".into(),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
pub(super) fn fixture_response(entries: Vec<Value>) -> Value {
    json!({"entries": entries})
}

/// How a string field is cut into passages: at the latest paragraph break that
/// leaves one past 600 characters, else at the latest sentence break within
/// 1,400; a sentence longer than that is a passage of its own.
const PASSAGE_PLAN: Plan<'static> = Plan {
    max: 1_400,
    ladder: &[
        Rung {
            at_least: Strength::Paragraph,
            floor: 0.43,
        },
        Rung {
            at_least: Strength::Sentence,
            floor: 0.0,
        },
    ],
    overflow: Overflow::Extend,
};

fn passages(value: &Value, path: &str, index: usize, output: &mut Vec<Value>) {
    match value {
        Value::String(text) => {
            for span in partition(text, &Prose, &Chars, &PASSAGE_PLAN) {
                output.push(json!({"id":format!("unit-{index}-passage-{}",output.len()),"field":path,"text":text.get(span).unwrap_or_default()}));
            }
        }
        Value::Array(items) => {
            for (i, item) in items.iter().enumerate() {
                passages(item, &format!("{path}/{i}"), index, output);
            }
        }
        Value::Object(items) => {
            for (key, item) in items.iter().filter(|(key, _)| key.as_str() != "kind") {
                passages(item, &format!("{path}/{key}"), index, output);
            }
        }
        _ => {}
    }
}

/// What a check unit's passages are, for the extractor.
const CHECK_ROLE: &str = "This section is a check. Before its 'Check your thinking' line it poses a case for the learner to reason through: the case's inputs are lesson-internal, not universal facts. After that line it gives the answer, the reasoning, and misconceptions a learner might hold, each named as wrong. Extract what the lesson asserts about a misconception (that it fails, and why it fails); never extract the misconception itself as a true fact.";

/// What the extractor is told of a question and its options.
const CONTEXT_ROLE: &str = "The question and its options, for reading its explanation by. They are not passages and no claim is located in them: never extract an option as a fact. The selected answer is correctIndex.";

/// Why a section with nothing to read has no claims.
const NOTHING_TO_READ: &str = "Nothing in this section is read for claims: it asks something of the learner and states no fact.";

/// A question and its options as context, each field with an id of its own.
fn context(unit: &Value, index: usize) -> Vec<Value> {
    let options = unit["options"]
        .as_array()
        .into_iter()
        .flatten()
        .enumerate()
        .map(|(ordinal, option)| (format!("/options/{ordinal}"), option));
    std::iter::once(("/prompt".to_owned(), &unit["prompt"]))
        .chain(options)
        .filter_map(|(field, text)| text.as_str().map(|text| (field, text)))
        .enumerate()
        .map(|(position, (field, text))| {
            json!({"id":format!("unit-{index}-context-{position}"),"field":field,"text":text})
        })
        .collect()
}

/// What the extractor reads: a block's body and a question's answer
/// explanation, which is where the lesson states what is so. A heading, a
/// rubric, a reflection's prompts and a question's stem and options are the
/// lesson's own scaffolding, never read for claims (claim classes design §3.4).
pub(crate) fn inputs(content: &[Value]) -> Vec<Value> {
    content.iter().enumerate().map(|(index, unit)| {
        let mut text = Vec::new();
        if unit["role"] != "reflection" {
            passages(&unit["body"], "/body", index, &mut text);
        }
        passages(&unit["explanation"], "/explanation", index, &mut text);
        for (slot, reason) in unit["rationales"].as_array().into_iter().flatten().enumerate() {
            if reason.as_str().is_some_and(|reason| !reason.trim().is_empty()) {
                passages(reason, &format!("/rationales/{slot}"), index, &mut text);
            }
        }
        let mut input = json!({"index":index,"kind":unit.get("kind"),"correctIndex":unit.get("correctIndex"),"passages":text});
        // A check names misconceptions as wrong below its reveal line
        // (learning science lessons §3.3), as a quiz names wrong options: the
        // extractor must not take one for an assertion and send it to be
        // "corrected" into a true statement.
        let is_check = unit["kind"] == "teaching"
            && unit["body"].as_str().and_then(crate::generation::split_check).is_some();
        if let (true, Some(object)) = (is_check, input.as_object_mut()) {
            object.insert("checkRole".into(), json!(CHECK_ROLE));
        }
        if let (true, Some(object)) = (unit["kind"] == "assessment", input.as_object_mut()) {
            object.insert("context".into(), json!(context(unit, index)));
            object.insert("contextRole".into(), json!(CONTEXT_ROLE));
        }
        input
    }).collect()
}

/// The inventory of `section` that `claims` gives: each claim's quote is the exact text of the sentence it points at.
fn resolve(section: &Section, claims: Claims) -> std::result::Result<UnitClaims, String> {
    let index = *section.index.value();
    let texts = section.texts();
    let mut located = Vec::new();
    for claim in claims.claims {
        let (_, sentence) = texts
            .found(claim.sentence.reference)
            .ok_or_else(|| format!("Section {index} names a sentence it does not hold"))?;
        located.push(Claim {
            quote: sentence.to_owned(),
            statement: claim.statement,
        });
    }
    Ok(UnitClaims {
        index,
        claims: located,
        non_factual_reason: claims.non_factual_reason,
    })
}

#[cfg(any(test, feature = "test-support"))]
pub(crate) async fn extract(llm: &dyn LLMPort, content: &[Value]) -> Result<Inventory> {
    extract_scoped(
        llm,
        content,
        &(0..content.len()).collect::<Vec<_>>(),
        BTreeMap::new(),
    )
    .await
}

pub(super) async fn extract_remaining(
    llm: &dyn LLMPort,
    content: &[Value],
    retained: Vec<UnitClaims>,
) -> Result<Inventory> {
    let mut retained: BTreeMap<_, _> = retained
        .into_iter()
        .map(|unit| (unit.index, unit))
        .collect();
    for index in 0..content.len() {
        if let std::collections::btree_map::Entry::Vacant(entry) = retained.entry(index) {
            if let Some(updated) = revisions::update(llm, content, index).await? {
                entry.insert(updated);
            }
        }
    }
    let missing: Vec<_> = (0..content.len())
        .filter(|index| !retained.contains_key(index))
        .collect();
    extract_scoped(llm, content, &missing, retained).await
}

async fn extract_scoped(
    llm: &dyn LLMPort,
    content: &[Value],
    indices: &[usize],
    mut accepted: BTreeMap<usize, UnitClaims>,
) -> Result<Inventory> {
    let input = inputs(content);
    // A section with nothing to read has no claims to ask the model for.
    for index in indices {
        let unread = input
            .get(*index)
            .and_then(|unit| unit["passages"].as_array())
            .is_some_and(|passages| claim_sentences::ids(passages).is_empty());
        if unread {
            accepted.insert(
                *index,
                UnitClaims {
                    index: *index,
                    claims: Vec::new(),
                    non_factual_reason: NOTHING_TO_READ.into(),
                },
            );
        }
    }
    let pending: Vec<usize> = indices
        .iter()
        .copied()
        .filter(|index| !accepted.contains_key(index))
        .collect();
    ask(
        llm,
        (content, input.as_slice()),
        &pending,
        &mut accepted,
        true,
    )
    .await?;
    let inventory = Inventory {
        units: accepted.into_values().collect(),
    };
    validate_inventory(&inventory, content)?;
    Ok(inventory)
}

#[cfg(test)]
mod tests;
