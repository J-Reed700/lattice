use super::*;
use crate::domain::provenance::markdown_links;

#[test]
fn the_owners_link_ranks_first_then_the_nearest() {
    let text = "Intro with [a far link](https://far.example/x).\n\nServe chicken at 74 °C, as [Serious Eats](https://seriouseats.com/a) notes, following the [USDA chart](https://www.fsis.usda.gov/chart). See also [a review](https://review.example/r).\n\nNext paragraph [other](https://other.example/o).";
    let links = markdown_links(text);
    let passage = "Serve chicken at 74 °C";
    let start = text.find(passage).unwrap();
    let candidates =
        citation_candidates(text, (start, start + passage.len()), &links, &[], |link| {
            if link.url.contains("usda.gov") {
                LinkStanding::Owner
            } else {
                LinkStanding::Unknown
            }
        });
    let urls: Vec<&str> = candidates.iter().map(|c| c.url.as_str()).collect();
    assert_eq!(
        urls,
        vec!["https://seriouseats.com/a"],
        "only the nearest link after the passage in its paragraph"
    );
    let whole = "Serve chicken at 74 °C, as [Serious Eats](https://seriouseats.com/a) notes, following the [USDA chart](https://www.fsis.usda.gov/chart).";
    let start = text.find(whole).unwrap();
    let candidates = citation_candidates(text, (start, start + whole.len()), &links, &[], |link| {
        if link.url.contains("usda.gov") {
            LinkStanding::Owner
        } else {
            LinkStanding::Unknown
        }
    });
    let urls: Vec<&str> = candidates.iter().map(|c| c.url.as_str()).collect();
    assert_eq!(
        urls,
        vec![
            "https://www.fsis.usda.gov/chart",
            "https://seriouseats.com/a",
            "https://review.example/r"
        ]
    );
}

#[test]
fn footnote_markers_reach_links_elsewhere_in_the_text() {
    let text = "Adults need 150 minutes a week [2].\n\nReferences: [[1]](https://a.example/1) [[2]](https://www.who.int/pa)";
    let links = markdown_links(text);
    let passage = "Adults need 150 minutes a week [2].";
    let candidates = citation_candidates(text, (0, passage.len()), &links, &[], |_| {
        LinkStanding::Unknown
    });
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].url, "https://www.who.int/pa");
    assert_eq!(candidates[0].distance, 0);
}

#[test]
fn links_into_the_citing_page_or_its_publisher_are_not_citations() {
    let text = "Models call tools ([related: RLHF](https://www.example.org/rlhf), [see below](https://book.example.org/ch2.html#why-models-need-tools)), as [the paper](https://arxiv.org/abs/2302.04761) shows.";
    let links = markdown_links(text);
    let passage = "Models call tools";
    let citing = Origin::of_url("https://book.example.org/ch2.html")
        .unwrap()
        .key;
    let candidates = citation_candidates(text, (0, passage.len()), &links, &[citing], |_| {
        LinkStanding::Unknown
    });
    let urls: Vec<&str> = candidates.iter().map(|c| c.url.as_str()).collect();
    assert_eq!(
        urls,
        vec!["https://arxiv.org/abs/2302.04761"],
        "the page's own fragments and its publisher's other pages are skipped, so the nearest outside link is found"
    );
}

#[test]
fn a_chain_goes_on_only_through_new_non_primary_clusters() {
    let seen = vec![ClusterId("a".into())];
    assert!(follow_further(
        TrustLevel::Established,
        &ClusterId("b".into()),
        &seen
    ));
    assert!(!follow_further(
        TrustLevel::Established,
        &ClusterId("a".into()),
        &seen
    ));
    assert!(!follow_further(
        TrustLevel::Primary,
        &ClusterId("c".into()),
        &seen
    ));
}
