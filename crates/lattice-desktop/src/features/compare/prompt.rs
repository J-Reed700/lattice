//! The single per-document task. One call fills every column; the passages
//! follow it under `Passages:`, as many as the model's window holds.

use lattice_core::application::ports::llm_port::tokens_for_chars;
use lattice_core::application::services::grounded_generation::EvidencePassage;
use lattice_core::application::services::model_contract::ask::{Alone, Answered, Refusal, Says};
use lattice_core::application::services::model_contract::packed::Packed;
use lattice_core::application::services::model_contract::shape::{
    Authored, Each, Fields, Form, Union,
};
use lattice_core::application::services::model_contract::shown::{
    Drawn, Kind, Picked, Showing, Told,
};
use lattice_core::application::services::model_contract::{Contract, ContractId};
use lattice_core::shared::error::Result;
use lattice_core::shared::text::sentences;
use serde::{Deserialize, Serialize};

use super::retrieval::RetrievedChunk;

/// Heading the passages are written under, after the task.
pub const PASSAGES_HEADING: &str = "Passages:";

/// Answer room one cell takes: a short value, the sentences cited for it and
/// the JSON around them.
pub const CELL_OUTPUT_TOKENS: usize = 128;

/// The call that fills the columns of one document's row.
pub(super) struct CompareRow;

impl Contract for CompareRow {
    const ID: ContractId = ContractId::new("compare-row");
    type Input = Input;
}

/// One column of the table, as the model names it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct Column {
    pub(super) index: usize,
    pub(super) name: Told<String>,
}

impl Kind for Column {
    const PREFIX: &'static str = "f";
    const DRAWN: Drawn = Drawn::Name;
    type Ref = usize;

    fn reference(&self) -> usize {
        self.index
    }
}

/// Which sentence of which chunk.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct SentenceRef {
    pub(super) chunk_id: String,
    pub(super) at: usize,
}

/// One sentence of a retrieved passage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct Sentence {
    pub(super) chunk_id: String,
    pub(super) at: usize,
    pub(super) text: Told<String>,
}

impl Kind for Sentence {
    const PREFIX: &'static str = "s";
    const DRAWN: Drawn = Drawn::Itself;
    type Ref = SentenceRef;

    fn reference(&self) -> SentenceRef {
        SentenceRef {
            chunk_id: self.chunk_id.clone(),
            at: self.at,
        }
    }
}

/// A retrieved chunk as its sentences.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct Passage {
    pub(super) sentences: Vec<Sentence>,
}

fn one_line(text: &str) -> String {
    text.replace(['\r', '\n'], " ")
}

impl Passage {
    fn of(chunk: &RetrievedChunk) -> Self {
        let sentences = sentences(&chunk.content)
            .into_iter()
            .filter_map(|(start, stop)| chunk.content.get(start..stop))
            .map(str::trim)
            .filter(|sentence| !sentence.is_empty())
            .enumerate()
            .map(|(at, sentence)| Sentence {
                chunk_id: chunk.chunk_id.clone(),
                at,
                text: Told(one_line(sentence)),
            })
            .collect();
        Self { sentences }
    }
}

/// What is asked about the columns still open.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct Input {
    pub(super) title: Told<String>,
    pub(super) columns: Vec<Column>,
    pub(super) passages: Packed<Passage>,
}

impl Input {
    /// The chunks as passages, the best ranked first, each with the tokens showing it takes.
    pub(super) fn passages_of(chunks: &[RetrievedChunk], chars_per_token: f64) -> Packed<Passage> {
        Packed::new(chunks.iter().map(Passage::of).collect(), |passage| {
            let chars = passage
                .sentences
                .iter()
                .map(|sentence| sentence.text.0.chars().count())
                .sum();
            tokens_for_chars(chars, chars_per_token)
        })
    }
}

/// What the model answered for one column.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum Answer {
    Stated {
        value: String,
        support: Vec<Picked<Sentence>>,
    },
    NotStated,
}

impl Union for Answer {
    fn forms() -> Vec<(&'static str, Form)> {
        vec![
            (
                "stated",
                Form::new().text("value").cite::<Sentence>("support"),
            ),
            ("not-stated", Form::new()),
        ]
    }

