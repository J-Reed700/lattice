//! The Verifier's `search_web` takes the same typed need every research
//! planner writes: rewordings, a newer-than date, a dated read and a known
//! work's cited sources reach the search, a query naming several sites is
//! one search per site, and a need outside the schema is refused without
//! searching. `fetch_page` takes a search result's handle, and a handle it
//! was not shown is answered in words.

use std::sync::Mutex;

use lattice_core::application::services::model_contract::test_support::taint::{tool_holds, Marks};
use lattice_core::domain::sourcing::NeedKind;
use lattice_core::shared::error::AppError;

use super::*;
use crate::lessons::verifier_audit::tests::ScriptedVerifier;
use serde_json::json;

/// Runs one tool call as a round of the research does.
async fn call(
    web: &dyn VerifierWeb,
    name: &str,
    arguments: &Value,
    scope: &mut Scope,
    research: &mut Research,
) -> String {
    let llm = ScriptedVerifier::answering(200_000, |_, _| json!({}));
    let tool = Call {
        llm: &llm,
        web,
        room: Room::for_all(),
        share: 10_000,
    };
    tool.run(name, arguments, scope, research).await
}

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
    let (mut scope, mut research) = (Scope::for_question(), Research::default());
    let need = json!({
        "query": "python.org downloads page",
        "kind": "web",
        "identifier": "https://www.python.org/downloads/",
        "asOf": "2019-03-01",
        "alternatives": ["Python download page 2019"]
    });
    call(&web, SEARCH, &need, &mut scope, &mut research).await;
    let references = json!({
        "query": "10.48550/arXiv.1706.03762",
        "kind": "leads",
        "identifier": "10.48550/arXiv.1706.03762"
    });
    call(&web, SEARCH, &references, &mut scope, &mut research).await;
    let undated =
        json!({"query": "python.org downloads page", "kind": "web", "asOf": "2019-03-01"});
    let refused = call(&web, SEARCH, &undated, &mut scope, &mut research).await;
    assert!(refused.starts_with("Search not run"));

    let needs = web.needs.lock().unwrap();
    assert_eq!(needs.len(), 2, "the refused call never searched");
    assert_eq!(needs[0].as_of.as_deref(), Some("2019-03-01"));
    assert_eq!(needs[0].alternatives, ["Python download page 2019"]);
    assert_eq!(needs[1].kind, NeedKind::Leads);
}

#[tokio::test]
async fn a_search_is_refused_only_when_no_terms_are_left_once_its_sites_are_lifted_out() {
    let web = Recording::default();
    let (mut scope, mut research) = (Scope::for_question(), Research::default());
    for need in [
        json!({"query": "   ", "kind": "web"}),
        json!({"query": "site:docs.python.org", "kind": "official_docs"}),
        json!({"query": "tool result error handling", "kind": "web", "alternatives": ["  "]}),
    ] {
        let refused = call(&web, SEARCH, &need, &mut scope, &mut research).await;
        assert!(refused.starts_with("Search not run"), "{refused}");
        assert!(
            refused.contains("not blank once any site: operators are lifted out"),
            "{refused}"
        );
        assert!(!refused.contains("3 to 240"), "{refused}");
    }
    assert!(
        web.needs.lock().unwrap().is_empty(),
        "a refused call never searched"
    );
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
    let need = json!({
        "query": "site:platform.openai.com OR site:docs.anthropic.com tool result error handling",
        "kind": "official_docs"
    });

    let one_down = PerSite {
        down: vec!["platform.openai.com"],
        searched: Mutex::new(Vec::new()),
    };
    let (mut scope, mut research) = (Scope::for_question(), Research::default());
    let result = call(&one_down, SEARCH, &need, &mut scope, &mut research).await;
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
    assert_eq!(result.matches("[h").count(), 1, "{result}");
    assert!(
        result.contains("Link: https://docs.anthropic.com/page"),
        "{result}"
    );
    assert_eq!(research.web_note, None, "web search was used");

    let both_down = PerSite {
        down: vec!["platform.openai.com", "docs.anthropic.com"],
        searched: Mutex::new(Vec::new()),
    };
    let (mut scope, mut research) = (Scope::for_question(), Research::default());
    let result = call(&both_down, SEARCH, &need, &mut scope, &mut research).await;
    assert!(result.starts_with("Search failed"), "{result}");
    assert!(research.web_note.is_some());
}

/// Answers a search with two pages and saves the page it is asked for.
struct Pages {
    saved: Mutex<Vec<String>>,
    text: String,
}

