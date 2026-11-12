use super::*;

fn done(cut: Cut) -> Option<String> {
    match cut {
        Cut::Done(text) => Some(text),
        _ => None,
    }
}

#[test]
fn a_sentence_taken_out_leaves_the_rest_as_written() {
    let cut = |text: &str, quote: &str| done(sentence(text, quote));
    assert_eq!(
        cut("A one. B two. C three.", "B two.").as_deref(),
        Some("A one. C three.")
    );
    assert_eq!(cut("A one. B two.", "A one.").as_deref(), Some("B two."));
    assert_eq!(cut("A one. B two.", "B two.").as_deref(), Some("A one."));
    assert_eq!(
        cut("- A.\n- B.\n- C.", "- B.").as_deref(),
        Some("- A.\n- C.")
    );
    // A paragraph break stays where it was.
    assert_eq!(
        cut("P1 a. P1 b.\n\nP2 a.", "P1 b.").as_deref(),
        Some("P1 a.\n\nP2 a.")
    );
    assert_eq!(
        cut("P1 a. P1 b.\n\nP2 a.\n", "P2 a.").as_deref(),
        Some("P1 a. P1 b.\n")
    );
    assert_eq!(cut("P1 a.\n\nP2 a.", "P1 a.").as_deref(), Some("P2 a."));
}

#[test]
fn every_place_a_sentence_is_written_goes_with_it() {
    let cut = |text: &str, quote: &str| done(sentence(text, quote));
    assert_eq!(cut("Dup. Keep. Dup.", "Dup.").as_deref(), Some("Keep."));
    // A quote over several sentences takes them all; one inside a sentence
    // takes the sentence it is in.
    assert_eq!(
        cut("A one. B two. C three.", "A one. B two.").as_deref(),
        Some("C three.")
    );
    assert_eq!(
        cut("A one. B two. C three.", "two").as_deref(),
        Some("A one. C three.")
    );
    assert_eq!(
        cut("```py\nx\n```\nProse one. Prose two.", "Prose one.").as_deref(),
        Some("```py\nx\n```\nProse two.")
    );
}

#[test]
fn text_already_out_needs_no_cut_and_the_whole_of_a_field_or_a_code_block_stays() {
    assert!(matches!(sentence("A one.", "B two."), Cut::Absent));
    assert!(matches!(sentence("A one.", ""), Cut::Absent));
    assert!(matches!(sentence("Only. Only.", "Only."), Cut::Refused));
    assert!(matches!(
        sentence("Text.\n```py\nx = 1. y = 2\n```\n", "x = 1."),
        Cut::Refused
    ));
    assert!(matches!(
        sentence("A.\n```\nB\n```\n", "```\nB"),
        Cut::Refused
    ));
}

#[test]
fn a_fenced_example_is_taken_out_with_its_fences() {
    let code = "print(1)\n";
    let cut = |text: &str| done(fence(text, code));
    assert_eq!(
        cut("Intro.\n\n```python\nprint(1)\n```\n\nAfter.").as_deref(),
        Some("Intro.\n\nAfter.")
    );
    assert_eq!(
        cut("Intro.\n```python\nprint(1)\n```\nAfter.").as_deref(),
        Some("Intro.\nAfter.")
    );
    assert_eq!(
        cut("Intro.\n\n```python\nprint(1)\n```\n").as_deref(),
        Some("Intro.\n")
    );
    assert_eq!(
        cut("```python\nprint(1)\n```\n\nAfter.").as_deref(),
        Some("After.")
    );
    assert!(matches!(fence("Intro.", code), Cut::Absent));
    assert!(
        matches!(fence("print(1)\n", code), Cut::Refused),
        "no fence around it"
    );
    assert!(
        matches!(fence("```python\nprint(1)\n```\n", code), Cut::Refused),
        "nothing would be left"
    );
}

const ORDERED: &str = "2. Step two. Done two.\n3. First sentence. Second sentence.\n4. Step four.";

#[test]
fn a_list_marker_stays_on_its_line_when_the_first_sentence_of_the_item_is_cut() {
    let cut = |text: &str, quote: &str| done(sentence(text, quote));
    assert_eq!(
        cut(ORDERED, "First sentence.").as_deref(),
        Some("2. Step two. Done two.\n3. Second sentence.\n4. Step four.")
    );
    assert_eq!(
        cut(
            "- Step two. Done two.\n- First sentence. Second sentence.\n- Step four.",
            "First sentence."
        )
        .as_deref(),
        Some("- Step two. Done two.\n- Second sentence.\n- Step four.")
    );
}

#[test]
fn a_list_marker_stands_before_the_first_sentence_of_its_item_that_is_kept() {
    let text = "2. Step two.\n3. First sentence. Second sentence. Third sentence.\n4. Step four.";
    assert_eq!(
        done(sentence(text, "First sentence. Second sentence.")).as_deref(),
        Some("2. Step two.\n3. Third sentence.\n4. Step four.")
    );
}

#[test]
fn the_place_of_a_cut_item_start_records_the_sentence_without_its_marker() {
    let place = cut_place(ORDERED, "First sentence.").unwrap();
    assert_eq!(place.sentence, "First sentence.");
    assert_eq!(place.before.as_deref(), Some("Done two."));
    assert_eq!(place.after.as_deref(), Some("Second sentence."));
}

#[test]
fn an_item_cut_whole_leaves_its_neighbours_on_adjacent_lines_without_a_bare_marker() {
    assert_eq!(
        done(sentence(
            "2. Step two.\n3. Only sentence.\n4. Step four.",
            "Only sentence."
        ))
        .as_deref(),
        Some("2. Step two.\n4. Step four.")
    );
}

#[test]
fn cutting_a_later_sentence_of_an_item_leaves_its_marker_and_first_sentence_as_written() {
    assert_eq!(
        done(sentence(ORDERED, "Second sentence.")).as_deref(),
        Some("2. Step two. Done two.\n3. First sentence.\n4. Step four.")
    );
}

#[test]
fn a_cut_in_the_middle_of_a_plain_paragraph_is_unchanged() {
    assert_eq!(
        done(sentence(
            "Alpha holds. Beta holds. Gamma holds.\n\nDelta holds.",
            "Beta holds."
        ))
        .as_deref(),
        Some("Alpha holds. Gamma holds.\n\nDelta holds.")
    );
}

#[test]
fn a_dash_that_opens_a_sentence_inside_a_line_is_not_a_list_marker() {
    let out = done(sentence(
        "Alpha holds. - Beta holds. Gamma holds.",
        "Beta holds.",
    ))
    .unwrap();
    assert!(out.ends_with("Gamma holds."), "{out}");
    assert!(!out.contains("- Gamma"), "{out}");
}

#[test]
fn a_cut_takes_only_the_place_it_records() {
    let text = "Dup. Keep. Dup.";
    assert_eq!(done(sentence(text, "Dup.")).as_deref(), Some("Keep. Dup."));
    assert_eq!(cut_place(text, "Dup.").unwrap().sentence, "Dup.");
}

#[test]
fn a_quote_after_a_longer_fence_closes_is_cut() {
    let text = "````\n```\nlet x = 1;\n````\nRust is fast.";
    let out = done(sentence(text, "Rust is fast."));
    assert!(
        out.as_deref()
            .is_some_and(|out| out.contains("let x = 1;") && !out.contains("Rust is fast.")),
        "{out:?}"
    );
}
