use super::*;

fn link(url: &str, anchor: &str) -> OutboundLink {
    OutboundLink {
        url: url.into(),
        anchor: anchor.into(),
        start_byte: 0,
        end_byte: 0,
    }
}

fn research() -> Chosen {
    Chosen {
        by: AddedBy::Research,
        note: Some("covers the gap on tool schemas".into()),
    }
}

#[test]
fn a_byline_and_the_domain_name_identify_a_personal_sites_author() {
    let origin = Origin::of_url("https://prakashkagitha.github.io/the-llm-stack/tools/").unwrap();
    let text = "# Tool Use & Function Calling\n\nBy Prakash Kagitha\n\nBy default the model \
                answers in text.\n\n## Schemas\n\nA schema names the arguments.";
    let evidence = ContentEvidence::of(text, &[], &origin, research());
    assert_eq!(evidence.bylines, vec!["By Prakash Kagitha".to_owned()]);
    assert_eq!(evidence.name_in_domain.as_deref(), Some("prakashkagitha"));
    assert!(WorkEvidence::of(&evidence).name_in_domain);
}

#[test]
fn a_copyright_line_and_an_about_link_say_who_publishes_without_a_domain_list() {
    let origin = Origin::of_url("https://www.example-flora.org/species/quercus").unwrap();
    let text =
        "Quercus robur is a deciduous oak.\n\n© 2024 Example Flora Society. All rights reserved.";
    let links = [
        link("https://www.example-flora.org/about-us", "About"),
        link("https://www.example-flora.org/species/fagus", "Beech"),
    ];
    let evidence = ContentEvidence::of(text, &links, &origin, research());
    assert_eq!(evidence.bylines.len(), 1);
    assert_eq!(
        evidence.about_links,
        vec!["https://www.example-flora.org/about-us".to_owned()]
    );
}

#[test]
fn citations_count_external_restricted_and_scholarly_links_without_a_domain_list() {
    let origin = Origin::of_url("https://history.example.net/chapter-1").unwrap();
    let links = [
        link(
            "https://www.archives.gov/research/records",
            "National archives",
        ),
        link("https://doi.org/10.1093/ahr/rhab001", "a review"),
        link("https://arxiv.org/abs/2305.12345v2", "a preprint"),
        link("https://history.example.net/chapter-2", "next chapter"),
    ];
    let evidence = ContentEvidence::of("text", &links, &origin, research());
    assert_eq!(evidence.citations.restricted_registry_links, 1);
    assert_eq!(evidence.citations.scholarly_identifiers, 2);
    assert_eq!(evidence.citations.same_site_links, 1);
    assert_eq!(evidence.citations.external_domains, 3);
    assert!(WorkEvidence::of(&evidence).stands_on_sources());
}

#[test]
fn a_chaptered_work_and_a_paper_have_their_shapes() {
    let mut origin = Origin::of_url("https://book.example.org/08-tool-use/01-calling").unwrap();
    origin.canonical_url = Some("https://book.example.org/08-tool-use/01-calling".into());
    let chapter = "# Calling tools\n\n## Why\n\ntext\n\n## How\n\n```\n# not a heading\n```\n";
    let evidence = ContentEvidence::of(chapter, &[], &origin, research());
    assert!(evidence.work.numbered_path);
    assert_eq!(evidence.work.headings, 3);
    assert_eq!(evidence.work.code_blocks, 1);
    assert!(WorkEvidence::of(&evidence).structured());

    let paper_origin = Origin::of_url("https://journal.example.org/article/42").unwrap();
    let paper = "# Abstract\n\nWe measure mitosis.\n\n# References\n\n1. A paper.";
    let evidence = ContentEvidence::of(paper, &[], &paper_origin, research());
    assert!(evidence.work.has_abstract_or_references);
    assert!(!evidence.work.numbered_path);
}

#[test]
fn prose_that_starts_with_a_cue_word_is_not_a_byline() {
    let origin = Origin::of_url("https://example.org/x").unwrap();
    let text = "By 2025, emissions fell.\nVon Neumann architecture stores programs.\nPar exemple.";
    assert!(ContentEvidence::of(text, &[], &origin, research())
        .bylines
        .is_empty());
}

#[test]
fn shrinking_cuts_the_lists_from_the_end_and_marks_the_evidence() {
    let origin = Origin::of_url("https://example.org/x").unwrap();
    let text = "# One\n# Two\n# Three\n# Four\nBy Ada Lovelace\nAuthor: Charles Babbage";
    let mut evidence = ContentEvidence::of(text, &[], &origin, research());
    let summary = WorkEvidence::of(&evidence);
    evidence.shrink();
    assert!(evidence.truncated);
    assert_eq!(evidence.work.heading_lines, vec!["One", "Two"]);
    assert_eq!(evidence.bylines, vec!["By Ada Lovelace"]);
    assert_eq!(evidence.work.headings, summary.headings);
}
