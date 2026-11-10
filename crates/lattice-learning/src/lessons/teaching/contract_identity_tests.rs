//! Whether the lesson's own words bear out a quote the check gives.
#![allow(clippy::unwrap_used, clippy::indexing_slicing)]
use super::*;

fn findings_for(quote: &str) -> usize {
    let lesson = json!({"blocks":[{"kind":"explanation","title":"Opening",
        "body":"The tool prints the working folder when it runs."}],"questions":[]});
    let items = vec![Item {
        id: "idea-0".into(),
        kind: "idea",
        text: "printing the folder".into(),
    }];
    let answer = Answer {
        planned: vec![Planned {
            item: "idea-0".into(),
            taught: true,
            section_id: "section-0".into(),
            quote: quote.into(),
        }],
        used_before_taught: Vec::new(),
        contradictions: Vec::new(),
        unseen: Vec::new(),
    };
    findings(&answer, &items, &parts_of(&lesson), &lesson, &[]).len()
}

#[test]
fn a_quote_that_differs_only_in_the_case_of_a_letter_is_found_in_the_part() {
    assert_eq!(findings_for("Prints the working folder"), 0);
    assert_eq!(findings_for("prints the working folder"), 0);
}

#[test]
fn a_quote_in_other_words_is_still_not_found() {
    assert_eq!(findings_for("Prints the current directory"), 1);
}