    fn read(kind: &str, fields: &mut Fields<'_, '_>) -> Option<Self> {
        match kind {
            "stated" => {
                let value = fields.text("value")?;
                let support = fields.cite::<Sentence>("support");
                Some(Self::Stated { value, support })
            }
            "not-stated" => Some(Self::NotStated),
            _ => None,
        }
    }
}

/// One cell of the row.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Cell {
    pub(super) answer: Answer,
}

impl Authored for Cell {
    fn form() -> Form {
        Form::new().one_of::<Answer>("answer")
    }

    fn read(fields: &mut Fields<'_, '_>) -> Option<Self> {
        Some(Self {
            answer: fields.one_of::<Answer>("answer")?,
        })
    }
}

impl Answered for CompareRow {
    type Answer = Each<Column, Cell>;
    const ALONE: Alone = Alone::Pending;

    fn says(input: &Input, showing: &mut Showing<'_>) -> Result<Says> {
        let held: Vec<Sentence> = input
            .passages
            .iter()
            .flat_map(|passage| passage.sentences.iter().cloned())
            .collect();
        let columns = showing.show(&input.columns)?;
        let shown = showing.show(&held)?;
        let fields: Vec<String> = columns
            .iter()
            .map(|(handle, column)| format!("{} {}", handle.bracketed(), column.name.0))
            .collect();
        let mut rows = shown.iter();
        let evidence: Vec<EvidencePassage> = input
            .passages
            .iter()
            .enumerate()
            .filter_map(|(index, passage)| {
                let text = passage
                    .sentences
                    .iter()
                    .filter_map(|_| rows.next())
                    .map(|(handle, sentence)| {
                        format!("{} {}", handle.bracketed(), sentence.text.0)
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                let id = passage.sentences.first()?.chunk_id.clone();
                Some(EvidencePassage {
                    id,
                    label: String::new(),
                    text,
                    rank: 1.0 / (index + 1) as f32,
                })
            })
            .collect();
        let mut says = Says::new("", build_compare_task(&input.title.0, &fields));
        says.evidence_heading = PASSAGES_HEADING.to_string();
        says.evidence = evidence;
        says.output_tokens = input.columns.len().saturating_mul(CELL_OUTPUT_TOKENS);
        Ok(says)
    }

    fn accept(_input: &Input, cell: &Cell) -> std::result::Result<(), Refusal> {
        match &cell.answer {
            Answer::Stated { value, .. } if value.trim().is_empty() => Err(Refusal(
                "The value is empty. Give a short answer, or say it is not stated.".to_string(),
            )),
            _ => Ok(()),
        }
    }

    fn within(input: &Input, room: usize) -> Option<Input> {
        Some(Input {
            title: input.title.clone(),
            columns: input.columns.clone(),
            passages: input.passages.within(room),
        })
    }
}

/// `fields` are the columns, each already written with its handle.
pub fn build_compare_task(title: &str, fields: &[String]) -> String {
    let field_list = fields.join("\n");

    [
        "You are filling one row of a comparison table about a single document.".to_string(),
        String::new(),
        format!("Document title: {title}"),
        String::new(),
        "Fill exactly these fields, using only the passages below:".to_string(),
        field_list,
        String::new(),
        "Respond with a single JSON object and nothing else. No prose before or after, no"
            .to_string(),
        "markdown fences. The object has an \"entries\" list with one entry per field: \"of\" is the"
            .to_string(),
        "handle shown beside the field, and \"answer\" is an object whose \"kind\" is \"stated\" or"
            .to_string(),
        "\"not-stated\". A \"stated\" answer has two more keys:".to_string(),
        "  \"value\": a short factual answer (at most 25 words)".to_string(),
        "  \"support\": the handles of the sentences of the passages that state it".to_string(),
        String::new(),
        "Rules:".to_string(),
        "- If the passages do not state the answer, answer \"not-stated\". Do".to_string(),
        "  not guess, do not infer, do not answer from general knowledge.".to_string(),
        "- Keep \"value\" plain text: no markdown, no citations, no brackets.".to_string(),
    ]
    .join("\n")
}
