use super::*;

fn id(seed: u128) -> String {
    uuid::Uuid::from_u128(seed).to_string()
}

fn blueprint() -> LearningAssessmentBlueprint {
    LearningAssessmentBlueprint {
        id: id(1),
        purpose: LearningAssessmentPurpose::ModuleTest,
        title: "Concurrency under changed constraints".into(),
        instructions: "Answer from the frozen scenario and explain decisions.".into(),
        expected_minutes: 30,
        allowed_aids: vec!["Saved source library".into()],
        feedback_timing: LearningFeedbackTiming::AfterSubmission,
        requirements: vec![LearningBlueprintRequirement {
            outcome_id: id(2),
            format: LearningItemFormat::SingleChoice,
            count: 2,
            difficulty_min: 2,
            difficulty_max: 4,
        }],
    }
}

fn candidate(seed: u128, difficulty: u8) -> LearningAssessmentCandidate {
    LearningAssessmentCandidate {
        id: id(seed),
        outcome_ids: vec![id(2)],
        format: LearningItemFormat::SingleChoice,
        difficulty,
        prompt: format!("Which invariant holds in scenario {seed}?"),
        options: vec!["Alpha".into(), "Bravo".into(), "Charlie".into()],
        left: vec![],
        source_version_ids: vec![id(20)],
        rubric: vec![],
        key: LearningAssessmentKey::Choice(1),
        explanation: "Bravo holds.".into(),
        distractor_rationales: vec![],
    }
}

#[test]
fn form_selection_prefers_unseen_items_and_labels_required_repeats() -> Result<()> {
    let candidates = vec![candidate(10, 3), candidate(11, 3), candidate(12, 4)];
    let exposed = HashSet::from([id(10), id(11)]);
    let form = build_assessment_form(&blueprint(), &candidates, &exposed, id(30), 40)?;
    assert_eq!(form.items.len(), 2);
    assert_eq!(form.items[0].id, id(12));
    assert!(!form.items[0].previously_exposed);
    assert!(form.items[1].previously_exposed);
    Ok(())
}

#[test]
fn a_blueprint_is_valid_whatever_its_title_and_instructions_run_to() {
    let mut long = blueprint();
    long.title = "A title that names the whole lesson. ".repeat(10);
    long.instructions = "Answer every question you can. ".repeat(200);
    let checked = validate_blueprint(&long);
    assert!(checked.is_ok(), "{:?}", checked.err());
    long.title = "  ".into();
    assert!(validate_blueprint(&long).is_err());
}

#[test]
fn insufficient_blueprint_coverage_is_an_error_instead_of_synthetic_content() {
    assert!(build_assessment_form(
        &blueprint(),
        &[candidate(10, 3)],
        &HashSet::new(),
        id(30),
        40,
    )
    .is_err());
}

#[test]
fn shown_options_follow_the_stored_order() -> Result<()> {
    let candidates = vec![candidate(10, 3), candidate(11, 3)];
    let form = build_assessment_form(&blueprint(), &candidates, &HashSet::new(), id(30), 40)?;
    for item in &form.items {
        let authored = candidates.iter().find(|c| c.id == item.id).unwrap();
        for (shown, original) in item.option_order.iter().enumerate() {
            assert_eq!(item.options[shown], authored.options[*original]);
        }
    }
    Ok(())
}

#[test]
fn a_number_against_a_key_only_in_words_goes_to_the_judge() {
    let words = LearningTextKey {
        variants: vec!["half".into(), "one half".into()],
        numeric: None,
    };
    assert_eq!(text_matches("0.5", &words), None);
    let numbers = LearningTextKey {
        variants: vec!["3".into()],
        numeric: None,
    };
    assert_eq!(text_matches("3.0", &numbers), Some(true));
    assert_eq!(text_matches("4", &numbers), Some(false));
}

fn choice_item(prompt: &str, options: &[&str], key: usize) -> LearningAssessmentCandidate {
    LearningAssessmentCandidate {
        prompt: prompt.into(),
        options: options.iter().map(|o| (*o).to_owned()).collect(),
        key: LearningAssessmentKey::Choice(key),
        ..candidate(30, 3)
    }
}

