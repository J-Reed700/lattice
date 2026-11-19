//! The corpus planner's contract: the request it is shown and the plan it writes.

use serde::{Deserialize, Serialize};

use lattice_core::application::services::model_contract::ask::{Alone, Answered, Says};
use lattice_core::application::services::model_contract::shape::{
    Authored, Count, Fields, Form, Value, Vocabulary,
};
use lattice_core::application::services::model_contract::shown::{
    Drawn, Kind, Picked, Showing, Told,
};
use lattice_core::application::services::model_contract::{Contract, ContractId};
use lattice_core::domain::value_objects::source_context::SourceContext;
use lattice_core::shared::error::Result;

use super::planner::CorrectionRequest;
use super::prompt::PLANNER_OUTPUT_TOKENS;
use super::{CORRECTION_INSTRUCTION, MAX_QUERIES, PLANNER_SYSTEM};

pub(super) struct PlanContract;

const QUERIES: Count = Count("queries");

impl Contract for PlanContract {
    const ID: ContractId = ContractId::new("corpus-plan");
    type Input = PlanInput;
}

/// What the first retrieval pass already tried, as the planner reads it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct Correction {
    pub queries_already_tried: Vec<String>,
    pub top_result_titles: Vec<String>,
    pub why_insufficient: Vec<String>,
    pub more_titles_not_shown: usize,
}

impl Correction {
    pub(super) fn of(request: &CorrectionRequest) -> Self {
        Self {
            queries_already_tried: request.queries_already_tried.clone(),
            top_result_titles: request.top_result_titles.clone(),
            why_insufficient: request
                .why_insufficient
                .iter()
                .map(|reason| (*reason).to_owned())
                .collect(),
            more_titles_not_shown: request.more_titles_not_shown,
        }
    }
}

/// One catalog row, in the order shown. Its id is the app's, never written to the planner.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct Row {
    pub id: String,
    pub name: Told<String>,
    pub opening: Told<String>,
    pub chapter_number: Option<Told<u32>>,
    pub source_context: Option<Told<SourceContext>>,
    pub sections: Told<Vec<String>>,
    pub summary: Option<Told<String>>,
}

impl Kind for Row {
    const PREFIX: &'static str = "d";
    const DRAWN: Drawn = Drawn::Itself;
    type Ref = String;

    fn reference(&self) -> String {
        self.id.clone()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct PlanInput {
    pub question: Told<String>,
    pub history: Told<String>,
    pub catalog_total: Told<usize>,
    pub correction: Option<Told<Correction>>,
    pub documents: Vec<Row>,
}

#[derive(Serialize)]
struct CatalogRow<'a> {
    id: String,
    name: &'a str,
    opening: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    chapter_number: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    source_context: Option<&'a SourceContext>,
    sections: &'a [String],
    #[serde(skip_serializing_if = "Option::is_none")]
    summary: Option<&'a str>,
}

/// `row` as the catalog shows it under `handle`.
pub(super) fn catalog_json(row: &Row, handle: &str) -> Result<String> {
    Ok(serde_json::to_string(&catalog_row(row, handle))?)
}

fn catalog_row<'a>(row: &'a Row, handle: &str) -> CatalogRow<'a> {
    CatalogRow {
        id: handle.to_owned(),
        name: &row.name.0,
        opening: &row.opening.0,
        chapter_number: row.chapter_number.as_ref().map(|number| number.0),
        source_context: row.source_context.as_ref().map(|context| &context.0),
        sections: &row.sections.0,
        summary: row.summary.as_ref().map(|summary| summary.0.as_str()),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Beginning {
    Yes,
    No,
}

impl Vocabulary for Beginning {
    const WORDS: &'static [(&'static str, Self)] =
        &[("true", Beginning::Yes), ("false", Beginning::No)];
}

/// One concise search string.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Query(pub String);

impl Authored for Query {
    fn form() -> Form {
        Form::new().text("query")
    }

    fn read(fields: &mut Fields<'_, '_>) -> Option<Self> {
        Some(Self(fields.text("query")?))
    }
}

/// What the planner wrote: queries, the documents it wants opened, and whether to read from the start.
#[derive(Debug, Clone)]
pub(super) struct Plan {
    pub queries: Vec<Query>,
    pub opening: Vec<Picked<Row>>,
    pub start_at_beginning: bool,
}

impl Authored for Plan {
    fn form() -> Form {
        Form::new()
            .counted::<Query>("queries", &QUERIES)
            .cite::<Row>("opening_document_ids")
            .word::<Beginning>("start_at_beginning")
    }

    fn read(fields: &mut Fields<'_, '_>) -> Option<Self> {
        let queries = fields.list::<Query>("queries")?;
        let opening = fields.cite::<Row>("opening_document_ids");
        let start = fields.word::<Beginning>("start_at_beginning")?;
        Some(Self {
            queries,
            opening,
            start_at_beginning: start == Beginning::Yes,
        })
    }
}

impl Answered for PlanContract {
    type Answer = Value<Plan>;
    const ALONE: Alone = Alone::Fails;

    fn says(input: &PlanInput, showing: &mut Showing<'_>) -> Result<Says> {
        showing.asks(&QUERIES, 1..=MAX_QUERIES);
        let shown = showing.show(&input.documents)?;
        let rows = shown
            .iter()
            .map(|(handle, row)| catalog_row(row, &handle.written()))
            .collect::<Vec<_>>();
        let mut payload = serde_json::json!({
            "request": input.question.0,
            "recent_conversation": input.history.0,
            "catalog_total": input.catalog_total.0,
            "catalog": rows,
        });
        if let (Some(correction), Some(payload)) = (&input.correction, payload.as_object_mut()) {
            let correction = &correction.0;
            payload.insert(
                "correction".into(),
                serde_json::json!({
                    "instruction": CORRECTION_INSTRUCTION,
                    "queries_already_tried": correction.queries_already_tried,
                    "top_result_titles": correction.top_result_titles,
                    "more_titles_not_shown": correction.more_titles_not_shown,
                    "why_insufficient": correction.why_insufficient,
                }),
            );
        }
        let mut says = Says::new(PLANNER_SYSTEM, serde_json::to_string(&payload)?);
        says.output_tokens = PLANNER_OUTPUT_TOKENS as usize;
        Ok(says)
    }
}
