//! RRF against a fusion computed by hand, the tie-breaks, dedupe by
//! canonical key, and the diversity pass.

#![cfg(test)]

use url::Url;

use super::*;
use crate::domain::sourcing::lead::LeadKind;
use crate::domain::sourcing::need::Identifier;

const K: u32 = 60;

fn id(name: &'static str) -> FinderId {
    FinderId::from_static(name)
}

fn lead(raw: &str, finder: &FinderId, rank: usize) -> Lead {
    Lead::page(Url::parse(raw).unwrap(), finder.clone(), rank)
}

fn urls(leads: &[Lead]) -> Vec<&str> {
    leads.iter().map(|lead| lead.url.as_str()).collect()
}

#[test]
fn matches_a_fusion_computed_by_hand() {
    let (ddg, bing, wiki) = (id("duckduckgo"), id("bing_html"), id("wikipedia"));
    let ddg_list = vec![
        lead("https://a.example/", &ddg, 1),
        lead("https://b.example/page", &ddg, 2),
        lead("https://c.example/", &ddg, 3),
        lead("https://d.example/doc#top", &ddg, 4),
    ];
    let bing_list = vec![
        // B again, utm-tagged: folds into B.
        lead("https://b.example/page?utm_source=x", &bing, 1),
        lead("https://d.example/doc", &bing, 2),
        lead("https://a.example/", &bing, 3),
    ];
    let mut archived_d = lead(
        "https://web.archive.org/web/2024/https://d.example/doc",
        &wiki,
        1,
    );
    archived_d.kind = LeadKind::ArchivedCopy {
        original: Url::parse("https://d.example/doc").unwrap(),
        captured_at: None,
    };
    let wiki_list = vec![archived_d];

    let fused = rrf_scored(
        &[
            RankedList {
                finder: &ddg,
                wave: 0,
                leads: &ddg_list,
            },
            RankedList {
                finder: &bing,
                wave: 0,
                leads: &bing_list,
            },
            RankedList {
                finder: &wiki,
                wave: 1,
                leads: &wiki_list,
            },
        ],
        K,
        None,
    );

    // By hand, k = 60:
    //   D: 1/64 + 1/62 + 1/61 = 0.015625 + 0.0161290323 + 0.0163934426 = 0.0481474749
    //   B: 1/62 + 1/61        = 0.0161290323 + 0.0163934426            = 0.0325224749
    //   A: 1/61 + 1/63        = 0.0163934426 + 0.0158730159            = 0.0322664585
    //   C: 1/63                                                         = 0.0158730159
    let expected = [
        ("https://d.example/doc", 0.048_147_474_9),
        ("https://b.example/page?utm_source=x", 0.032_522_474_9),
        ("https://a.example/", 0.032_266_458_5),
        ("https://c.example/", 0.015_873_015_9),
    ];
    assert_eq!(fused.len(), expected.len());
    for ((lead, score), (url, hand)) in fused.iter().zip(expected) {
        assert_eq!(lead.url.as_str(), url);
        assert!((score - hand).abs() < 1e-9, "{url}: {score} vs {hand}");
    }

    // The live page represents D over its archived copy, and D carries every
    // finder that found it, in list order.
    let d = &fused[0].0;
    assert_eq!(d.kind, LeadKind::Page);
    assert_eq!(
        d.found_by,
        vec![(ddg.clone(), 4), (bing.clone(), 2), (wiki.clone(), 1)]
    );
    assert_eq!(fused[1].0.found_by, vec![(ddg, 2), (bing, 1)]);
}

#[test]
fn equal_scores_break_by_owner_kind_then_wave() {
    let (one, two) = (id("one"), id("two"));
    let owner = Origin::new("learn.microsoft.com", None);

    let mut document = lead("https://z.example/rfc", &two, 1);
    document.kind = LeadKind::Document(Identifier::Rfc(9110));

    let cases: Vec<(&str, Lead, usize, Lead, usize, &str)> = vec![
        (
            "owner match first",
            lead("https://a.example/x", &one, 1),
            0,
            lead("https://learn.microsoft.com/dotnet/x", &two, 1),
            0,
            "https://learn.microsoft.com/dotnet/x",
        ),
        (
            "document before page",
            lead("https://a.example/x", &one, 1),
            0,
            document,
            0,
            "https://z.example/rfc",
        ),
        (
            "earlier wave first",
            lead("https://a.example/x", &one, 1),
            1,
            lead("https://z.example/y", &two, 1),
            0,
            "https://z.example/y",
        ),
    ];
    for (case, first, first_wave, second, second_wave, winner) in cases {
        let first_list = [first];
        let second_list = [second];
        let fused = rrf(
            &[
                RankedList {
                    finder: &one,
                    wave: first_wave,
                    leads: &first_list,
                },
                RankedList {
                    finder: &two,
                    wave: second_wave,
                    leads: &second_list,
                },
            ],
            K,
            Some(&owner),
        );
        assert_eq!(fused[0].url.as_str(), winner, "{case}");
    }
}

#[test]
fn a_finders_own_duplicate_counts_once() {
    let ddg = id("duckduckgo");
    let list = vec![
        lead("https://a.example/x", &ddg, 1),
        lead("https://a.example/x?utm_medium=feed", &ddg, 2),
    ];
    let fused = rrf_scored(
        &[RankedList {
            finder: &ddg,
            wave: 0,
            leads: &list,
        }],
        K,
        None,
    );
    assert_eq!(fused.len(), 1);
    assert!((fused[0].1 - rrf_term(K, 1)).abs() < 1e-12);
    assert_eq!(fused[0].0.found_by, vec![(ddg, 1)]);
}

#[test]
fn a_later_page_keeps_its_true_rank() {
    let ddg = id("duckduckgo");
    let list = vec![lead("https://a.example/x", &ddg, 11)];
    let fused = rrf_scored(
        &[RankedList {
            finder: &ddg,
            wave: 0,
            leads: &list,
        }],
        K,
        None,
    );
    assert!((fused[0].1 - 1.0 / 71.0).abs() < 1e-12);
}

#[test]
fn diversity_moves_repeat_hosts_back_but_never_the_owner() {
    let ddg = id("duckduckgo");
    let owner = Origin::new("owner.example", None);
    let leads = vec![
        lead("https://a.example/1", &ddg, 1),
        lead("https://www.a.example/2", &ddg, 2),
        lead("https://b.example/1", &ddg, 3),
        lead("https://owner.example/1", &ddg, 4),
        lead("https://docs.owner.example/2", &ddg, 5),
        lead("https://owner.example/3", &ddg, 6),
    ];
    let diversified = diversify(leads, Some(&owner));
    assert_eq!(
        urls(&diversified),
        vec![
            "https://a.example/1",
            "https://b.example/1",
            "https://owner.example/1",
            "https://docs.owner.example/2",
            "https://owner.example/3",
            "https://www.a.example/2",
        ]
    );
}
