use super::super::clear_right_choice_reasons;

#[test]
fn a_right_choices_reason_is_cleared_and_the_wrong_choices_keep_theirs() {
    let raw = r#"{"blocks":[],"questions":[{"correctIndex":1,"rationales":["A fails.","This is correct.","C fails."],"explanation":"B holds."}]}"#;
    let out: serde_json::Value =
        serde_json::from_str(&clear_right_choice_reasons(raw.to_string())).unwrap();
    assert_eq!(
        out["questions"][0]["rationales"],
        serde_json::json!(["A fails.", "", "C fails."])
    );
    assert_eq!(out["questions"][0]["explanation"], "B holds.");
}

#[test]
fn a_draft_whose_right_choices_carry_no_reason_comes_back_as_written() {
    let raw = r#"{"questions": [ {"correctIndex":1, "rationales":["A fails.","","C fails."]} ]}"#;
    assert_eq!(clear_right_choice_reasons(raw.to_string()), raw);
}

#[test]
fn a_question_whose_right_choice_has_no_reason_slot_is_left_as_written() {
    let raw = r#"{"questions": [ {"correctIndex":5, "rationales":["A.","B.","C."]}, {"rationales":["A.","B."]} ]}"#;
    assert_eq!(clear_right_choice_reasons(raw.to_string()), raw);
}

#[test]
fn a_draft_that_is_not_json_comes_back_as_written() {
    assert_eq!(
        clear_right_choice_reasons("not json".to_string()),
        "not json"
    );
}
