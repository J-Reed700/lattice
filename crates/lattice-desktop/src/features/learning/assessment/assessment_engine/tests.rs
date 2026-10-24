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
