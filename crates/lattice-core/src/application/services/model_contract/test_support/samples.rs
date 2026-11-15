//! One small contract per answer shape, over invented things: a shelf of tools, a list of colours.

use std::fmt::Write as _;

use serde::{Deserialize, Serialize};

use super::taint::Marks;
use crate::application::services::grounded_generation::EvidencePassage;
use crate::application::services::model_contract::ask::{Alone, Answered, Says};
use crate::application::services::model_contract::edit::Change;
use crate::application::services::model_contract::packed::Packed;
use crate::application::services::model_contract::shape::{
    Authored, Each, Fields, Flagged, Form, Lead, New, Order, Pick, PickSome, Pointers, Schema,
    Shape, Union, Value, Vocabulary,
};
use crate::application::services::model_contract::shown::{
    Drawn, Internal, Kind, Picked, Scope, Showing, Told,
};
use crate::application::services::model_contract::units::{Grain, Unit, Units};
use crate::application::services::model_contract::{Contract, ContractId};
use crate::shared::error::Result;

/// A tool on the shelf, with the slots it has of its own.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tool {
    pub key: String,
    pub name: Told<String>,
    pub slots: Vec<Slot>,
}

impl Kind for Tool {
    const PREFIX: &'static str = "t";
    const DRAWN: Drawn = Drawn::Name;
    type Ref = String;

    fn reference(&self) -> String {
        self.key.clone()
    }
}

/// One of the own things of a tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Slot {
    pub key: String,
    pub label: Told<String>,
}

impl Kind for Slot {
    const PREFIX: &'static str = "o";
    const DRAWN: Drawn = Drawn::Itself;
    type Ref = String;

    fn reference(&self) -> String {
        self.key.clone()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Colour {
    pub id: u64,
    pub name: Told<String>,
}

impl Kind for Colour {
    const PREFIX: &'static str = "c";
    const DRAWN: Drawn = Drawn::Number;
    type Ref = u64;

    fn reference(&self) -> u64 {
        self.id
    }
}

/// The input of every sample: what a contract of the layer holds, in the three kinds of field.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Input {
    pub place: Told<String>,
    pub request: Internal<String>,
    pub batch: Internal<u64>,
    pub shelf: Vec<Tool>,
    pub colours: Vec<Colour>,
    /// The sentences of the text a change edits, without their full stops.
    pub sentences: Vec<Told<String>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mood {
    Calm,
    Tense,
}

impl Vocabulary for Mood {
    const WORDS: &'static [(&'static str, Self)] = &[("calm", Mood::Calm), ("tense", Mood::Tense)];
}

#[derive(Debug, Clone, PartialEq)]
pub struct Detail {
    pub place: String,
}

impl Authored for Detail {
    fn form() -> Form {
        Form::new().text("where")
    }

