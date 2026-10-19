//! Planned searches are typed needs: each states its kind, takes a subject
//! only from the gap it serves, and keeps to a host only when the planner
//! set one.

use std::sync::Mutex;

use lattice_core::domain::sourcing::NeedKind;
use lattice_core::shared::error::Result;
use serde_json::json;

use super::web_research::{need_schema, plan_needs, PlannedNeed};
use crate::features::learning::lessons::course_generation_tests::ScriptedModel;

fn scripted(outputs: Vec<String>) -> ScriptedModel {
    ScriptedModel {
        outputs: Mutex::new(outputs.into()),
        prompts: Mutex::new(Vec::new()),
    }
}

fn need(query: &str, kind: NeedKind) -> PlannedNeed {
    PlannedNeed {
        kind,
        ..PlannedNeed::web(query.into())
    }
}

#[test]
fn a_need_is_checked_against_what_a_search_allows() {
    let trimmed = PlannedNeed {
        subject: Some("  ".into()),
        ecosystem: Some(" pypi ".into()),
        ..need("  requests timeout  ", NeedKind::Package)
    }
    .checked()
    .unwrap();
    assert_eq!(trimmed.query, "requests timeout");
    assert_eq!(trimmed.subject, None);
    assert_eq!(trimmed.ecosystem.as_deref(), Some("pypi"));

    assert_eq!(need("ab", NeedKind::Web).checked(), None, "too short");
    assert_eq!(
        need("an archived page", NeedKind::Archive).checked(),
        None,
        "an archived copy starts from an address"
    );
    assert_eq!(need("a page's links", NeedKind::Leads).checked(), None);
    let unknown_registry = PlannedNeed {
        ecosystem: Some("cpan".into()),
        ..need("Moose", NeedKind::Package)
    };
    assert_eq!(unknown_registry.checked(), None);
}

#[test]
fn only_a_plan_for_gaps_names_the_gap_each_search_serves() {
    let course = need_schema(None);
    assert_eq!(course["required"], json!(["query", "kind"]));
    assert!(course["properties"].get("gap").is_none());

    let lesson = need_schema(Some(3));
    assert_eq!(lesson["required"], json!(["query", "kind", "gap"]));
    assert_eq!(lesson["properties"]["gap"]["maximum"], json!(2));
    let kinds = lesson["properties"]["kind"]["enum"].as_array().unwrap();
    assert!(kinds.contains(&json!("official_docs")));
    assert!(!kinds.contains(&json!("archive")));
}

#[tokio::test]
async fn a_need_takes_its_subject_from_the_gap_it_serves_and_no_host_it_was_not_given() -> Result<()>
{
    let gaps = [
        json!({"claim": "ExecutionContext flows across await points", "missingEvidence": "official docs",
               "kind": "specification", "subject": ".NET", "ownerOriginHints": ["learn.microsoft.com"],
               "needsIndependentSource": true}),
        json!({"claim": "Self-attention cost grows with the square of the sequence length",
               "missingEvidence": "a paper", "kind": "measurement", "subject": "Transformer"}),
    ];
    let llm = scripted(vec![json!({"needs": [
        {"gap": 0, "query": "ExecutionContext flow across await", "kind": "official_docs", "site": "learn.microsoft.com"},
        {"gap": 0, "query": "ExecutionContext AsyncLocal flow analysis", "kind": "web"},
        {"gap": 1, "query": "self-attention quadratic sequence length", "kind": "paper", "subject": "attention"}
    ]})
    .to_string()]);
    let needs = plan_needs(&llm, "async .NET", &gaps, json!([]), &[]).await?;
    let routed: Vec<_> = needs
        .iter()
        .map(|need| (need.kind, need.subject.as_deref(), need.site.as_deref()))
        .collect();
    assert_eq!(
        routed,
        [
            (
                NeedKind::OfficialDocs,
                Some(".NET"),
                Some("learn.microsoft.com")
            ),
            // The independent search keeps no host: owner hints are the
            // planner's to use, never a filter added behind it.
            (NeedKind::Web, Some(".NET"), None),
            // The planner's own subject wins over the gap's.
            (NeedKind::Paper, Some("attention"), None),
        ]
    );
    Ok(())
}

#[tokio::test]
async fn a_search_for_a_gap_the_planner_was_not_given_is_refused() {
    let gaps = [json!({"claim": "c", "missingEvidence": "m"})];
    let llm = scripted(vec![json!({"needs": [
        {"gap": 1, "query": "some public search", "kind": "web"}
    ]})
    .to_string()]);
    assert!(plan_needs(&llm, "topic", &gaps, json!([]), &[])
        .await
        .is_err());
}

#[tokio::test]
async fn no_gaps_plan_no_searches_without_asking_the_model() -> Result<()> {
    let llm = scripted(Vec::new());
    assert!(plan_needs(&llm, "topic", &[], json!([]), &[])
        .await?
        .is_empty());
    assert!(llm.prompts.lock().unwrap().is_empty());
    Ok(())
}
