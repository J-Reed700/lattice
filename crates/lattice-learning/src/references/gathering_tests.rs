//! The order a topic's search widens in.
use super::gathering::Rounds;
use super::web_research::{PlannedNeed, Room};
use lattice_core::application::ports::MockWebService;
use lattice_core::application::contracts::web::WebSearchResult;
use lattice_core::domain::sourcing::NeedKind;
use lattice_core::shared::error::Result;

type Ask = (String, String, usize);

fn ask(query: &str, kind: &str, page: usize) -> Ask {
    (query.into(), kind.into(), page)
}

fn need(query: &str, kind: NeedKind) -> PlannedNeed {
    PlannedNeed {
        kind,
        ..PlannedNeed::web(query.into())
    }
}

/// A web where every search finds `count` distinct pages.
fn web_finding(count: usize) -> MockWebService {
    let web = MockWebService::new();
    web.set_search_results(
        (0..count)
            .map(|i| WebSearchResult {
                title: "Reference".into(),
                url: format!("https://example.org/reference-{i}"),
                snippet: String::new(),
                published_date: None,
                source: None,
            })
            .collect(),
    );
    web
}

/// Every round's asks, in order, after running each round's searches.
async fn rounds_asked(mut rounds: Rounds, web: &MockWebService) -> Result<Vec<Vec<Ask>>> {
    let mut all = Vec::new();
    while let Some(mut round) = rounds.next() {
        all.push(
            round
                .asks()
                .into_iter()
                .map(|(query, kind, page)| (query, kind.to_owned(), page))
                .collect(),
        );
        for search in round.iter_mut() {
            search.next_page(web, Room::for_all()).await?;
        }
    }
    assert!(rounds.next().is_none(), "a finished plan yields nothing");
    Ok(all)
}

#[tokio::test]
async fn a_topic_planned_as_one_paper_search_is_widened_in_four_rounds() -> Result<()> {
    let rounds = Rounds::new(&[need("q1", NeedKind::Paper)], "Random assignment");
    let asked = rounds_asked(rounds, &web_finding(15)).await?;
    assert_eq!(
        asked,
        [
            vec![
                ask("q1", "paper", 1),
                ask("q1", "web", 1),
                ask("Random assignment", "encyclopedia", 1)
            ],
            vec![
                ask("q1", "paper", 2),
                ask("q1", "web", 2),
                ask("Random assignment", "encyclopedia", 2)
            ],
            vec![ask("q1", "community", 1)],
            vec![ask("Random assignment", "web", 1)],
        ]
    );
    Ok(())
}

#[tokio::test]
async fn only_searches_whose_first_page_was_full_get_a_second_page() -> Result<()> {
    let rounds = Rounds::new(&[need("q1", NeedKind::Paper)], "Random assignment");
    let asked = rounds_asked(rounds, &web_finding(3)).await?;
    assert_eq!(asked.len(), 3, "no deeper round: {asked:?}");
    assert!(asked.iter().flatten().all(|(_, _, page)| *page == 1));
    Ok(())
}

#[tokio::test]
async fn kinds_the_plan_already_asked_are_not_asked_again_in_the_other_kinds_round() -> Result<()> {
    let rounds = Rounds::new(
        &[
            need("q1", NeedKind::Paper),
            need("q2", NeedKind::Web),
            need("q3", NeedKind::Encyclopedia),
        ],
        "Random assignment",
    );
    let asked = rounds_asked(rounds, &web_finding(1)).await?;
    assert_eq!(
        asked,
        [
            vec![
                ask("q1", "paper", 1),
                ask("q2", "web", 1),
                ask("q3", "encyclopedia", 1)
            ],
            vec![ask("q1", "community", 1)],
            vec![ask("Random assignment", "web", 1)],
        ]
    );
    Ok(())
}

#[tokio::test]
async fn the_general_kinds_are_asked_in_order_with_the_first_query() -> Result<()> {
    let rounds = Rounds::new(&[need("q1", NeedKind::Paper)], "  ");
    let asked = rounds_asked(rounds, &web_finding(1)).await?;
    assert_eq!(
        asked,
        [
            vec![ask("q1", "paper", 1), ask("q1", "web", 1)],
            vec![ask("q1", "encyclopedia", 1), ask("q1", "community", 1)],
        ],
        "a blank title is never searched"
    );
    Ok(())
}

#[tokio::test]
async fn the_title_is_not_searched_on_the_web_again_when_the_plan_did() -> Result<()> {
    let rounds = Rounds::new(
        &[need("Random assignment", NeedKind::Web)],
        " Random assignment ",
    );
    let asked = rounds_asked(rounds, &web_finding(1)).await?;
    assert_eq!(
        asked,
        [
            vec![
                ask("Random assignment", "web", 1),
                ask("Random assignment", "encyclopedia", 1)
            ],
            vec![
                ask("Random assignment", "community", 1),
                ask("Random assignment", "paper", 1)
            ],
        ]
    );
    Ok(())
}

#[tokio::test]
async fn no_search_is_asked_twice() -> Result<()> {
    for found in [0, 1, 12, 15] {
        let rounds = Rounds::new(
            &[
                need("q1", NeedKind::Paper),
                need("q2", NeedKind::OfficialDocs),
            ],
            "Random assignment",
        );
        let asked: Vec<Ask> = rounds_asked(rounds, &web_finding(found))
            .await?
            .into_iter()
            .flatten()
            .collect();
        let mut unique = asked.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), asked.len(), "{found} results: {asked:?}");
    }
    Ok(())
}

#[tokio::test]
async fn a_topic_with_no_planned_searches_has_no_rounds() -> Result<()> {
    let asked = rounds_asked(Rounds::new(&[], "Random assignment"), &web_finding(15)).await?;
    assert!(asked.is_empty());
    Ok(())
}
