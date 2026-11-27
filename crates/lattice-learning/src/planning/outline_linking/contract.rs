use lattice_core::application::services::model_contract::{
    ask::{Alone, Answered, Refusal, Says},
    packed::Packed,
    shape::{Authored, Each, Fields, Form},
    shown::{Drawn, Internal, Kind, Picked, Showing, Told},
    Contract, ContractId,
};
use lattice_core::shared::error::Result;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;

use super::LINK_SYSTEM;

pub(crate) struct OutlineLinking;

impl Contract for OutlineLinking {
    const ID: ContractId = ContractId::new("outline-linking");
    type Input = Input;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ItemKind {
    Summary,
    Outcome,
    Lesson,
}

impl ItemKind {
    pub(super) fn name(self) -> &'static str {
        match self {
            Self::Summary => "module summary",
            Self::Outcome => "outcome",
            Self::Lesson => "lesson objective",
        }
    }

    pub(super) fn text(self, field: &Value) -> Option<&str> {
        match self {
            Self::Summary | Self::Outcome => field.get("text"),
            Self::Lesson => field.get("objective"),
        }
        .and_then(Value::as_str)
    }
}

/// A saved passage, as the model names it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Passage {
    pub(super) at: usize,
    pub(super) title: Told<String>,
    pub(super) quote: Told<String>,
}

impl Kind for Passage {
    const PREFIX: &'static str = "p";
    const DRAWN: Drawn = Drawn::Itself;
    type Ref = usize;

    fn reference(&self) -> usize {
        self.at
    }
}

/// An outline item, as the model names it, and the passages offered for it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Item {
    pub(super) at: usize,
    pub(super) kind: Told<String>,
    pub(super) module_title: Told<String>,
    pub(super) text: Told<String>,
    pub(super) lesson_title: Option<Told<String>>,
    pub(super) candidates: Internal<Vec<usize>>,
}

impl Kind for Item {
    const PREFIX: &'static str = "i";
    const DRAWN: Drawn = Drawn::Itself;
    type Ref = usize;

    fn reference(&self) -> usize {
        self.at
    }
}

/// What one request says about the items still open.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Input {
    pub(super) items: Vec<Item>,
    pub(super) passages: Packed<Passage>,
}

/// What the model chose for one item.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Choice {
    pub(super) candidate: Option<Picked<Passage>>,
    pub(super) reason: String,
}

impl Authored for Choice {
    fn form() -> Form {
        Form::new()
            .point_or_none::<Passage>("candidate")
            .text("reason")
    }

    fn read(fields: &mut Fields<'_, '_>) -> Option<Self> {
        Some(Self {
            candidate: fields.point_or_none("candidate")?,
            reason: fields.text("reason")?,
        })
    }
}

impl Answered for OutlineLinking {
    type Answer = Each<Item, Choice>;
    const ALONE: Alone = Alone::Pending;

    fn within(input: &Input, room: usize) -> Option<Input> {
        let list: Vec<&Passage> = input.passages.iter().collect();
        let needs: Vec<Vec<usize>> = input
            .items
            .iter()
            .map(|item| {
                item.candidates
                    .value()
                    .iter()
                    .filter_map(|at| list.iter().position(|passage| passage.at == *at))
                    .collect()
            })
            .collect();
        let items: Vec<Item> = input
            .passages
            .for_owners(room, &needs)
            .into_iter()
            .filter_map(|(owner, places)| {
                if places.is_empty() {
                    return None;
                }
                let item = input.items.get(owner)?;
                let candidates = places
                    .iter()
                    .filter_map(|place| list.get(*place).map(|passage| passage.at))
                    .collect();
                Some(Item {
                    candidates: Internal::of(candidates),
                    ..item.clone()
                })
            })
            .collect();
        let named: Vec<usize> = items
            .iter()
            .flat_map(|item| item.candidates.value().iter().copied())
            .collect();
        let kept: Vec<Passage> = list
            .into_iter()
            .filter(|passage| named.contains(&passage.at))
            .cloned()
            .collect();
        let passages = Packed::new(kept, |_| 0);
        Some(Input { items, passages })
    }

    fn says(input: &Input, showing: &mut Showing<'_>) -> Result<Says> {
        let items = showing.show(&input.items)?;
        let offered: Vec<Passage> = input.passages.iter().cloned().collect();
        let passages = showing.show(&offered)?;
        let mut handles: BTreeMap<usize, String> = BTreeMap::new();
        let listed: Vec<Value> = passages
            .iter()
            .map(|(handle, passage)| {
                handles.insert(passage.at, handle.written());
                json!({"id": handle.written(), "sourceTitle": passage.title, "passage": passage.quote})
            })
            .collect();
        let asked: Vec<Value> = items
            .iter()
            .map(|(handle, item)| {
                let candidates: Vec<&String> = item
                    .candidates
                    .value()
                    .iter()
                    .filter_map(|at| handles.get(at))
                    .collect();
                let mut value = json!({"id": handle.written(), "kind": item.kind, "moduleTitle": item.module_title, "item": item.text, "candidates": candidates});
                if let (Some(title), Some(object)) = (&item.lesson_title, value.as_object_mut()) {
                    object.insert("lessonTitle".into(), json!(title));
                }
                value
            })
            .collect();
        let task = json!({"passages": listed, "items": asked}).to_string();
        let mut says = Says::new(LINK_SYSTEM, task);
        says.output_tokens = 200 + 120 * input.items.len();
        Ok(says)
    }

    fn accept_for(
        input: &Input,
        of: &Picked<Item>,
        choice: &Choice,
    ) -> std::result::Result<(), Refusal> {
        if choice.reason.trim().is_empty() {
            return Err(Refusal("Give the reason for this choice in reason.".into()));
        }
        if let Some(candidate) = &choice.candidate {
            let allowed = input
                .items
                .iter()
                .find(|item| item.at == of.reference)
                .is_some_and(|item| item.candidates.value().contains(&candidate.reference));
            if !allowed {
                return Err(Refusal(
                    "Choose only among the candidate passages listed for this item, or none."
                        .into(),
                ));
            }
        }
        Ok(())
    }
}
