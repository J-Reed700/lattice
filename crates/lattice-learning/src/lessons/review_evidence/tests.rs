use super::*;

/// A reply listing `decisions` in order, each under its issue ID.
fn reply(decisions: &[(&str, Value)]) -> Value {
    let listed: Vec<Value> = decisions
        .iter()
        .map(|(id, decision)| {
            let mut decision = decision.clone();
            if let Some(fields) = decision.as_object_mut() {
                fields.insert("issueId".into(), json!(id));
            }
            decision
        })
        .collect();
    json!({ "decisions": listed })
}

fn proposal(text: &str) -> Vec<ReviewFinding> {
    vec![ReviewFinding::reviewer(1, None, text)]
}

/// The check of a first answer, which a broken decision fails.
fn first(
    raw: &str,
    issues: &[ReviewFinding],
    evidence: &HashMap<String, EvidenceKind>,
) -> Result<Vec<ReviewFinding>> {
    accepted(raw, issues, evidence, false, &mut HashMap::new())
}

fn placed(findings: Vec<ReviewFinding>) -> Vec<(String, Option<FindingSite>)> {
    findings.into_iter().map(|f| (f.note, f.site)).collect()
}

#[tokio::test]
async fn every_matching_reference_is_offered_while_the_window_has_room() -> Result<()> {
    use crate::dto::LearningSourceDto;
    use crate::lessons::course_generation_tests::ScriptedModel;
    let sources: Vec<_> = (0..30)
        .map(|i| LearningSourceDto {
            id: format!("source-{i}"),
            title: format!("Reference {i}"),
            url: None,
            excerpt: format!("Aggregate roots enforce invariants in document {i}."),
            acquired_at: 1,
        })
        .collect();
    let references = ReferenceCollection::lexical(&sources)?;
    let model = ScriptedModel {
        outputs: std::sync::Mutex::new(vec![json!({"decisions":[{"issueId":"issue-0","verdict":"not_established","basis":"external_fact","evidenceIds":[],"requirementId":null,"reason":"No passage establishes this correction."}]}).to_string()].into()),
        prompts: Default::default(),
    };
    let issues = proposal("Aggregate roots do not enforce invariants.");
    check(
        &model,
        &issues,
        &json!({}),
        &json!({"blocks":[],"questions":[]}),
        &references,
        None,
    )
    .await?;
    let prompts = model.prompts.lock().unwrap();
    let request: Value = serde_json::from_str(prompts[0].rsplit("\n\n").next().unwrap())?;
    // Not the former eight: every passage that matches and fits.
    assert_eq!(request["referencePassages"].as_array().unwrap().len(), 30);
    Ok(())
}

#[tokio::test]
async fn a_malformed_actionable_finding_is_asked_for_once_more() -> Result<()> {
    use crate::lessons::course_generation_tests::ScriptedModel;
    let decision = |ids| {
        json!({"decisions":[{"issueId":"issue-0","verdict":"actionable","basis":"lesson_contract",
        "evidenceIds":ids,"requirementId":"requirements","partId":"blocks-0","reason":"The candidate omitted the required deliverable."}]}).to_string()
    };
    let rejected = decision(json!([]));
    let model = ScriptedModel {
        outputs: std::sync::Mutex::new(
            vec![rejected.clone(), decision(json!(["blocks-0"]))].into(),
        ),
        prompts: Default::default(),
    };
    let issues = proposal("The exercise omitted its required deliverable.");
    let sources = ReferenceCollection::lexical(&[])?;
    assert_eq!(
        placed(
            check(
                &model,
                &issues,
                &json!({"deliverable":"Record the resulting state."}),
                &json!({"blocks":[{"body":"Read the instructions."}],"questions":[]}),
                &sources,
                None
            )
            .await?
        ),
        vec![(issues[0].note.clone(), Some(FindingSite::Section(0)))]
    );
    // The refused decision is asked for once more, with its reason.
    assert_eq!(model.prompts.lock().unwrap().len(), 2);
    Ok(())
}

