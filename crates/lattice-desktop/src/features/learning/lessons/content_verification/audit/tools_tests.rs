//! The Verifier's `search_web` takes the same typed need every research
//! planner writes: rewordings, a newer-than date, a dated read and a known
//! work's cited sources reach the search, a query naming several sites is
//! one search per site, and a need outside the schema is refused without
//! searching.
#![allow(clippy::unwrap_used, clippy::indexing_slicing)]

use std::sync::Mutex;

use lattice_core::domain::sourcing::NeedKind;
use lattice_core::shared::error::AppError;

use super::*;

/// Records every need the Verifier searches for.
#[derive(Default)]
struct Recording {
    needs: Mutex<Vec<PlannedNeed>>,
}

#[async_trait]
impl VerifierWeb for Recording {
    async fn search(&self, need: &PlannedNeed, _room: Room<'_>) -> Result<Vec<SearchHit>> {
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
    run(SEARCH, &call, &web, Room::for_all(), &mut answer).await;
    let references = json!({
        "query": "10.48550/arXiv.1706.03762",
        "kind": "leads",
        "identifier": "10.48550/arXiv.1706.03762"
    });
    run(SEARCH, &references, &web, Room::for_all(), &mut answer).await;
    let undated =
        json!({"query": "python.org downloads page", "kind": "web", "asOf": "2019-03-01"});
    let refused = run(SEARCH, &undated, &web, Room::for_all(), &mut answer).await;
    assert!(refused.starts_with("Search not run"));

    let needs = web.needs.lock().unwrap();
    assert_eq!(needs.len(), 2, "the refused call never searched");
    assert_eq!(needs[0].as_of.as_deref(), Some("2019-03-01"));
    assert_eq!(needs[0].alternatives, ["Python download page 2019"]);
    assert_eq!(needs[1].kind, NeedKind::Leads);
}

/// Answers each site's search with one page on it, and fails the sites in
/// `down`, as a provider failing for one scoped search would.
struct PerSite {
    down: Vec<&'static str>,
    searched: Mutex<Vec<(Option<String>, String)>>,
}

#[async_trait]
impl VerifierWeb for PerSite {
    async fn search(&self, need: &PlannedNeed, _room: Room<'_>) -> Result<Vec<SearchHit>> {
        self.searched
            .lock()
            .unwrap()
            .push((need.site.clone(), need.query.clone()));
        let site = need.site.clone().unwrap_or_default();
        if self.down.contains(&site.as_str()) {
            return Err(AppError::Network(format!("{site} is down")));
        }
        Ok(vec![SearchHit {
            title: format!("{site} page"),
            url: format!("https://{site}/page"),
            snippet: String::new(),
        }])
    }
    async fn save_page(&self, _: &str) -> Result<AuditPassage> {
        unreachable!("these tests only search")
    }
}

#[tokio::test]
async fn a_query_naming_two_sites_searches_each_and_fails_only_when_both_do() {
    let call = json!({
        "query": "site:platform.openai.com OR site:docs.anthropic.com tool result error handling",
        "kind": "official_docs"
    });

    let one_down = PerSite {
        down: vec!["platform.openai.com"],
        searched: Mutex::new(Vec::new()),
    };
    let mut answer = Answer::default();
    let result = run(SEARCH, &call, &one_down, Room::for_all(), &mut answer).await;
    assert_eq!(
        *one_down.searched.lock().unwrap(),
        [
            (
                Some("platform.openai.com".to_owned()),
                "tool result error handling".to_owned()
            ),
            (
                Some("docs.anthropic.com".to_owned()),
                "tool result error handling".to_owned()
            ),
        ]
    );
    let hits: Value = serde_json::from_str(&result).unwrap();
    assert_eq!(hits.as_array().unwrap().len(), 1, "{result}");
    assert!(
        result.contains("https://docs.anthropic.com/page"),
        "{result}"
    );
    assert_eq!(answer.web_note, None, "web search was used");

    let both_down = PerSite {
        down: vec!["platform.openai.com", "docs.anthropic.com"],
        searched: Mutex::new(Vec::new()),
    };
    let mut answer = Answer::default();
    let result = run(SEARCH, &call, &both_down, Room::for_all(), &mut answer).await;
    assert!(result.starts_with("Search failed"), "{result}");
    assert!(answer.web_note.is_some());
}
