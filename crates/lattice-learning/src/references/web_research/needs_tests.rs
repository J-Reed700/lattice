//! The search planner of a lesson's research: what it is told of its gaps and what its answer may point at.
#![allow(clippy::unwrap_used, clippy::indexing_slicing)]
use super::*;
use crate::lessons::structured_call::task_messages;
use futures::future::BoxFuture;
use lattice_core::application::ports::llm_port::CompletionResponse;
use lattice_core::application::services::model_contract::test_support::taint::{holds, Marks};
use lattice_core::application::services::model_contract::test_support::{ScriptedModel, Sent};
use lattice_core::shared::error::{AppError, Result};

fn reply(text: String) -> BoxFuture<'static, Result<CompletionResponse>> {
    Box::pin(std::future::ready(Ok(CompletionResponse::from_text(text))))
}

fn task(sent: &Sent) -> Value {
    serde_json::from_str(task_messages(&sent.request).1).unwrap()
}

fn gaps() -> Vec<Value> {
    vec![
        json!({"claim": "A harbour gate opens at first light", "missingEvidence": "the harbour master's notice", "subject": "Harbour gate"}),
        json!({"claim": "Tide tables are revised each spring", "missingEvidence": "a tide table"}),
    ]
}

#[test]
fn what_the_planner_reads_is_told_and_what_the_app_keeps_is_not() {
    let mut marks = Marks::new();
    let input = NeedsInput {
        topic: marks.told("the harbour gate and its tide tables".to_owned()),
        gaps: vec![Gap {
            number: marks.reference(100_007_usize),
            body: marks.told(json!({"claim": "A harbour gate opens at first light"})),
        }],
        existing_sources: marks.told(json!([{"title": "A harbour master's handbook"}])),
        already_searched: marks.told(vec!["tide table revision schedule".to_owned()]),
    };
    holds::<ReferenceNeeds>(&input, &marks);
}

#[tokio::test]
async fn a_gap_is_shown_by_its_handle_and_a_search_points_at_it() -> Result<()> {
    let model = ScriptedModel::new().on_request::<ReferenceNeeds>(|sent, _| {
        let task = task(sent);
        assert_eq!(task["gaps"][0]["gap"], "g1");
        assert_eq!(task["gaps"][1]["gap"], "g2");
        reply(
            needs_answer(json!([
                {"gap": 1, "query": "tide table revision schedule", "kind": "web"},
                {"gap": 0, "query": "harbour gate opening times", "kind": "official_docs"}
            ]))
            .to_string(),
        )
    });
    let planned = plan_needs(&model, "harbour", &gaps(), json!([]), &[]).await?;
    let served: Vec<(usize, &str)> = planned
        .iter()
        .map(|served| (served.gap, served.need.query.as_str()))
        .collect();
    assert_eq!(
        served,
        [
            (1, "tide table revision schedule"),
            (0, "harbour gate opening times")
        ]
    );
    assert_eq!(
        planned[1].need.subject.as_deref(),
        Some("Harbour gate"),
        "the gap's subject"
    );
    Ok(())
}

#[tokio::test]
async fn a_search_that_points_at_no_gap_is_left_out_alone() -> Result<()> {
    let model = ScriptedModel::new().on_request::<ReferenceNeeds>(|_, _| {
        reply(
            needs_answer(json!([
                {"gap": 4, "query": "a search for nothing shown", "kind": "web"},
                {"gap": 0, "query": "harbour gate opening times", "kind": "web"}
            ]))
            .to_string(),
        )
    });
    let planned = plan_needs(&model, "harbour", &gaps(), json!([]), &[]).await?;
    assert_eq!(planned.len(), 1);
    assert_eq!(planned[0].need.query, "harbour gate opening times");
    Ok(())
}

#[tokio::test]
async fn a_search_outside_what_a_search_allows_is_left_out_and_the_others_stand() -> Result<()> {
    let model = ScriptedModel::new().on_request::<ReferenceNeeds>(|_, _| {
        reply(
            needs_answer(json!([
                {"gap": 0, "query": "harbour gate opening times", "kind": "web", "after": "last spring"},
                {"gap": 1, "query": "tide table revision schedule", "kind": "web"}
            ]))
            .to_string(),
        )
    });
    let planned = plan_needs(&model, "harbour", &gaps(), json!([]), &[]).await?;
    assert_eq!(planned.len(), 1);
    assert_eq!(planned[0].need.query, "tide table revision schedule");
    Ok(())
}

#[test]
fn a_continued_request_lists_what_stands_as_already_searched() {
    let input = NeedsInput {
        topic: Told("harbour".into()),
        gaps: Vec::new(),
        existing_sources: Told(json!([])),
        already_searched: Told(vec!["tide table revision schedule".to_owned()]),
    };
    let standing = [GapNeed {
        gap: 0,
        need: PlannedNeed {
            site: Some("harbour.example.org".into()),
            ..PlannedNeed::web("harbour gate opening times".into())
        },
    }];
    assert_eq!(
        input.continued(&standing).already_searched.0,
        [
            "tide table revision schedule",
            "site:harbour.example.org harbour gate opening times"
        ]
    );
}

#[test]
fn the_kinds_the_planner_may_write_are_the_kinds_a_search_may_ask_for() {
    let written: Vec<NeedKind> = PlannedKind::WORDS.iter().map(|(_, kind)| kind.0).collect();
    assert_eq!(written, PLANNED_KINDS);
    for (word, kind) in PlannedKind::WORDS {
        assert_eq!(*word, kind.0.as_str());
    }
}

#[tokio::test]
async fn items_written_and_none_usable_fail_the_research() {
    let model = ScriptedModel::new().on_request::<ReferenceNeeds>(|_, _| {
        reply(
            needs_answer(json!([
                {"gap": 0, "query": "harbour gate opening times", "kind": "web", "after": "last spring"}
            ]))
            .to_string(),
        )
    });
    let error = plan_needs(&model, "harbour", &gaps(), json!([]), &[])
        .await
        .unwrap_err();
    assert!(
        matches!(error, AppError::ModelAnswerUnusable(_)),
        "{error:?}"
    );
}

#[tokio::test]
async fn no_item_written_means_no_research_is_needed() -> Result<()> {
    let model = ScriptedModel::new()
        .on_request::<ReferenceNeeds>(|_, _| reply(needs_answer(json!([])).to_string()));
    let planned = plan_needs(&model, "harbour", &gaps(), json!([]), &[]).await?;
    assert!(planned.is_empty());
    Ok(())
}