#[tokio::test]
async fn a_decision_broken_again_leaves_its_finding_not_established() -> Result<()> {
    use crate::lessons::course_generation_tests::ScriptedModel;
    let decision = |ids: Value, reason: &str| json!({"verdict":"actionable","basis":"lesson_contract","evidenceIds":ids,"requirementId":"requirements","partId":"blocks-0","reason":reason});
    // The first decision cites nothing, in both answers.
    let answer = reply(&[
        (
            "issue-0",
            decision(json!([]), "The candidate omitted the required deliverable."),
        ),
        (
            "issue-1",
            decision(
                json!(["blocks-0"]),
                "The exercise never asks for the recorded state.",
            ),
        ),
    ]);
    let model = ScriptedModel {
        outputs: std::sync::Mutex::new(vec![answer.to_string(), answer.to_string()].into()),
        prompts: Default::default(),
    };
    let issues = [
        proposal("The exercise omitted its required deliverable."),
        proposal("The exercise never asks for the recorded state."),
    ]
    .concat();
    let sources = ReferenceCollection::lexical(&[])?;
    let established = check(
        &model,
        &issues,
        &json!({"deliverable":"Record the resulting state."}),
        &json!({"blocks":[{"body":"Read the instructions."}],"questions":[]}),
        &sources,
        None,
    )
    .await?;
    assert_eq!(
        placed(established),
        vec![(issues[1].note.clone(), Some(FindingSite::Section(0)))]
    );
    assert_eq!(model.prompts.lock().unwrap().len(), 2);
    Ok(())
}

/// The notes `check` establishes from two scripted answers, the second
/// the one asked for again.
async fn established_from(
    first: &Value,
    second: &Value,
    issues: &[ReviewFinding],
) -> Result<Vec<String>> {
    use crate::lessons::course_generation_tests::ScriptedModel;
    let model = ScriptedModel {
        outputs: std::sync::Mutex::new(vec![first.to_string(), second.to_string()].into()),
        prompts: Default::default(),
    };
    let sources = ReferenceCollection::lexical(&[])?;
    let established = check(
        &model,
        issues,
        &json!({"deliverable":"Record the resulting state."}),
        &json!({"blocks":[{"body":"Read the instructions."}],"questions":[]}),
        &sources,
        None,
    )
    .await?;
    assert_eq!(model.prompts.lock().unwrap().len(), 2);
    Ok(established
        .into_iter()
        .map(|finding| finding.note)
        .collect())
}

#[tokio::test]
async fn a_decision_broken_only_when_asked_again_keeps_the_grounded_one_the_first_answer_gave(
) -> Result<()> {
    let decision = |verdict: &str, ids: Value, requirement: Value| json!({"verdict":verdict,"basis":"lesson_contract","evidenceIds":ids,"requirementId":requirement,"partId":"blocks-0","reason":"The exercise does not ask for what the contract requires."});
    let grounded = decision("actionable", json!(["blocks-0"]), json!("requirements"));
    let uncited = decision("actionable", json!([]), json!("requirements"));
    let issues = [
        proposal("The exercise omitted its required deliverable."),
        proposal("The exercise never asks for the recorded state."),
    ]
    .concat();
    let notes: Vec<String> = issues.iter().map(|issue| issue.note.clone()).collect();
    // The first answer cites nothing for issue-0, so it is asked for
    // again; asked again, it cites nothing for issue-1, which the first
    // answer grounded.
    let first = reply(&[("issue-0", uncited.clone()), ("issue-1", grounded.clone())]);
    let second = reply(&[("issue-0", grounded.clone()), ("issue-1", uncited)]);
    assert_eq!(established_from(&first, &second, &issues).await?, notes);
    // An answer asked for again that breaks the protocol on another
    // verdict does not take the first answer's.
    let withdrawn = decision("not_established", json!([]), json!("requirements"));
    let second = reply(&[("issue-0", grounded), ("issue-1", withdrawn)]);
    assert_eq!(
        established_from(&first, &second, &issues).await?,
        notes[..1].to_vec()
    );
    Ok(())
}

