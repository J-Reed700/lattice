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
    accepted(
        raw,
        issues,
        Beside::NONE,
        evidence,
        false,
        &mut HashMap::new(),
    )
    .map(|checked| checked.established)
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
        Beside::NONE,
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
                Beside::NONE,
                &json!({"deliverable":"Record the resulting state."}),
                &json!({"blocks":[{"body":"Read the instructions."}],"questions":[]}),
                &sources,
                None
            )
            .await?
            .established
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
        Beside::NONE,
        &json!({"deliverable":"Record the resulting state."}),
        &json!({"blocks":[{"body":"Read the instructions."}],"questions":[]}),
        &sources,
        None,
    )
    .await?
    .established;
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
        Beside::NONE,
        &json!({"deliverable":"Record the resulting state."}),
        &json!({"blocks":[{"body":"Read the instructions."}],"questions":[]}),
        &sources,
        None,
    )
    .await?
    .established;
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
        Beside::NONE,
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

const LEFT_TO_SENTENCE: &str = " otherChecks lists the checks that have already read this whole candidate, each with what it decides. recordedFindings, when given, are the defects those checks recorded and will see corrected. For every proposal give in leftTo the id of the recorded finding that describes the same defect; when no recorded finding does, the id of the check that decides what the proposal is about; and none when no check decides it. What another check decides is not corrected from a review finding, whether that check recorded the defect or found none: return not_established for every proposal whose leftTo is not none.";

const DECIDES: [&str; 2] = [
    "whether the kettle is full; whether the lid is shut",
    "whether the rope is tied",
];

fn recorded_findings() -> Vec<ReviewFinding> {
    vec![
        ReviewFinding::rule(
            crate::review_findings::Rule::Schema,
            crate::review_findings::FindingKind::Structure,
            FindingSite::Section(0),
            "Repair note for the first recorded defect.",
        )
        .with_summary("The first recorded defect."),
        ReviewFinding::rule(
            crate::review_findings::Rule::Schema,
            crate::review_findings::FindingKind::Structure,
            FindingSite::Question(1),
            "The second recorded defect.",
        ),
    ]
}

fn gate_decision(verdict: &str, left_to: &str) -> String {
    let grounded = verdict == "actionable";
    json!({"decisions":[{"issueId":"issue-0","verdict":verdict,"basis":"internal_conflict",
        "evidenceIds":if grounded { json!(["blocks-0"]) } else { json!([]) },
        "requirementId":null,"partId":if grounded { "blocks-0" } else { "lesson" },
        "leftTo":left_to,"reason":"The candidate contradicts itself."}]})
    .to_string()
}

/// What the gate sent in one call: system text, user text and schema.
struct Sent {
    system: String,
    user: Value,
    schema: Value,
}

/// The scripted model, keeping what each answer-bearing call was sent.
struct Seen {
    inner: crate::lessons::course_generation_tests::ScriptedModel,
    sent: std::sync::Mutex<Vec<Sent>>,
}

#[async_trait::async_trait]
impl LLMPort for Seen {
    async fn complete(
        &self,
        request: &lattice_core::application::ports::llm_port::CompletionRequest,
    ) -> Result<lattice_core::application::ports::llm_port::CompletionResponse> {
        if !crate::lessons::structured_call::is_draft_request(request) {
            let (system, user) = crate::lessons::structured_call::task_messages(request);
            self.sent.lock().unwrap().push(Sent {
                system: system.to_owned(),
                user: serde_json::from_str(user).unwrap(),
                schema: request.json_schema.clone().unwrap(),
            });
        }
        self.inner.complete(request).await
    }
    fn model_name(&self) -> &str {
        self.inner.model_name()
    }
    fn count_tokens(&self, text: &str) -> usize {
        self.inner.count_tokens(text)
    }
    fn max_context_tokens(&self) -> usize {
        self.inner.max_context_tokens()
    }
    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
}

/// The gate asked about one proposal while `beside` stand, answering
/// `outputs`: what it sent and the findings it established.
async fn gate_beside(
    outputs: Vec<String>,
    beside: Beside<'_>,
) -> Result<(Vec<Sent>, Vec<ReviewFinding>)> {
    let (sent, checked) = gate_ruled(outputs, beside).await?;
    Ok((sent, checked.established))
}

