//! A lesson says what its references say in its own words: a teaching
//! sentence that is a whole sentence of an excerpt, or a quotable span of one,
//! stands only in double quotation marks, whether or not the lesson cites it.
#![allow(clippy::unwrap_used, clippy::indexing_slicing)]
use super::copied_wording::{findings, Wording};
use super::text_edits::apply_in;
use crate::review_findings::{FindingKind, FindingSite};
use serde_json::{json, Value};

const SENTENCE: &str = "The widget keeps every reading in a ledger until the next sweep.";
const EXCERPT: &str =
    "The widget keeps every reading in a ledger until the next sweep. A sweep clears the ledger and starts a new one.";

fn wording() -> Wording {
    Wording::of_excerpts([EXCERPT])
}

fn lesson(body: &str) -> Value {
    json!({"blocks":[{"kind":"explanation","title":"Readings","body":body,
        "quote":"","citations":[]}],"questions":[]})
}

fn copies(body: &str) -> usize {
    findings(&lesson(body), &wording()).len()
}

#[test]
fn a_sentence_copied_from_an_excerpt_with_no_quote_and_no_citation_is_a_finding() {
    let found = findings(&lesson(SENTENCE), &wording());
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].kind, FindingKind::Teaching);
    assert_eq!(found[0].place().site, FindingSite::Section(0));
    assert!(found[0].note.contains(SENTENCE), "{}", found[0].note);
    assert!(
        found[0].note.contains("double quotation marks"),
        "{}",
        found[0].note
    );
}

#[test]
fn a_sentence_that_copies_a_source_sentence_and_adds_a_clause_is_a_finding() {
    assert_eq!(
        copies("The widget keeps every reading in a ledger until the next sweep, and that is why it is slow."),
        1
    );
}

#[test]
fn a_quotable_span_of_an_excerpt_is_a_finding() {
    assert_eq!(
        copies("Keeps every reading in a ledger until the next sweep."),
        1
    );
}

#[test]
fn the_same_words_inside_quotation_marks_are_no_finding() {
    assert_eq!(
        copies(&format!(
            "The manual puts it this way: \"{SENTENCE}\" The lesson goes on in its own words."
        )),
        0
    );
}

#[test]
fn ordinary_phrasing_on_the_same_topic_is_no_finding() {
    assert_eq!(
        copies("The widget stores readings in a ledger and empties it when it is swept."),
        0
    );
    assert_eq!(copies("A sweep starts a new ledger."), 0);
}

#[test]
fn the_review_and_the_draft_check_find_the_copy_once() {
    let policy = crate::lessons::shape::policy();
    let schema = policy.schema(crate::lessons::shape::Phase::Kept, 2);
    let teaching = |lesson: &Value| {
        crate::teaching::structural_findings(
            crate::teaching::ReviewSchema::Lesson(crate::lessons::shape::LessonShape::new(
                &schema, policy,
            )),
            &lesson.to_string(),
            &wording(),
        )
        .unwrap()
        .into_iter()
        .filter(|finding| finding.kind == FindingKind::Teaching)
        .count()
    };
    assert_eq!(teaching(&lesson(SENTENCE)), 1);
    let mut cited = lesson(SENTENCE);
    cited["blocks"][0]["citations"] =
        json!([{"sentence":SENTENCE,"sourceIndex":0,"quote":SENTENCE}]);
    assert_eq!(teaching(&cited), 1, "a cited quote is not reported twice");
}

fn edit(after: &str) -> lattice_core::shared::error::Result<Value> {
    let patch = json!({"edits":[{"path":"/blocks/0/body","before":"Opening.","after":after}]});
    apply_in(
        &lesson("Opening."),
        &patch.to_string(),
        &Value::Null,
        &wording(),
    )
}

#[test]
fn a_repair_edit_adding_a_copied_sentence_is_refused() {
    let refused = edit(&format!("Opening. {SENTENCE}"));
    assert!(refused.is_err(), "{refused:?}");
    let message = refused.unwrap_err().to_string();
    assert!(message.contains(SENTENCE), "{message}");
    assert!(message.contains("double quotation marks"), "{message}");
}

#[test]
fn a_repair_edit_in_the_lessons_own_words_is_applied() {
    let applied = edit("Opening. The widget empties its ledger when it is swept.").unwrap();
    assert_eq!(
        applied["blocks"][0]["body"],
        "Opening. The widget empties its ledger when it is swept."
    );
}

#[test]
fn an_excerpt_too_short_to_be_quoted_makes_no_sentence_a_copy() {
    let short = Wording::of_excerpts(["Sweeps clear it."]);
    let found = findings(
        &lesson("Then sweeps clear it before the next reading."),
        &short,
    );
    assert!(found.is_empty(), "{found:?}");
}