#[test]
fn flat_responses_still_require_the_right_evidence_for_internal_conflicts() -> Result<()> {
    let issues = proposal("The two statements in the candidate are incompatible.");
    let evidence = HashMap::from([
        ("questions-0".into(), EvidenceKind::Candidate),
        ("reference-0".into(), EvidenceKind::Reference),
        ("requirements".into(), EvidenceKind::Requirements),
    ]);
    let response = |ids: Value, requirement: Value, verdict: &str| {
        reply(&[("issue-0", json!({
        "verdict":verdict,"basis":"internal_conflict","evidenceIds":ids,
        "requirementId":requirement,"partId":"questions-0","reason":"The candidate gives incompatible descriptions of the same scenario."
    }))]).to_string()
    };
    for ids in [
        json!([]),
        json!(["reference-0"]),
        json!(["questions-0", "reference-0"]),
    ] {
        assert!(first(
            &response(ids, Value::Null, "actionable"),
            &issues,
            &evidence
        )
        .is_err());
    }
    assert!(first(
        &response(json!(["questions-0"]), json!("requirements"), "actionable"),
        &issues,
        &evidence
    )
    .is_err());
    assert_eq!(
        placed(first(
            &response(json!(["questions-0"]), Value::Null, "actionable"),
            &issues,
            &evidence
        )?),
        vec![(issues[0].note.clone(), Some(FindingSite::Question(0)))]
    );
    // Free-text reasons never silently promote a negative model verdict.
    assert!(first(
        &response(json!([]), Value::Null, "not_established"),
        &issues,
        &evidence
    )?
    .is_empty());
    Ok(())
}

#[test]
fn external_factual_corrections_require_reference_evidence() -> Result<()> {
    let issues = proposal("A proposed factual correction.");
    let evidence = HashMap::from([
        ("blocks-0".into(), EvidenceKind::Candidate),
        ("reference-0".into(), EvidenceKind::Reference),
    ]);
    let decision = |ids: Value, verdict: &str| {
        reply(&[("issue-0", json!({"verdict":verdict,"basis":"external_fact","evidenceIds":ids,"requirementId":null,"partId":"lesson","reason":"The supplied evidence supports this comparison."}))]).to_string()
    };
    assert!(first(
        &decision(json!(["blocks-0"]), "actionable"),
        &issues,
        &evidence
    )
    .is_err());
    assert!(first(
        &decision(json!(["invented"]), "actionable"),
        &issues,
        &evidence
    )
    .is_err());
    // Reference evidence names no part of the lesson: the whole lesson.
    assert_eq!(
        placed(first(
            &decision(json!(["reference-0"]), "actionable"),
            &issues,
            &evidence
        )?),
        vec![(issues[0].note.clone(), Some(FindingSite::Lesson))]
    );
    // A finding the section review already placed keeps its section.
    let section = vec![ReviewFinding::reviewer(
        1,
        Some(FindingSite::Section(4)),
        "Section 5 (candidate blocks-4): a proposed factual correction.",
    )];
    assert_eq!(
        first(
            &decision(json!(["reference-0"]), "actionable"),
            &section,
            &evidence
        )?,
        section
    );
    assert!(first(&decision(json!([]), "not_established"), &issues, &evidence)?.is_empty());
    assert!(first("{}", &issues, &evidence).is_err());
    Ok(())
}

