//! The link scanner and date parsing, one rule each.

use super::dates::parse_date;
use super::links::markdown_links;
use chrono::DateTime;

fn at(text: &str) -> i64 {
    DateTime::parse_from_rfc3339(text).unwrap().timestamp()
}

#[test]
fn links_are_read_from_the_reader_grammar_with_their_byte_offsets() {
    let text =
        "Café [the *guide*](https://example.org/guide) and [\\[1\\]](https://example.org/#fn1).";
    let links = markdown_links(text);
    assert_eq!(links.len(), 2);
    let first = &links[0];
    assert_eq!(first.anchor, "the guide");
    assert_eq!(first.url, "https://example.org/guide");
    assert_eq!(
        &text[first.start_byte..first.end_byte],
        "[the *guide*](https://example.org/guide)"
    );
    assert_eq!(links[1].anchor, "[1]");
}

#[test]
fn escaped_brackets_code_images_and_other_schemes_are_not_links() {
    let text = concat!(
        "\\[not](https://a.example/) prose, `[code](https://b.example/)`, ",
        "![image](https://c.example/i.png), [mail](mailto:x@d.example), ",
        "[relative](/path), [spaced](https://e.example/ x)\n",
        "```text\n[fenced](https://f.example/)\n```\n",
        "[after](https://g.example/) and [outer [inner]](https://h.example/)"
    );
    let urls: Vec<String> = markdown_links(text).into_iter().map(|l| l.url).collect();
    assert_eq!(urls, vec!["https://g.example/", "https://h.example/"]);
}

#[test]
fn an_unclosed_fence_or_label_does_not_lose_or_invent_links() {
    assert!(markdown_links("[open](https://a.example/ and ```").is_empty());
    let text = "[a](https://a.example/)\n````\n[b](https://b.example/)\n```\nstill code\n";
    assert_eq!(markdown_links(text).len(), 1);
}

#[test]
fn page_dates_parse_in_the_forms_pages_write_them() {
    let noon = at("2026-03-14T12:00:00Z");
    for written in [
        "2026-03-14T12:00:00Z",
        "2026-03-14T08:00:00-04:00",
        "2026-03-14T12:00:00.000+0000",
        "2026-03-14T12:00:00",
        "2026-03-14 12:00",
        "Sat, 14 Mar 2026 12:00:00 GMT",
    ] {
        assert_eq!(parse_date(written), Some(noon), "{written}");
    }
    assert_eq!(parse_date("2026/03/14"), Some(at("2026-03-14T00:00:00Z")));
    for unparsed in ["", "last Tuesday", "2026-13-45", "14/03/2026"] {
        assert_eq!(parse_date(unparsed), None, "{unparsed}");
    }
}