#[async_trait]
impl VerifierWeb for Pages {
    async fn search(&self, _need: &PlannedNeed, _room: Room<'_>) -> Result<Vec<SearchHit>> {
        Ok(vec![
            SearchHit {
                title: "Harbour tide tables".into(),
                url: "https://example.org/tides".into(),
                snippet: "High water at the harbour.".into(),
            },
            SearchHit {
                title: "Kiln firing notes".into(),
                url: "https://example.org/kiln".into(),
                snippet: "Firing a bowl.".into(),
            },
        ])
    }
    async fn save_page(&self, url: &str) -> Result<AuditPassage> {
        self.saved.lock().unwrap().push(url.to_owned());
        Ok(AuditPassage {
            source_version_id: "version-1".into(),
            title: "Harbour tide tables".into(),
            start_byte: 0,
            end_byte: self.text.len(),
            text: self.text.clone(),
        })
    }
}

fn pages(text: &str) -> Pages {
    Pages {
        saved: Mutex::new(Vec::new()),
        text: text.into(),
    }
}

fn search_call() -> Value {
    json!({"query": "harbour tide tables", "kind": "web"})
}

#[tokio::test]
async fn fetch_page_takes_a_search_results_handle_and_returns_the_page_as_citable_sentences() {
    let web = pages("High water comes twice a day. The crane waits for it.");
    let (mut scope, mut research) = (Scope::for_question(), Research::default());
    let found = call(&web, SEARCH, &search_call(), &mut scope, &mut research).await;
    assert!(found.contains("[h1] Harbour tide tables"), "{found}");
    assert!(found.contains("[h2] Kiln firing notes"), "{found}");

    let page = call(
        &web,
        FETCH,
        &json!({"hit": "h2"}),
        &mut scope,
        &mut research,
    )
    .await;

    assert_eq!(
        web.saved.lock().unwrap().as_slice(),
        ["https://example.org/kiln"]
    );
    assert_eq!(
        page,
        "Harbour tide tables\n[u1] High water comes twice a day.\n[u2] The crane waits for it."
    );
    assert_eq!(research.fetched.len(), 1);
    assert_eq!(research.fetched[0].source_version_id, "version-1");
}

#[tokio::test]
async fn a_handle_that_was_not_shown_is_answered_in_words_and_nothing_is_fetched() {
    let web = pages("High water comes twice a day.");
    let (mut scope, mut research) = (Scope::for_question(), Research::default());
    call(&web, SEARCH, &search_call(), &mut scope, &mut research).await;

    for arguments in [
        json!({"hit": "h7"}),
        json!({"hit": "https://example.org/kiln"}),
        json!({"url": "https://example.org/kiln"}),
        json!({}),
    ] {
        let told = call(&web, FETCH, &arguments, &mut scope, &mut research).await;
        assert!(!told.is_empty(), "{arguments}");
    }

    assert!(web.saved.lock().unwrap().is_empty());
    assert!(research.fetched.is_empty());
}

#[tokio::test]
async fn a_page_too_long_for_its_share_is_cut_to_whole_sentences() {
    let sentences: Vec<String> = (1..=8)
        .map(|at| format!("Tide table number {at} lists the high water."))
        .collect();
    let whole = sentences.join(" ");
    let web = pages(&whole);
    let llm = ScriptedVerifier::answering(200_000, |_, _| json!({}));
    let (mut scope, mut research) = (Scope::for_question(), Research::default());
    let tool = Call {
        llm: &llm,
        web: &web,
        room: Room::for_all(),
        share: 40,
    };
    tool.run(SEARCH, &search_call(), &mut scope, &mut research)
        .await;

    let page = tool
        .run(FETCH, &json!({"hit": "h1"}), &mut scope, &mut research)
        .await;

    let kept = &research.fetched[0].text;
    assert!(llm.count_tokens(kept) <= 40, "{kept}");
    assert!(kept.len() < whole.len());
    assert!(kept.ends_with("high water."), "{kept}");
    assert!(whole.starts_with(kept.as_str()));
    assert_eq!(
        page.lines().count(),
        1 + kept.matches("high water.").count()
    );
}

#[test]
fn the_fetch_tool_asks_for_a_hit_and_not_an_address() {
    let definitions = definitions();
    let fetch = &definitions[1];
    assert_eq!(fetch.name, FETCH);
    let properties = &fetch.parameters["properties"];
    assert!(properties.get("hit").is_some());
    assert!(properties.get("url").is_none());
    assert_eq!(fetch.parameters["required"], json!(["hit"]));
    assert!(!fetch.description.contains("W1"));
}

#[test]
fn a_search_result_shows_its_words_and_no_internal_value() {
    let mut marks = Marks::new();
    let hit = Hit {
        title: marks.told("Harbour tide tables".to_owned()).0,
        url: marks.told("https://example.org/tides".to_owned()).0,
        snippet: marks.told("High water at the harbour.".to_owned()).0,
    };
    let hits = [hit];
    let mut scope = Scope::for_question();
    let mut showing = scope.request();
    let shown = showing.show(&hits).unwrap();

    tool_holds(&fetch_parameters(), &hits_view(&shown), &marks);
}