#[test]
fn instructional_corrections_need_both_the_contract_and_candidate() -> Result<()> {
    let issues = proposal("The exercise omits a required deliverable.");
    let evidence = HashMap::from([
        ("blocks-0".into(), EvidenceKind::Candidate),
        ("requirements".into(), EvidenceKind::Requirements),
    ]);
    let decision = |ids: Value, requirement: Value| {
        reply(&[("issue-0", json!({"verdict":"actionable","basis":"lesson_contract","evidenceIds":ids,"requirementId":requirement,"partId":"blocks-0","reason":"The task omits the deliverable explicitly required by the contract."}))]).to_string()
    };
    assert!(first(
        &decision(json!(["blocks-0"]), json!(null)),
        &issues,
        &evidence
    )
    .is_err());
    assert_eq!(
        placed(first(
            &decision(json!(["blocks-0"]), json!("requirements")),
            &issues,
            &evidence
        )?),
        vec![(issues[0].note.clone(), Some(FindingSite::Section(0)))]
    );
    Ok(())
}

#[test]
fn a_short_or_long_reason_is_read_not_refused() -> Result<()> {
    let issues = proposal("The two statements in the candidate are incompatible.");
    let evidence = HashMap::from([("questions-0".into(), EvidenceKind::Candidate)]);
    let long = "The candidate gives incompatible descriptions of the same scenario. ".repeat(20);
    assert!(long.trim().chars().count() > 700);
    for reason in ["Yes.", long.as_str()] {
        let response = reply(&[(
            "issue-0",
            json!({
            "verdict":"actionable","basis":"internal_conflict","evidenceIds":["questions-0"],
            "requirementId":null,"partId":"questions-0","reason":reason}),
        )])
        .to_string();
        assert_eq!(
            placed(first(&response, &issues, &evidence)?),
            vec![(issues[0].note.clone(), Some(FindingSite::Question(0)))]
        );
    }
    Ok(())
}

#[test]
fn a_blank_reason_is_still_refused() {
    let issues = proposal("The two statements in the candidate are incompatible.");
    let evidence = HashMap::from([("questions-0".into(), EvidenceKind::Candidate)]);
    let response = reply(&[(
        "issue-0",
        json!({
        "verdict":"actionable","basis":"internal_conflict","evidenceIds":["questions-0"],
        "requirementId":null,"partId":"questions-0","reason":"   "}),
    )])
    .to_string();
    assert!(first(&response, &issues, &evidence).is_err());
}

fn parts_evidence() -> HashMap<String, EvidenceKind> {
    HashMap::from([
        ("blocks-1".into(), EvidenceKind::Candidate),
        ("blocks-2".into(), EvidenceKind::Candidate),
        ("blocks-4".into(), EvidenceKind::Candidate),
        ("questions-0".into(), EvidenceKind::Candidate),
        ("reference-0".into(), EvidenceKind::Reference),
        ("requirements".into(), EvidenceKind::Requirements),
    ])
}

/// A decision on issue-0 with the given verdict and basis, citing `ids`;
/// `part` is its partId, left out when `None`.
fn part_decision(verdict: &str, basis: &str, ids: Value, part: Option<&str>) -> String {
    let mut decision = json!({"verdict":verdict,"basis":basis,"evidenceIds":ids,"requirementId":null,"reason":"The supplied evidence settles this proposal."});
    if let (Some(part), Some(fields)) = (part, decision.as_object_mut()) {
        fields.insert("partId".into(), json!(part));
    }
    reply(&[("issue-0", decision)]).to_string()
}

#[test]
fn an_issue_is_placed_at_the_part_the_decision_names_not_the_lowest_it_cites() -> Result<()> {
    let issues = proposal("The worked example contradicts its explanation.");
    let evidence = parts_evidence();
    let both = json!(["blocks-2", "blocks-4"]);
    for (part, site) in [
        ("blocks-4", FindingSite::Section(4)),
        ("blocks-2", FindingSite::Section(2)),
        ("questions-0", FindingSite::Question(0)),
    ] {
        assert_eq!(
            placed(first(
                &part_decision("actionable", "internal_conflict", both.clone(), Some(part)),
                &issues,
                &evidence
            )?),
            vec![(issues[0].note.clone(), Some(site))]
        );
    }
    Ok(())
}

