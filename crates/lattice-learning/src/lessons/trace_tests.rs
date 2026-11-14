use super::*;

fn list(items: &[&str]) -> Vec<String> {
    items.iter().map(|item| (*item).to_owned()).collect()
}

#[test]
fn unchanged_sentences_give_an_empty_diff() {
    let same = list(&["One.", "Two.", "Two."]);
    assert_eq!(diff_sentences(&same, &same), Diff::default());
    assert_eq!(
        diff("A plain sentence.", "A plain sentence."),
        Diff::default()
    );
}

#[test]
fn removed_and_added_keep_their_order() {
    let found = diff_sentences(&list(&["a", "b", "c", "d"]), &list(&["a", "x", "c", "y"]));
    assert_eq!(found.removed, list(&["b", "d"]));
    assert_eq!(found.added, list(&["x", "y"]));
}

#[test]
fn duplicates_are_matched_one_for_one() {
    let found = diff_sentences(&list(&["a", "a", "b"]), &list(&["a", "b", "b"]));
    assert_eq!(found.removed, list(&["a"]));
    assert_eq!(found.added, list(&["b"]));
}

#[test]
fn a_sentence_that_only_moved_is_not_a_change() {
    let found = diff_sentences(&list(&["a", "b"]), &list(&["b", "a"]));
    assert_eq!(found, Diff::default());
}

#[test]
fn a_part_as_json_is_diffed_by_its_sentences() {
    let before = r#"{"body":"Alpha is first. Beta is second."}"#;
    let after = r#"{"body":"Alpha is first. Gamma is third."}"#;
    let found = diff(before, after);
    assert_eq!(found.removed, list(&["Beta is second."]));
    assert_eq!(found.added, list(&["Gamma is third."]));
}

#[test]
fn a_cause_lists_what_it_was_given() {
    let claims = [ClaimId("c1".into()), ClaimId("c2".into())];
    let cause = Cause::none().claims(&claims);
    assert_eq!(cause.claims, list(&["c1", "c2"]));
    assert!(cause.findings.is_empty());
}

#[test]
fn a_stage_is_named_in_snake_case() {
    assert_eq!(Stage::ClaimRepair.name(), "claim_repair");
    assert_eq!(Stage::TeachingRepair.to_string(), "teaching_repair");
    assert_eq!(Stage::Publishing.name(), "publishing");
}
