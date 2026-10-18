use super::*;

#[test]
fn links_are_read_from_the_reader_grammar_with_their_byte_offsets() {
    let text = "Café [the *guide*](https://example.org/guide) and [\\[1\\]](https://example.org/#fn1) and [[2]](https://example.org/#fn2).";
    let links = markdown_links(text);
    assert_eq!(links.len(), 3);
    let first = &links[0];
    assert_eq!(first.anchor, "the guide");
    assert_eq!(first.url, "https://example.org/guide");
    assert_eq!(
        &text[first.start_byte..first.end_byte],
        "[the *guide*](https://example.org/guide)"
    );
    assert_eq!(links[1].anchor, "[1]");
    assert_eq!(links[2].anchor, "[2]", "a nested label is one link");
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
