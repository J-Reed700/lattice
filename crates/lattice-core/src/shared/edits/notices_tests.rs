use super::notices::markdown_notices;
use super::types::FieldNotice;

#[test]
fn a_list_that_skips_a_number_names_the_line() {
    let md = "1. First item\n2. Second item\n4. Fourth item\n";
    assert_eq!(
        markdown_notices("/body", md),
        vec![FieldNotice {
            path: "/body".into(),
            code: "list_numbering".into(),
            message: "The numbering jumps from 2 to 4 at line 3. A line may have lost its number."
                .into(),
            line: Some(3),
        }]
    );
}

#[test]
fn a_line_that_lost_its_number_is_noticed() {
    let md = "1. One\n2. Two\n3. Three\nAn unmarked line in the list\n5. Five\n";
    assert_eq!(
        markdown_notices("/body", md),
        vec![FieldNotice {
            path: "/body".into(),
            code: "list_numbering".into(),
            message: "The numbering jumps from 3 to 5 at line 5. A line may have lost its number."
                .into(),
            line: Some(5),
        }]
    );
}

#[test]
fn numbers_inside_a_code_fence_are_not_a_list() {
    let md =
        "Intro:\n\n```text\n1. First\n5. Jumped inside code\n```\n\n1. Real one\n2. Real two\n";
    assert_eq!(markdown_notices("/body", md), Vec::new());
}

#[test]
fn a_list_numbered_all_alike_is_left_alone() {
    let md = "1. Alpha\n1. Beta\n1. Gamma\n";
    assert_eq!(markdown_notices("/body", md), Vec::new());
}

#[test]
fn an_unclosed_fence_is_noticed() {
    let md = "Heading\n\n```rust\nfn example() {}\n";
    assert_eq!(
        markdown_notices("/body", md),
        vec![FieldNotice {
            path: "/body".into(),
            code: "open_code_fence".into(),
            message: "A code block that opens at line 3 is never closed.".into(),
            line: Some(3),
        }]
    );
}
