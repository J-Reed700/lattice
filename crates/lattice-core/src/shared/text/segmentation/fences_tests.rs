//! Fenced code blocks: what opens one, what closes it, where it lies.

#![allow(clippy::unwrap_used, clippy::indexing_slicing)]

use super::fences::*;

fn wholes(text: &str) -> Vec<&str> {
    code_fences(text)
        .iter()
        .filter_map(|fence| text.get(fence.whole()))
        .collect()
}

#[test]
fn a_backtick_fence_runs_from_its_opening_to_its_closing_line() {
    let text = "Intro.\n```rust title\nlet x = 1;\n```\nAfter.\n";
    let fences = code_fences(text);
    assert_eq!(fences.len(), 1);
    let fence = &fences[0];
    assert_eq!(fence.language, "rust");
    assert_eq!(text.get(fence.opening.clone()), Some("```rust title\n"));
    assert_eq!(text.get(fence.text.clone()), Some("let x = 1;\n"));
    assert_eq!(
        fence.closing.clone().and_then(|c| text.get(c)),
        Some("```\n")
    );
    assert_eq!(wholes(text), ["```rust title\nlet x = 1;\n```\n"]);
}

#[test]
fn a_tilde_fence_is_a_fence() {
    assert_eq!(wholes("~~~\nx\n~~~\ny"), ["~~~\nx\n~~~\n"]);
}

#[test]
fn a_shorter_or_different_run_does_not_close_a_longer_fence() {
    let text = "````md\n```\nx\n~~~~\n````\nout\n";
    assert_eq!(wholes(text), ["````md\n```\nx\n~~~~\n````\n"]);
}

#[test]
fn a_fence_opened_in_a_list_item_or_a_blockquote_is_a_fence() {
    let text = "- ```rust\n  fn main() {}\n  ```\n> ~~~\n> y\n> ~~~\n";
    let fences = code_fences(text);
    assert_eq!(fences.len(), 2);
    assert_eq!(fences[0].language, "rust");
    assert!(fences.iter().all(|fence| fence.closing.is_some()));
}

#[test]
fn an_unclosed_fence_runs_to_the_end_of_the_text() {
    let text = "a\n```\nx\ny";
    let fences = code_fences(text);
    assert_eq!(fences.len(), 1);
    assert_eq!(fences[0].closing, None);
    assert_eq!(wholes(text), ["```\nx\ny"]);
}

#[test]
fn a_backtick_in_the_info_string_opens_nothing() {
    assert!(code_fences("```a`b\nx\n```\n").is_empty());
    assert!(code_fences("`` x\n").is_empty());
}

#[test]
fn a_fence_has_no_language_without_an_info_string() {
    assert_eq!(code_fences("```\nx\n```\n")[0].language, "");
}

#[test]
fn a_byte_is_in_a_fence_from_its_opening_line_to_its_closing_line() {
    let text = "ab\n```\ncd\n```\nef\n";
    assert!(!in_code_fence(text, 0));
    assert!(in_code_fence(text, 3));
    assert!(in_code_fence(text, 8));
    assert!(in_code_fence(text, 13));
    assert!(!in_code_fence(text, 15));
}