    fn read(fields: &mut Fields<'_, '_>) -> Option<Self> {
        Some(Self {
            place: fields.text("where")?,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Step {
    pub text: String,
}

impl Authored for Step {
    fn form() -> Form {
        Form::new().text("step")
    }

    fn read(fields: &mut Fields<'_, '_>) -> Option<Self> {
        Some(Self {
            text: fields.text("step")?,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Choice {
    Plain(String),
    Moody(String, Mood),
}

impl Union for Choice {
    fn forms() -> Vec<(&'static str, Form)> {
        vec![
            ("plain", Form::new().text("body")),
            ("moody", Form::new().text("body").word::<Mood>("mood")),
        ]
    }

    fn read(kind: &str, fields: &mut Fields<'_, '_>) -> Option<Self> {
        match kind {
            "plain" => Some(Self::Plain(fields.text("body")?)),
            "moody" => {
                let body = fields.text("body")?;
                let mood = fields.word::<Mood>("mood")?;
                Some(Self::Moody(body, mood))
            }
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Note {
    pub summary: String,
    pub mood: Mood,
    pub detail: Detail,
    pub steps: Vec<Step>,
    pub choice: Choice,
}

impl Authored for Note {
    fn form() -> Form {
        Form::new()
            .text("summary")
            .word::<Mood>("mood")
            .group::<Detail>("detail")
            .list::<Step>("steps")
            .one_of::<Choice>("choice")
    }

    fn read(fields: &mut Fields<'_, '_>) -> Option<Self> {
        let summary = fields.text("summary")?;
        let mood = fields.word::<Mood>("mood")?;
        let detail = fields.group::<Detail>("detail")?;
        let steps = fields.list::<Step>("steps")?;
        let choice = fields.one_of::<Choice>("choice")?;
        Some(Self {
            summary,
            mood,
            detail,
            steps,
            choice,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Card {
    pub front: String,
    pub sources: Vec<Picked<Tool>>,
}

impl Authored for Card {
    fn form() -> Form {
        Form::new().text("front").cite::<Tool>("sources")
    }

    fn read(fields: &mut Fields<'_, '_>) -> Option<Self> {
        let front = fields.text("front")?;
        let sources = fields.cite::<Tool>("sources");
        Some(Self { front, sources })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Verdict {
    pub mood: Mood,
    pub why: String,
}

impl Authored for Verdict {
    fn form() -> Form {
        Form::new().word::<Mood>("mood").text("why")
    }

    fn read(fields: &mut Fields<'_, '_>) -> Option<Self> {
        let mood = fields.word::<Mood>("mood")?;
        let why = fields.text("why")?;
        Some(Self { mood, why })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Review {
    pub note: String,
    pub slot: Picked<Slot>,
    pub evidence: Vec<Picked<Slot>>,
    pub verdicts: Vec<(Picked<Slot>, Verdict)>,
}

impl Authored for Review {
    fn form() -> Form {
        Form::new()
            .text("note")
            .own::<Slot>("slot")
            .cite_own::<Slot>("evidence")
            .each_own::<Slot, Verdict>("verdicts")
    }

    fn read(fields: &mut Fields<'_, '_>) -> Option<Self> {
        let note = fields.text("note")?;
        let slot = fields.own::<Slot>("slot")?;
        let evidence = fields.cite_own::<Slot>("evidence");
        let verdicts = fields.each_own::<Slot, Verdict>("verdicts")?;
        Some(Self {
            note,
            slot,
            evidence,
            verdicts,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Remark {
    pub problem: String,
    pub tools: Vec<Picked<Tool>>,
}

impl Authored for Remark {
    fn form() -> Form {
        Form::new().text("problem").cite::<Tool>("tools")
    }

    fn read(fields: &mut Fields<'_, '_>) -> Option<Self> {
        let problem = fields.text("problem")?;
        let tools = fields.cite::<Tool>("tools");
        Some(Self { problem, tools })
    }
}

fn slot(marks: &mut Marks, key: &str, label: &str) -> Slot {
    Slot {
        key: marks.reference(key.to_owned()),
        label: marks.told(label.to_owned()),
    }
}

fn tool(marks: &mut Marks, key: &str, name: &str, slots: Vec<Slot>) -> Tool {
    Tool {
        key: marks.reference(key.to_owned()),
        name: marks.told(name.to_owned()),
        slots,
    }
}

fn colour(marks: &mut Marks, id: u64, name: &str) -> Colour {
    Colour {
        id: marks.reference(id),
        name: marks.told(name.to_owned()),
    }
}

fn empty(marks: &mut Marks) -> Input {
    Input {
        place: marks.told("the town of Marrowby".to_owned()),
        request: marks.internal("request-6d2f81a4".to_owned()),
        batch: marks.internal(7_340_219_u64),
        shelf: Vec::new(),
        colours: Vec::new(),
        sentences: Vec::new(),
    }
}

fn shelf(marks: &mut Marks, with_slots: bool) -> Vec<Tool> {
    let (compass_slots, coil_slots) = if with_slots {
        (
            vec![
                slot(marks, "slot-key-0101", "left hook"),
                slot(marks, "slot-key-0102", "right hook"),
            ],
            vec![
                slot(marks, "slot-key-0201", "inner knot"),
                slot(marks, "slot-key-0202", "outer knot"),
            ],
        )
    } else {
        (Vec::new(), Vec::new())
    };
    vec![
        tool(marks, "tool-key-0001", "brass compass", compass_slots),
        tool(marks, "tool-key-0002", "rope coil", coil_slots),
        tool(marks, "tool-key-0003", "iron lantern", Vec::new()),
    ]
}

fn colours(marks: &mut Marks) -> Vec<Colour> {
    vec![
        colour(marks, 5_000_101, "harbour blue"),
        colour(marks, 5_000_102, "signal red"),
        colour(marks, 5_000_103, "sand yellow"),
    ]
}

/// The shown things as a listing for the task: tools under their handles, their slots, colours by number.
fn listing(input: &Input, showing: &mut Showing<'_>) -> Result<String> {
    let mut out = String::new();
    let tools = showing.show(&input.shelf)?;
    for (handle, tool) in tools.iter() {
        let _ = writeln!(out, "{} {}", handle.bracketed(), tool.name.0);
        let slots = showing.under(handle, &tool.slots)?;
        for (label, slot) in slots.iter() {
            let _ = writeln!(out, "  {} {}", label.bracketed(), slot.label.0);
        }
    }
    let colours = showing.show(&input.colours)?;
    for (handle, colour) in colours.iter() {
        let _ = writeln!(out, "Colour {}: {}", handle.number(), colour.name.0);
    }
    Ok(out)
}

fn notice(input: &Input) -> Vec<EvidencePassage> {
    vec![EvidencePassage {
        id: input.request.value().clone(),
        label: "Notice".into(),
        text: input.place.0.clone(),
        rank: 0.0,
    }]
}

fn says_of(
    input: &Input,
    showing: &mut Showing<'_>,
    instructions: &str,
    task: &str,
) -> Result<Says> {
    let listing = listing(input, showing)?;
    let mut says = Says::new(instructions, format!("{task}\n\n{listing}"));
    says.evidence_heading = "Notices:".into();
    says.evidence = notice(input);
    Ok(says)
}

pub struct SampleValue;

impl Contract for SampleValue {
    const ID: ContractId = ContractId::new("sample-value");
    type Input = Input;
}

impl Answered for SampleValue {
    type Answer = Value<Note>;
    const ALONE: Alone = Alone::Fails;

    fn says(input: &Input, showing: &mut Showing<'_>) -> Result<Says> {
        says_of(
            input,
            showing,
            "You write short notes for a harbour office.",
            "Write one note about the notice.",
        )
    }
}

impl SampleValue {
    pub fn fixture(marks: &mut Marks) -> Input {
        empty(marks)
    }
}

pub struct SamplePick;

impl Contract for SamplePick {
    const ID: ContractId = ContractId::new("sample-pick");
    type Input = Input;
}

impl Answered for SamplePick {
    type Answer = Pick<Tool>;
    const ALONE: Alone = Alone::Pending;

    fn says(input: &Input, showing: &mut Showing<'_>) -> Result<Says> {
        says_of(
            input,
            showing,
            "You help a keeper choose from a shelf.",
            "Choose the tool that fits the notice, or none.",
        )
    }
}

impl SamplePick {
    pub fn fixture(marks: &mut Marks) -> Input {
        let shelf = shelf(marks, false);
        Input {
            shelf,
            ..empty(marks)
        }
    }
}

pub struct SamplePickSome;

impl Contract for SamplePickSome {
    const ID: ContractId = ContractId::new("sample-pick-some");
    type Input = Input;
}

impl Answered for SamplePickSome {
    type Answer = PickSome<Colour>;
    const ALONE: Alone = Alone::Pending;

    fn says(input: &Input, showing: &mut Showing<'_>) -> Result<Says> {
        says_of(
            input,
            showing,
            "You help a painter choose among colours.",
            "Choose every colour the notice calls for.",
        )
    }
}

impl SamplePickSome {
    pub fn fixture(marks: &mut Marks) -> Input {
        let colours = colours(marks);
        Input {
            colours,
            ..empty(marks)
        }
    }
}

pub struct SampleOrder;

impl Contract for SampleOrder {
    const ID: ContractId = ContractId::new("sample-order");
    type Input = Input;
}

impl Answered for SampleOrder {
    type Answer = Order<Tool>;
    const ALONE: Alone = Alone::Pending;

    fn says(input: &Input, showing: &mut Showing<'_>) -> Result<Says> {
        says_of(
            input,
            showing,
            "You help a keeper put a shelf in order.",
            "Put every tool in the order it is needed.",
        )
    }
}

impl SampleOrder {
    pub fn fixture(marks: &mut Marks) -> Input {
        let shelf = shelf(marks, false);
        Input {
            shelf,
            ..empty(marks)
        }
    }
}

pub struct SampleNew;

impl Contract for SampleNew {
    const ID: ContractId = ContractId::new("sample-new");
    type Input = Input;
}

impl Answered for SampleNew {
    type Answer = New<Card>;
    const ALONE: Alone = Alone::Pending;

    fn says(input: &Input, showing: &mut Showing<'_>) -> Result<Says> {
        says_of(
            input,
            showing,
            "You write study cards about a shelf of tools.",
            "Write three cards, each citing the tools it rests on.",
        )
    }
}

impl SampleNew {
    pub fn fixture(marks: &mut Marks) -> Input {
        let shelf = shelf(marks, false);
        Input {
            shelf,
            ..empty(marks)
        }
    }
}

pub struct SampleEach;

impl Contract for SampleEach {
    const ID: ContractId = ContractId::new("sample-each");
    type Input = Input;
}

impl Answered for SampleEach {
    type Answer = Each<Tool, Review>;
    const ALONE: Alone = Alone::Pending;

    fn says(input: &Input, showing: &mut Showing<'_>) -> Result<Says> {
        says_of(
            input,
            showing,
            "You review tools one by one.",
            "Review each tool, and judge each of its slots.",
        )
    }
}

impl SampleEach {
    pub fn fixture(marks: &mut Marks) -> Input {
        let shelf = shelf(marks, true);
        Input {
            shelf,
            ..empty(marks)
        }
    }
}

pub struct SampleFlagged;

impl Contract for SampleFlagged {
    const ID: ContractId = ContractId::new("sample-flagged");
    type Input = Input;
}

impl Answered for SampleFlagged {
    type Answer = Flagged<Tool, Remark>;
    const ALONE: Alone = Alone::Pending;

    fn says(input: &Input, showing: &mut Showing<'_>) -> Result<Says> {
        says_of(
            input,
            showing,
            "You inspect a shelf for faults.",
            "Mark only the tools that have a fault.",
        )
    }
}

impl SampleFlagged {
    pub fn fixture(marks: &mut Marks) -> Input {
        let shelf = shelf(marks, false);
        Input {
            shelf,
            ..empty(marks)
        }
    }
}

pub struct SampleChange;

impl Contract for SampleChange {
    const ID: ContractId = ContractId::new("sample-change");
    type Input = Input;
}

impl Answered for SampleChange {
    type Answer = Change<Unit, String>;
    const ALONE: Alone = Alone::Pending;

    fn says(input: &Input, showing: &mut Showing<'_>) -> Result<Says> {
        let sentences: Vec<&str> = input.sentences.iter().map(|told| told.0.as_str()).collect();
        let units = Units::of(&sentences.join(". "), Grain::Sentence);
        let shown = units.shown(showing)?;
        let task = format!(
            "Edit the notice where it is wrong.\n\n{}",
            units.rows(&shown)
        );
        let mut says = Says::new("You edit short notices.", task);
        says.evidence_heading = "Notices:".into();
        says.evidence = notice(input);
        Ok(says)
    }
}

impl SampleChange {
    pub fn fixture(marks: &mut Marks) -> Input {
        let sentences = [
            "The lamp is lit at dusk",
            "Boats leave before the tide turns",
            "The bell rings twice at noon",
        ]
        .into_iter()
        .map(|sentence| marks.told(sentence.to_owned()))
        .collect();
        Input {
            sentences,
            ..empty(marks)
        }
    }
}

pub struct SampleLead;

impl Contract for SampleLead {
    const ID: ContractId = ContractId::new("sample-lead");
    type Input = Input;
}

impl Answered for SampleLead {
    type Answer = Lead<Mood, Tool>;
    const ALONE: Alone = Alone::Pending;

    fn says(input: &Input, showing: &mut Showing<'_>) -> Result<Says> {
        says_of(
            input,
            showing,
            "You judge whether a tool suits a notice.",
            "Say calm or tense, then why, then the one tool you mean, or none.",
        )
    }
}

impl SampleLead {
    pub fn fixture(marks: &mut Marks) -> Input {
        let shelf = shelf(marks, false);
        Input {
            shelf,
            ..empty(marks)
        }
    }
}

/// The schema `C` asks for `input`, written for `pointers`. `None` for a text form.
pub fn schema_of<C: Answered>(input: &C::Input, pointers: Pointers) -> Option<Schema> {
    let mut scope = Scope::for_question();
    let mut showing = scope.request();
    if let Err(error) = C::says(input, &mut showing) {
        cannot_say(C::ID.as_str(), &error.to_string());
    }
    <C::Answer as Shape>::schema(&showing, pointers)
}

#[allow(clippy::panic)]
fn cannot_say(id: &str, error: &str) -> ! {
    panic!("the sample {id} could not say its request: {error}")
}

fn both<C: Answered<Input = Input>>(
    out: &mut Vec<(&'static str, Schema)>,
    names: [&'static str; 2],
    input: &Input,
) {
    for (name, pointers) in names.into_iter().zip([Pointers::Enum, Pointers::Plain]) {
        if let Some(schema) = schema_of::<C>(input, pointers) {
            out.push((name, schema));
        }
    }
}

/// Every structured sample's schema, written both ways, from its fixture.
pub fn schemas() -> Vec<(&'static str, Schema)> {
    let mut out = Vec::new();
    let mut marks = Marks::new();
    both::<SampleValue>(
        &mut out,
        ["sample-value/enum", "sample-value/plain"],
        &SampleValue::fixture(&mut marks),
    );
    both::<SamplePick>(
        &mut out,
        ["sample-pick/enum", "sample-pick/plain"],
        &SamplePick::fixture(&mut marks),
    );
    both::<SamplePickSome>(
        &mut out,
        ["sample-pick-some/enum", "sample-pick-some/plain"],
        &SamplePickSome::fixture(&mut marks),
    );
    both::<SampleOrder>(
        &mut out,
        ["sample-order/enum", "sample-order/plain"],
        &SampleOrder::fixture(&mut marks),
    );
    both::<SampleNew>(
        &mut out,
        ["sample-new/enum", "sample-new/plain"],
        &SampleNew::fixture(&mut marks),
    );
    both::<SampleEach>(
        &mut out,
        ["sample-each/enum", "sample-each/plain"],
        &SampleEach::fixture(&mut marks),
    );
    both::<SampleFlagged>(
        &mut out,
        ["sample-flagged/enum", "sample-flagged/plain"],
        &SampleFlagged::fixture(&mut marks),
    );
    both::<SampleChange>(
        &mut out,
        ["sample-change/enum", "sample-change/plain"],
        &SampleChange::fixture(&mut marks),
    );
    out
}

/// A page of a tide table: material a request may not hold whole.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Page {
    pub number: usize,
    pub text: Told<String>,
}

impl Kind for Page {
    const PREFIX: &'static str = "p";
    const DRAWN: Drawn = Drawn::Name;
    type Ref = usize;

    fn reference(&self) -> usize {
        self.number
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Finding {
    pub text: String,
    pub pages: Vec<Picked<Page>>,
}

impl Authored for Finding {
    fn form() -> Form {
        Form::new().text("text").cite::<Page>("pages")
    }

    fn read(fields: &mut Fields<'_, '_>) -> Option<Self> {
        let text = fields.text("text")?;
        let pages = fields.cite::<Page>("pages");
        Some(Self { text, pages })
    }
}

/// A question and the pages that may answer it, the most useful first.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FitInput {
    pub question: Told<String>,
    pub pages: Packed<Page>,
}

impl FitInput {
    /// `pages` as (text, tokens showing it takes), numbered from 1 in the order given.
    pub fn of(question: &str, pages: &[(String, usize)]) -> Self {
        let held: Vec<Page> = pages
            .iter()
            .enumerate()
            .map(|(at, (text, _))| Page {
                number: at.saturating_add(1),
                text: Told(text.clone()),
            })
            .collect();
        let pages = Packed::new(held, |page| {
            pages
                .get(page.number.saturating_sub(1))
                .map_or(0, |(_, cost)| *cost)
        });
        Self {
            question: Told(question.to_owned()),
            pages,
        }
    }
}

pub struct SampleFit;

impl Contract for SampleFit {
    const ID: ContractId = ContractId::new("sample-fit");
    type Input = FitInput;
}

impl Answered for SampleFit {
    type Answer = Value<Finding>;
    const ALONE: Alone = Alone::Pending;

    fn says(input: &FitInput, showing: &mut Showing<'_>) -> Result<Says> {
        let held: Vec<Page> = input.pages.iter().cloned().collect();
        let mut listing = String::new();
        for (handle, page) in showing.show(&held)?.iter() {
            let _ = writeln!(listing, "{} {}", handle.bracketed(), page.text.0);
        }
        Ok(Says::new(
            "You answer questions from the tide tables of a harbour.",
            format!(
                "Answer the question from the pages.

Question: {}

{listing}",
                input.question.0
            ),
        ))
    }

    fn within(input: &FitInput, room: usize) -> Option<FitInput> {
        Some(FitInput {
            question: input.question.clone(),
            pages: input.pages.within(room),
        })
    }
}

/// Tools to judge one by one, beside pages that may not all fit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FitEachInput {
    pub tools: Vec<Tool>,
    pub pages: Packed<Page>,
}

pub struct SampleFitEach;

impl Contract for SampleFitEach {
    const ID: ContractId = ContractId::new("sample-fit-each");
    type Input = FitEachInput;
}

impl Answered for SampleFitEach {
    type Answer = Each<Tool, Verdict>;
    const ALONE: Alone = Alone::Pending;

    fn says(input: &FitEachInput, showing: &mut Showing<'_>) -> Result<Says> {
        let mut listing = String::new();
        for (handle, tool) in showing.show(&input.tools)?.iter() {
            let _ = writeln!(listing, "{} {}", handle.bracketed(), tool.name.0);
        }
        for page in input.pages.iter() {
            let _ = writeln!(listing, "{}", page.text.0);
        }
        Ok(Says::new(
            "You judge tools against the tide tables of a harbour.",
            format!(
                "Judge each tool by the pages.

{listing}"
            ),
        ))
    }

    fn within(input: &FitEachInput, room: usize) -> Option<FitEachInput> {
        Some(FitEachInput {
            tools: input.tools.clone(),
            pages: input.pages.within(room),
        })
    }
}