#[test]
fn choices_that_differ_only_in_path_punctuation_are_different_choices() {
    let sets: [(&str, [&str; 4], usize); 3] = [
        (
            "Which path is the learner's own folder?",
            ["/home/learner/videos", "/home", "/home/learner", "/"],
            2,
        ),
        (
            "Which command enters the sibling folder from here?",
            [
                "cd music",
                "cd /home/learner/music",
                "cd /music",
                "cd ../../music",
            ],
            3,
        ),
        (
            "Which command enters the folder one level up?",
            ["cd notes", "cd ../../notes", "cd /notes", "cd ../notes"],
            3,
        ),
    ];
    for (prompt, options, key) in sets {
        let item = choice_item(prompt, &options, key);
        assert!(validate_candidate(&item).is_ok(), "{options:?}");
    }
}

#[test]
fn choices_a_reader_cannot_tell_apart_or_blank_choices_are_still_rejected() {
    let same = choice_item(
        "Which program reads the typed commands?",
        &["The shell", "The shell.", "The kernel", "The editor"],
        2,
    );
    assert!(validate_candidate(&same).is_err());
    let blank = choice_item(
        "Which program reads the typed commands?",
        &["The shell", "  ", "The kernel", "The editor"],
        2,
    );
    assert!(validate_candidate(&blank).is_err());
}

#[test]
fn path_like_values_are_distinct_in_a_multi_select_item() {
    let item = LearningAssessmentCandidate {
        format: LearningItemFormat::MultiSelect,
        key: LearningAssessmentKey::Choices([0, 1].into_iter().collect()),
        ..choice_item(
            "Which of these are absolute paths?",
            &["/home/learner/notes", "/home/learner", "home", "/"],
            0,
        )
    };
    assert!(validate_candidate(&item).is_ok());
}

fn owned(options: &[&str]) -> Vec<String> {
    options.iter().map(|o| (*o).to_owned()).collect()
}

#[test]
fn choices_that_differ_only_by_letter_case_are_distinct() {
    assert!(distinct_options(&owned(&[
        "Alpha", "alpha", "Bravo", "Charlie"
    ])));
}

#[test]
fn choices_that_differ_only_by_an_accent_are_distinct() {
    assert!(distinct_options(&owned(&[
        "Resume",
        "R\u{e9}sum\u{e9}",
        "Bravo",
        "Charlie"
    ])));
}

#[test]
fn choices_that_differ_only_by_a_compatibility_form_are_distinct() {
    assert!(distinct_options(&owned(&[
        "x\u{b2}", "x2", "Bravo", "Charlie"
    ])));
}

#[test]
fn choices_a_reader_cannot_tell_apart_are_not_distinct() {
    let pairs: [(&str, &str); 5] = [
        ("It\u{2019}s done", "It's done"),
        ("\u{201C}Alpha\u{201D}", "\"Alpha\""),
        ("Alpha   beta", "Alpha beta"),
        ("Alpha beta.", "Alpha beta"),
        ("R\u{e9}sum\u{e9}", "Re\u{301}sume\u{301}"),
    ];
    for (a, b) in pairs {
        assert!(
            !distinct_options(&owned(&[a, b, "Bravo", "Charlie"])),
            "{a:?} {b:?}"
        );
    }
}

fn requirement_of(
    outcome: u128,
    format: LearningItemFormat,
    count: usize,
    difficulty_min: u8,
    difficulty_max: u8,
) -> LearningBlueprintRequirement {
    LearningBlueprintRequirement {
        outcome_id: id(outcome),
        format,
        count,
        difficulty_min,
        difficulty_max,
    }
}

fn candidate_of(
    seed: u128,
    outcome: u128,
    format: LearningItemFormat,
    difficulty: u8,
) -> LearningAssessmentCandidate {
    LearningAssessmentCandidate {
        outcome_ids: vec![id(outcome)],
        format,
        ..candidate(seed, difficulty)
    }
}

fn picked(
    requirements: &[LearningBlueprintRequirement],
    candidates: &[LearningAssessmentCandidate],
    exposed: &HashSet<String>,
) -> std::result::Result<Vec<String>, Shortfall> {
    let refs: Vec<&LearningAssessmentCandidate> = candidates.iter().collect();
    form_items(requirements, &refs, exposed)
        .map(|items| items.iter().map(|item| item.id.clone()).collect())
}

