#![allow(clippy::panic, clippy::indexing_slicing)]
use super::review_candidate;
use serde_json::json;

fn passages(body: &str) -> Vec<String> {
    let (reviewed, ids) = review_candidate(&json!({"blocks":[{"body":body}]}));
    let texts: Vec<String> = reviewed["blocks"][0]["bodyPassages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["text"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(ids[0].len(), texts.len());
    texts
}

fn rejoined(texts: &[String]) -> String {
    texts.concat()
}

#[test]
fn a_long_paragraph_is_one_passage_and_no_passage_starts_or_ends_inside_a_word() {
    let body = "The folder holds every report. ".repeat(23);
    assert!(body.len() > 700);
    let texts = passages(&body);
    for pair in texts.windows(2) {
        let last = pair[0].chars().last().unwrap();
        let first = pair[1].chars().next().unwrap();
        assert!(!(last.is_alphanumeric() && first.is_alphanumeric()));
    }
    assert_eq!(texts.len(), 1);
    assert_eq!(rejoined(&texts), body);
}

#[test]
fn three_paragraphs_are_three_passages() {
    let body = "First paragraph, short.\n\nSecond paragraph, short.\n\nThird paragraph, short.";
    let texts = passages(body);
    assert_eq!(texts.len(), 3);
    assert_eq!(texts[0], "First paragraph, short.\n\n");
    assert_eq!(rejoined(&texts), body);
}

#[test]
fn a_numbered_list_of_four_steps_is_four_passages() {
    let body = "1. Open the box.\n2. Remove the lid\n   and set it aside.\n3. Pour the contents out.\n4. Close the box.";
    let texts = passages(body);
    assert_eq!(texts.len(), 4);
    assert_eq!(texts[1], "2. Remove the lid\n   and set it aside.\n");
    assert_eq!(rejoined(&texts), body);
}

#[test]
fn a_fenced_code_block_is_never_split() {
    let code: String = (0..30)
        .map(|n| format!("    let value_{n} = compute({n});\n"))
        .collect();
    let body = format!("Here is the code:\n```rust\n{code}```");
    assert!(code.len() > 600);
    let texts = passages(&body);
    assert_eq!(texts.len(), 1);
    assert_eq!(rejoined(&texts), body);
}

#[test]
fn a_fence_with_blank_lines_inside_stays_whole_and_a_table_stays_whole() {
    let body = "Intro.\n\n```\nfirst\n\nsecond\n```\n\n| a | b |\n|---|---|\n| 1 | 2 |\n| 3 | 4 |";
    let texts = passages(body);
    assert_eq!(texts.len(), 3);
    assert_eq!(texts[1], "```\nfirst\n\nsecond\n```\n\n");
    assert_eq!(texts[2], "| a | b |\n|---|---|\n| 1 | 2 |\n| 3 | 4 |");
    assert_eq!(rejoined(&texts), body);
}
