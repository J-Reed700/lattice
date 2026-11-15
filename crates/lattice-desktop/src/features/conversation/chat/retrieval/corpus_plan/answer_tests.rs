//! The planner's contract: what it shows, and how its plan is read.

use std::collections::HashMap;

use futures::future::BoxFuture;
use tokio_util::sync::CancellationToken;

use lattice_core::application::contracts::search::CorpusDocument;
use lattice_core::application::ports::llm_port::{CompletionInput, CompletionResponse};
use lattice_core::application::services::model_contract::shown::Told;
use lattice_core::application::services::model_contract::test_support::taint::{holds, Marks};
use lattice_core::application::services::model_contract::test_support::{ScriptedModel, Sent};
use lattice_core::shared::error::Result;

use super::answer::{Correction, PlanContract, PlanInput, Row};
use super::health::PlannerHealth;
use super::{plan, CorpusSearchPlan, PlannerTurn};

fn harbour() -> Vec<CorpusDocument> {
    ["tide-tables", "kiln-log", "crane-manual"]
        .iter()
        .map(|id| CorpusDocument {
            id: (*id).into(),
            name: format!("{id} notes"),
            opening: format!("Harbour office {id} opening text"),
            chapter_number: None,
            source_context: None,
            sections: Vec::new(),
        })
        .collect()
}

fn user_turn(sent: &Sent) -> serde_json::Value {
    let text = sent
        .request
        .input()
        .iter()
        .rev()
        .find_map(|item| match item {
            CompletionInput::Message { role, content } if role == "user" => Some(content.as_str()),
            _ => None,
        })
        .unwrap_or_default();
    serde_json::from_str(text).unwrap_or_default()
}

/// The handle the request shows for the document whose name holds `name`.
fn handle_of(sent: &Sent, name: &str) -> String {
    user_turn(sent)["catalog"]
        .as_array()
        .and_then(|rows| {
            rows.iter()
                .find(|row| row["name"].as_str().is_some_and(|n| n.contains(name)))
        })
        .and_then(|row| row["id"].as_str())
        .unwrap_or_default()
        .to_owned()
}

fn reply(text: String) -> BoxFuture<'static, Result<CompletionResponse>> {
    Box::pin(std::future::ready(Ok(CompletionResponse::from_text(text))))
}

fn written(queries: &[&str], opening: &[String], start: &str) -> String {
    serde_json::json!({
        "queries": queries.iter().map(|q| serde_json::json!({ "query": q })).collect::<Vec<_>>(),
        "opening_document_ids": opening,
        "start_at_beginning": start,
    })
    .to_string()
}

async fn planned(model: &ScriptedModel) -> Result<CorpusSearchPlan> {
    let health = PlannerHealth::default();
    plan(
        model,
        "Where are the tide tables kept?",
        None,
        &harbour(),
        &HashMap::new(),
        &PlannerTurn::with_health(&health),
        &CancellationToken::new(),
    )
    .await
}

#[test]
fn the_request_tells_what_it_must_and_hides_the_identities() {
    let mut marks = Marks::new();
    let input = PlanInput {
        question: marks.told("Where are the tide tables kept?".to_owned()),
        history: marks.told("user: earlier question about harbour fees".to_owned()),
        catalog_total: Told(3),
        correction: Some(marks.told(Correction {
            queries_already_tried: vec!["harbour tide tables".into()],
            top_result_titles: vec!["Quarterly harbour revenue".into()],
            why_insufficient: vec!["low_term_coverage".into()],
            more_titles_not_shown: 0,
        })),
        documents: vec![Row {
            id: marks.reference("0f9e8d7c-aaaa-bbbb-cccc-111122223333".to_owned()),
            name: marks.told("Tide tables 2026".to_owned()),
            opening: marks.told("Tide tables for the outer harbour".to_owned()),
            chapter_number: Some(Told(4)),
            source_context: None,
            sections: marks.told(vec!["Spring tides".to_owned()]),
            summary: Some(marks.told("Heights and times of high water".to_owned())),
        }],
    };
    holds::<PlanContract>(&input, &marks);
}

#[tokio::test]
async fn a_plan_names_documents_by_the_handles_it_was_shown() {
    let model = ScriptedModel::new().on_request::<PlanContract>(|sent, _| {
        let opening = [handle_of(sent, "tide-tables")];
        reply(written(&["tide tables"], &opening, "false"))
    });

    let plan = planned(&model).await.unwrap();

    assert_eq!(plan.queries, ["tide tables"]);
    assert_eq!(plan.opening_document_ids, ["tide-tables"]);
    assert!(!plan.start_at_beginning);
    assert_eq!(model.count::<PlanContract>(), 1);
}

/// A handle that names nothing shown costs only that pointer.
#[tokio::test]
async fn a_handle_that_names_nothing_shown_costs_only_that_pointer() {
    let model = ScriptedModel::new().on_request::<PlanContract>(|sent, _| {
        let opening = [handle_of(sent, "kiln-log"), "d99".to_owned()];
        reply(written(&["kiln log"], &opening, "false"))
    });

    let plan = planned(&model).await.unwrap();

    assert_eq!(plan.opening_document_ids, ["kiln-log"]);
    assert_eq!(model.count::<PlanContract>(), 1);
}

/// A word outside the two the flag may hold is asked for again once.
#[tokio::test]
async fn a_flag_written_in_other_words_is_corrected_once() {
    let model = ScriptedModel::new().on_request::<PlanContract>(|_, nth| {
        let flag = if nth == 0 { "yes" } else { "false" };
        reply(written(&["crane manual"], &[], flag))
    });

    let plan = planned(&model).await.unwrap();

    assert_eq!(plan.queries, ["crane manual"]);
    assert_eq!(model.count::<PlanContract>(), 2);
}

/// An answer no reading can use is the caller's to fall back from.
#[tokio::test]
async fn an_answer_that_stays_unreadable_is_an_error_after_one_correction() {
    let model = ScriptedModel::new()
        .on_request::<PlanContract>(|_, _| reply("The tide tables are in the harbour office.".into()));

    assert!(planned(&model).await.is_err());
    assert_eq!(model.count::<PlanContract>(), 2);
}