#[test]
fn a_requirement_whose_format_has_no_item_takes_an_unused_item_of_its_outcome() -> Result<()> {
    let requirements = vec![requirement_of(2, LearningItemFormat::TrueFalse, 1, 2, 4)];
    let candidates = vec![candidate_of(10, 2, LearningItemFormat::SingleChoice, 3)];
    assert_eq!(
        picked(&requirements, &candidates, &HashSet::new()),
        Ok(vec![id(10)])
    );

    let mut asked = blueprint();
    asked.requirements = requirements;
    let form = build_assessment_form(&asked, &candidates, &HashSet::new(), id(30), 40)?;
    assert_eq!(form.items.len(), 1);
    assert_eq!(form.items[0].id, id(10));
    Ok(())
}

#[test]
fn a_fallback_never_takes_an_item_a_later_requirement_needs_strictly() {
    let requirements = vec![
        requirement_of(2, LearningItemFormat::TrueFalse, 1, 3, 3),
        requirement_of(2, LearningItemFormat::SingleChoice, 1, 2, 4),
    ];
    // Item 10 is the nearest fallback for the first requirement and the only strict item of the second;
    // item 11 is single choice but outside the second requirement's difficulty range.
    let candidates = vec![
        candidate_of(10, 2, LearningItemFormat::SingleChoice, 3),
        candidate_of(11, 2, LearningItemFormat::SingleChoice, 1),
    ];
    assert_eq!(
        picked(&requirements, &candidates, &HashSet::new()),
        Ok(vec![id(11), id(10)])
    );
}

#[test]
fn a_shortfall_remains_when_the_outcome_has_too_few_unused_items_in_any_format() {
    let requirements = vec![requirement_of(2, LearningItemFormat::SingleChoice, 3, 2, 4)];
    let candidates = vec![
        candidate_of(10, 2, LearningItemFormat::SingleChoice, 3),
        candidate_of(11, 2, LearningItemFormat::Ordering, 1),
        candidate_of(12, 5, LearningItemFormat::SingleChoice, 3),
    ];
    assert_eq!(
        picked(&requirements, &candidates, &HashSet::new()),
        Err(Shortfall {
            requirement: requirements[0].clone(),
            eligible: 2,
        })
    );
}

#[test]
fn fallback_items_come_unseen_first_then_nearest_the_middle_then_by_id() {
    let requirements = vec![requirement_of(2, LearningItemFormat::TrueFalse, 2, 3, 3)];
    let candidates = vec![
        candidate_of(23, 2, LearningItemFormat::SingleChoice, 5),
        candidate_of(22, 2, LearningItemFormat::SingleChoice, 2),
        candidate_of(21, 2, LearningItemFormat::SingleChoice, 4),
        candidate_of(20, 2, LearningItemFormat::SingleChoice, 3),
    ];
    let exposed = HashSet::from([id(20)]);
    assert_eq!(
        picked(&requirements, &candidates, &exposed),
        Ok(vec![id(21), id(22)])
    );

    let all = vec![requirement_of(2, LearningItemFormat::TrueFalse, 4, 3, 3)];
    assert_eq!(
        picked(&all, &candidates, &exposed),
        Ok(vec![id(21), id(22), id(23), id(20)])
    );
}

#[test]
fn an_item_of_another_outcome_is_never_a_fallback() {
    let requirements = vec![requirement_of(2, LearningItemFormat::TrueFalse, 1, 3, 3)];
    // Item 10 is nearer and sorts first, but belongs to another outcome.
    let candidates = vec![
        candidate_of(10, 5, LearningItemFormat::SingleChoice, 3),
        candidate_of(11, 2, LearningItemFormat::SingleChoice, 1),
    ];
    assert_eq!(
        picked(&requirements, &candidates, &HashSet::new()),
        Ok(vec![id(11)])
    );

    let two = vec![requirement_of(2, LearningItemFormat::TrueFalse, 2, 3, 3)];
    assert_eq!(
        picked(&two, &candidates, &HashSet::new()),
        Err(Shortfall {
            requirement: two[0].clone(),
            eligible: 1,
        })
    );
}