/// `gate_beside`, with everything the gate made of the proposal.
async fn gate_ruled(outputs: Vec<String>, beside: Beside<'_>) -> Result<(Vec<Sent>, Checked)> {
    use crate::dto::LearningSourceDto;
    use crate::lessons::course_generation_tests::ScriptedModel;
    let sources = vec![LearningSourceDto {
        id: "source-0".into(),
        title: "Reference".into(),
        url: None,
        excerpt: "The kettle boils when the water reaches its limit.".into(),
        acquired_at: 1,
    }];
    let references = ReferenceCollection::lexical(&sources)?;
    let model = Seen {
        inner: ScriptedModel {
            outputs: std::sync::Mutex::new(outputs.into()),
            prompts: Default::default(),
        },
        sent: Default::default(),
    };
    let checked = check(
        &model,
        &proposal("The kettle is described twice."),
        beside,
        &json!({}),
        &json!({"blocks":[{"title":"Part"}],"questions":[]}),
        &references,
        None,
    )
    .await?;
    Ok((model.sent.into_inner().unwrap(), checked))
}

/// The gate asked while `recorded` stand and no other check is listed.
async fn gate_with(
    outputs: Vec<String>,
    recorded: &[ReviewFinding],
) -> Result<(Vec<Sent>, Vec<ReviewFinding>)> {
    gate_beside(
        outputs,
        Beside {
            decides: &[],
            recorded,
        },
    )
    .await
}

/// The gate asked while two other checks have read the candidate.
async fn gate_checks(
    outputs: Vec<String>,
    recorded: &[ReviewFinding],
) -> Result<(Vec<Sent>, Vec<ReviewFinding>)> {
    gate_beside(
        outputs,
        Beside {
            decides: &DECIDES,
            recorded,
        },
    )
    .await
}

#[tokio::test]
async fn the_gate_is_shown_the_checks_and_the_recorded_findings_and_asked_where_each_is_left(
) -> Result<()> {
    let (sent, _) = gate_checks(
        vec![gate_decision("not_established", "none")],
        &recorded_findings(),
    )
    .await?;
    assert_eq!(
        sent[0].user["otherChecks"],
        json!([
            {"id":"check-0","decides":DECIDES[0]},
            {"id":"check-1","decides":DECIDES[1]}
        ])
    );
    assert_eq!(
        sent[0].user["recordedFindings"],
        json!([
            {"id":"recorded-0","part":"Section 1","defect":"The first recorded defect."},
            {"id":"recorded-1","part":"Question 2","defect":"The second recorded defect."}
        ])
    );
    assert_eq!(sent[0].system, format!("{SYSTEM}{LEFT_TO_SENTENCE}"));
    let decision = &sent[0].schema["properties"]["decisions"]["items"];
    assert!(decision["required"]
        .as_array()
        .unwrap()
        .contains(&json!("leftTo")));
    assert_eq!(
        decision["properties"]["leftTo"]["enum"],
        json!(["recorded-0", "recorded-1", "check-0", "check-1", "none"])
    );
    assert!(decision["properties"].get("recordedAs").is_none());
    Ok(())
}

#[tokio::test]
async fn the_gate_is_shown_the_recorded_findings_alone_as_before() -> Result<()> {
    let (sent, _) = gate_with(
        vec![gate_decision("not_established", "none")],
        &recorded_findings(),
    )
    .await?;
    assert!(sent[0].user.get("otherChecks").is_none());
    assert_eq!(
        sent[0].user["recordedFindings"].as_array().map(Vec::len),
        Some(2)
    );
    assert_eq!(sent[0].system, format!("{SYSTEM}{LEFT_TO_SENTENCE}"));
    let decision = &sent[0].schema["properties"]["decisions"]["items"];
    assert_eq!(
        decision["properties"]["leftTo"]["enum"],
        json!(["recorded-0", "recorded-1", "none"])
    );
    Ok(())
}

#[tokio::test]
async fn with_checks_and_no_recorded_findings_only_the_checks_are_offered() -> Result<()> {
    let (sent, _) = gate_checks(vec![gate_decision("not_established", "none")], &[]).await?;
    assert_eq!(
        sent[0].user["otherChecks"].as_array().map(Vec::len),
        Some(2)
    );
    assert!(sent[0].user.get("recordedFindings").is_none());
    assert_eq!(sent[0].system, format!("{SYSTEM}{LEFT_TO_SENTENCE}"));
    let decision = &sent[0].schema["properties"]["decisions"]["items"];
    assert!(decision["required"]
        .as_array()
        .unwrap()
        .contains(&json!("leftTo")));
    assert_eq!(
        decision["properties"]["leftTo"]["enum"],
        json!(["check-0", "check-1", "none"])
    );
    Ok(())
}

#[tokio::test]
async fn beside_nothing_nothing_about_other_checks_is_asked() -> Result<()> {
    let (sent, _) =
        gate_beside(vec![gate_decision("not_established", "none")], Beside::NONE).await?;
    assert!(sent[0].user.get("recordedFindings").is_none());
    assert!(sent[0].user.get("otherChecks").is_none());
    assert_eq!(sent[0].system, SYSTEM);
    let decision = &sent[0].schema["properties"]["decisions"]["items"];
    assert_eq!(
        decision["required"],
        json!([
            "issueId",
            "verdict",
            "basis",
            "evidenceIds",
            "requirementId",
            "reason",
            "partId"
        ])
    );
    assert!(decision["properties"].get("leftTo").is_none());
    assert!(decision["properties"].get("recordedAs").is_none());
    Ok(())
}