#[test]
fn an_external_fact_issue_is_placed_where_the_decision_says_not_at_the_references() -> Result<()> {
    let issues = proposal("A proposed factual correction.");
    let evidence = parts_evidence();
    for (part, site) in [
        ("blocks-1", FindingSite::Section(1)),
        ("lesson", FindingSite::Lesson),
    ] {
        assert_eq!(
            placed(first(
                &part_decision(
                    "actionable",
                    "external_fact",
                    json!(["reference-0"]),
                    Some(part)
                ),
                &issues,
                &evidence
            )?),
            vec![(issues[0].note.clone(), Some(site))]
        );
    }
    Ok(())
}

#[test]
fn an_actionable_decision_without_a_candidate_part_or_lesson_is_refused() -> Result<()> {
    let issues = proposal("A proposed factual correction.");
    let evidence = parts_evidence();
    let decide = |part| {
        first(
            &part_decision("actionable", "external_fact", json!(["reference-0"]), part),
            &issues,
            &evidence,
        )
    };
    assert!(decide(Some("blocks-1"))?.len() == 1);
    for part in [
        None,
        Some("requirements"),
        Some("reference-0"),
        Some("blocks-9"),
        Some(""),
    ] {
        assert!(decide(part).is_err(), "{part:?}");
    }
    Ok(())
}

#[test]
fn a_not_established_decision_needs_no_part_and_its_part_is_ignored() -> Result<()> {
    let issues = proposal("A proposed factual correction.");
    let evidence = parts_evidence();
    for part in [None, Some("blocks-1"), Some("requirements")] {
        assert!(first(
            &part_decision("not_established", "external_fact", json!([]), part),
            &issues,
            &evidence
        )?
        .is_empty());
    }
    Ok(())
}

#[test]
fn a_finding_that_already_has_a_site_keeps_it_whatever_the_decision_names() -> Result<()> {
    let evidence = parts_evidence();
    let section = vec![ReviewFinding::reviewer(
        1,
        Some(FindingSite::Section(4)),
        "Section 5 (candidate blocks-4): a proposed factual correction.",
    )];
    for part in ["blocks-1", "questions-0", "lesson"] {
        assert_eq!(
            first(
                &part_decision(
                    "actionable",
                    "external_fact",
                    json!(["reference-0"]),
                    Some(part)
                ),
                &section,
                &evidence
            )?,
            section
        );
    }
    Ok(())
}

#[tokio::test]
async fn each_candidate_part_is_offered_with_the_label_the_user_sees() -> Result<()> {
    use crate::lessons::course_generation_tests::ScriptedModel;
    let model = ScriptedModel {
        outputs: std::sync::Mutex::new(vec![json!({"decisions":[{"issueId":"issue-0","verdict":"not_established","basis":"external_fact","evidenceIds":[],"requirementId":null,"reason":"No passage establishes this correction."}]}).to_string()].into()),
        prompts: Default::default(),
    };
    let sources = ReferenceCollection::lexical(&[])?;
    check(
        &model,
        &proposal("Section 2 of the lesson is wrong."),
        &json!({}),
        &json!({"blocks":[{"body":"one"},{"body":"two"}],"questions":[{"prompt":"ask"}]}),
        &sources,
        None,
    )
    .await?;
    let prompts = model.prompts.lock().unwrap();
    assert!(prompts[0].contains("partId"));
    let request: Value = serde_json::from_str(prompts[0].rsplit("\n\n").next().unwrap())?;
    let parts: Vec<_> = request["candidateParts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|part| (part["id"].as_str().unwrap(), part["label"].as_str()))
        .collect();
    assert_eq!(
        parts,
        [
            ("blocks-0", Some("Section 1")),
            ("blocks-1", Some("Section 2")),
            ("questions-0", Some("Question 1")),
        ]
    );
    Ok(())
}
