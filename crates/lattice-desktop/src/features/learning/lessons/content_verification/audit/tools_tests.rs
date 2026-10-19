//! The Verifier's `search_web` takes the same typed need every research
//! planner writes: rewordings, a newer-than date, a dated read and a known
//! work's cited sources reach the search, and a need outside the schema is
//! refused without searching.
#![allow(clippy::unwrap_used, clippy::indexing_slicing)]

use std::sync::Mutex;

use lattice_core::domain::sourcing::NeedKind;

use super::*;

/// Records every need the Verifier searches for.
#[derive(Default)]
struct Recording {
    needs: Mutex<Vec<PlannedNeed>>,
}

#[async_trait]
impl VerifierWeb for Recording {
    async fn search(&self, need: &PlannedNeed) -> Result<Vec<SearchHit>> {
        self.needs.lock().unwrap().push(need.clone());
        Ok(Vec::new())
    }
    async fn save_page(&self, _: &str) -> Result<AuditPassage> {
        unreachable!("these tests only search")
    }
}

#[test]
fn the_search_tool_offers_every_planned_field() {
    let definitions = definitions();
    let search = &definitions[0];
    assert_eq!(search.name, SEARCH);
    let properties = &search.parameters["properties"];
    for field in ["alternatives", "after", "asOf"] {
        assert!(properties.get(field).is_some(), "{field} missing");
    }
    assert!(properties["kind"]["enum"]
        .as_array()
        .unwrap()
        .contains(&json!("leads")));
    assert!(search.description.contains("asOf"));
}

#[tokio::test]
async fn a_search_call_carries_its_rewordings_and_dates_or_is_refused() {
    let web = Recording::default();
    let mut answer = Answer::default();
    let call = json!({
        "query": "python.org downloads page",
        "kind": "web",
        "identifier": "https://www.python.org/downloads/",
        "asOf": "2019-03-01",
        "alternatives": ["Python download page 2019"]
    });
    run(SEARCH, &call, &web, &mut answer).await;
    let references = json!({
        "query": "10.48550/arXiv.1706.03762",
        "kind": "leads",
        "identifier": "10.48550/arXiv.1706.03762"
    });
    run(SEARCH, &references, &web, &mut answer).await;
    let undated =
        json!({"query": "python.org downloads page", "kind": "web", "asOf": "2019-03-01"});
    let refused = run(SEARCH, &undated, &web, &mut answer).await;
    assert!(refused.starts_with("Search not run"));

    let needs = web.needs.lock().unwrap();
    assert_eq!(needs.len(), 2, "the refused call never searched");
    assert_eq!(needs[0].as_of.as_deref(), Some("2019-03-01"));
    assert_eq!(needs[0].alternatives, ["Python download page 2019"]);
    assert_eq!(needs[1].kind, NeedKind::Leads);
}