#[tokio::test]
async fn a_proposal_naming_a_recorded_finding_is_not_established_and_not_asked_again() -> Result<()>
{
    let (sent, established) = gate_with(
        vec![gate_decision("actionable", "recorded-0")],
        &recorded_findings(),
    )
    .await?;
    assert!(established.is_empty());
    assert_eq!(sent.len(), 1);
    Ok(())
}

#[tokio::test]
async fn a_proposal_naming_no_check_is_established_as_before() -> Result<()> {
    let (_, established) = gate_checks(
        vec![gate_decision("actionable", "none")],
        &recorded_findings(),
    )
    .await?;
    assert_eq!(established.len(), 1);
    assert_eq!(established[0].site, Some(FindingSite::Section(0)));
    Ok(())
}

#[tokio::test]
async fn a_not_established_proposal_naming_a_recorded_finding_stays_so() -> Result<()> {
    let (_, established) = gate_with(
        vec![gate_decision("not_established", "recorded-1")],
        &recorded_findings(),
    )
    .await?;
    assert!(established.is_empty());
    Ok(())
}

#[tokio::test]
async fn an_actionable_proposal_left_to_a_check_is_not_established_and_not_asked_again(
) -> Result<()> {
    let (sent, established) =
        gate_checks(vec![gate_decision("actionable", "check-1")], &[]).await?;
    assert!(established.is_empty());
    assert_eq!(sent.len(), 1);
    Ok(())
}

#[tokio::test]
async fn a_proposal_left_to_a_check_needs_none_of_the_evidence_rules() -> Result<()> {
    // Actionable with no evidence and no part would break the protocol otherwise.
    let bare =
        json!({"decisions":[{"issueId":"issue-0","verdict":"actionable","basis":"external_fact",
        "evidenceIds":[],"requirementId":null,"partId":"lesson","leftTo":"check-0",
        "reason":"The kettle is described twice."}]})
        .to_string();
    let (sent, established) = gate_checks(vec![bare], &[]).await?;
    assert!(established.is_empty());
    assert_eq!(sent.len(), 1);
    Ok(())
}

#[tokio::test]
async fn a_decision_naming_an_id_that_was_not_offered_is_asked_for_again() -> Result<()> {
    for unknown in ["recorded-7", "check-9"] {
        let (sent, established) = gate_checks(
            vec![
                gate_decision("actionable", unknown),
                gate_decision("actionable", "none"),
            ],
            &recorded_findings(),
        )
        .await?;
        assert_eq!(sent.len(), 2, "{unknown}");
        assert_eq!(established.len(), 1, "{unknown}");
    }
    Ok(())
}

#[tokio::test]
async fn a_proposal_left_to_a_check_that_recorded_nothing_is_handed_back_with_the_checks_place(
) -> Result<()> {
    let issues = proposal("The kettle is described twice.");
    let (_, checked) = gate_ruled(
        vec![gate_decision("actionable", "check-1")],
        Beside {
            decides: &DECIDES,
            recorded: &[],
        },
    )
    .await?;
    assert!(checked.established.is_empty());
    assert_eq!(
        checked.left,
        vec![Left {
            check: 1,
            finding: issues[0].clone()
        }]
    );
    Ok(())
}

#[tokio::test]
async fn a_proposal_left_to_a_recorded_finding_or_to_none_is_not_handed_back() -> Result<()> {
    let beside = Beside {
        decides: &DECIDES,
        recorded: &recorded_findings(),
    };
    for left_to in ["recorded-0", "none"] {
        let (_, checked) =
            gate_ruled(vec![gate_decision("not_established", left_to)], beside).await?;
        assert!(checked.left.is_empty(), "{left_to}");
    }
    Ok(())
}

#[test]
fn a_proposal_asked_for_a_second_time_is_handed_back_once() -> Result<()> {
    let issues = proposal("The kettle is described twice.");
    let evidence = HashMap::new();
    let beside = Beside {
        decides: &DECIDES,
        recorded: &[],
    };
    let raw = gate_decision("actionable", "check-1");
    let mut kept = HashMap::new();
    let first = accepted(&raw, &issues, beside, &evidence, false, &mut kept)?;
    assert_eq!(first.left.len(), 1);
    // The second answer breaks the protocol for this issue; the first one's ruling stands.
    let broken = gate_decision("actionable", "check-9");
    let second = accepted(&broken, &issues, beside, &evidence, true, &mut kept)?;
    assert_eq!(second.left, first.left);
    Ok(())
}
