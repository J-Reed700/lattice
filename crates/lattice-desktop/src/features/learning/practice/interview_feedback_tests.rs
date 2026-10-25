use super::*;
use crate::features::learning::practical_dto::*;

const LEARNER: &str = "I preserve the intended effect. I would drop the acknowledgement and retry, then check that only one effect was committed.";
const QUOTE: &str = "An idempotent operation has the same intended effect when repeated.";

fn fixture() -> (ClosingEvidence, LearningSimulationSessionDto) {
    let question = LearningInterviewQuestionDto {
        id: "00000000-0000-4000-8000-000000000301".into(),
        round: LearningInterviewRoundKind::TechnicalDeepDive,
        question: "Assume an upload response can be lost. How would you make retrying safe?".into(),
        company_rationale: "PRIVATE_PRELIMINARY_HINT".into(),
        follow_up_probes: vec!["PRIVATE_PRELIMINARY_PROBE".into()],
        evaluation_signals: vec!["PRIVATE_PRELIMINARY_SIGNAL".into()],
    };
    let mut second = question.clone();
    second.id = "00000000-0000-4000-8000-000000000302".into();
    second.question = "How would you verify the lost-response case?".into();
    let plan = LearningInterviewPlanDto {
        company_name: "Acme".into(),
        company_url: None,
        target_role: "Engineer".into(),
        round_kind: LearningInterviewRoundKind::TechnicalDeepDive,
        company_summary: "Hypothetical constraints.".into(),
        learner_role: "Candidate".into(),
        counterpart_role: "Interviewer".into(),
        researched_source_version_ids: vec!["PRIVATE_SOURCE_VERSION".into()],
        questions: vec![question, second],
    };
    let questions = plan.questions.iter().map(|question| json!({
        "questionId":question.id,"question":question.question,
        "standard":{
            "referenceAnswer":"Preserve intended effects; verify the lost-response case.",
            "keyPoints":[{"point":"Repeated execution preserves intended effects.","evidence":"sourced","citations":[{
                "sourceId":"00000000-0000-4000-8000-000000000201",
                "versionId":"00000000-0000-4000-8000-000000000202","quote":QUOTE
            }]}],
            "acceptableAlternatives":["Any valid approach preserving effects."],
            "commonMistakes":["Requiring identical response bytes."],
            "scoringCriteria":[],"evidenceGaps":["No company hiring practices are established."]
        }
    })).collect::<Vec<_>>();
    let guides: InterviewAnswerGuides = serde_json::from_value(json!({
        "schemaVersion":1,"authorModel":"test","reviewerModel":"test","questions":questions
    }))
    .expect("guide fixture");
    let rubric = ["Evidence and reasoning", "Collaboration"]
        .into_iter()
        .enumerate()
        .map(|(index, title)| LearningRubricCriterion {
            id: format!("00000000-0000-4000-8000-{:012}", index + 401),
            title: title.into(),
            description: title.into(),
            max_points: 4,
        })
        .collect::<Vec<_>>();
    let boundary = ClosingEvidence::new(&plan, &guides, &rubric).expect("boundary fixture");
    let session = LearningSimulationSessionDto {
        id: "session".into(),
        program_id: "program".into(),
        activity_id: "activity".into(),
        activity_revision: 0,
        practice_session_id: None,
        learner_role: "Candidate".into(),
        counterpart_role: "Interviewer".into(),
        status: LearningSimulationStatus::Active,
        revision: 1,
        created_at: 0,
        updated_at: 0,
        submitted_at: None,
        turns: vec![
            LearningSimulationTurnDto {
                id: "interviewer".into(),
                ordinal: 0,
                speaker: LearningSimulationSpeaker::Counterpart,
                content: "I coordinated the team.".into(),
                citations: vec![],
                model_name: None,
                created_at: 0,
            },
            LearningSimulationTurnDto {
                id: "learner".into(),
                ordinal: 1,
                speaker: LearningSimulationSpeaker::Learner,
                content: LEARNER.into(),
                citations: vec![],
                model_name: None,
                created_at: 0,
            },
        ],
    };
    (boundary, session)
}

fn response() -> Value {
    json!({
        "referenceApproach":"Preserve intended effects across retries; verify a lost response.",
        "nextPracticeStep":"Repeat the verification with concurrent duplicates.",
        "evaluatedQuestionIds":["q1"],"citationIds":["e1"],
        "scorecard":[
            {"criterionId":"c1","status":"observed","points":4,"learnerQuotes":["I would drop the acknowledgement and retry, then check that only one effect was committed."],"feedback":"You described concrete verification after a lost response."},
            {"criterionId":"c2","status":"insufficient_evidence","points":null,"learnerQuotes":[],"feedback":"No collaboration example was provided."}
        ]
    })
}

#[test]
fn closing_choices_expand_to_owned_quotes_and_app_renders_scores() -> Result<()> {
    let (boundary, session) = fixture();
    let result = boundary.read(&response().to_string(), &session)?;
    assert_eq!(result.citations.len(), 1);
    assert_eq!(
        result.citations[0].source_id,
        "00000000-0000-4000-8000-000000000201"
    );
    assert_eq!(
        result.citations[0].version_id,
        "00000000-0000-4000-8000-000000000202"
    );
    assert_eq!(result.citations[0].quote, QUOTE);
    assert_eq!(
        result.evaluated_question_ids,
        Some(vec!["00000000-0000-4000-8000-000000000301".into()])
    );
    assert!(result
        .content
        .contains("Evidence and reasoning: 4/4 (provisional)"));
    assert!(result
        .content
        .contains("Collaboration: insufficient evidence; no points assigned"));
    assert!(result
        .content
        .contains("drop the acknowledgement and retry"));
    assert!(!result.content.contains("Collaboration: 0/4"));
    assert!(result
        .content
        .contains("No company hiring practices are established."));
    Ok(())
}

#[test]
fn closing_prompt_excludes_internal_ids_and_preliminary_hints() -> Result<()> {
    let (boundary, _) = fixture();
    let mut prompt = json!({"sourceSnapshots":[]});
    boundary.add_to_prompt(&mut prompt)?;
    let serialized = prompt.to_string();
    assert!(!serialized.contains("00000000-0000-4000-8000-"));
    assert!(!serialized.contains("PRIVATE_"));
    assert!(serialized.contains("referenceAnswer"));
    assert!(serialized.contains(QUOTE));
    assert_eq!(prompt["rubric"][0]["id"], "c1");
    assert_eq!(prompt["interviewPlan"]["questions"][0]["id"], "q1");
    Ok(())
}

#[test]
fn closing_citations_must_belong_to_evaluated_frozen_questions() {
    let (boundary, session) = fixture();
    for (questions, citations) in [
        (json!(["q1"]), json!(["e999"])),
        (json!(["q1"]), json!(["e2"])),
        (json!(["q1", "q2"]), json!(["e1"])),
        (json!(["q999"]), json!(["e1"])),
        (json!(["q1", "q1"]), json!(["e1"])),
        (json!(["q1"]), json!(["e1", "e1"])),
    ] {
        let mut value = response();
        value["evaluatedQuestionIds"] = questions;
        value["citationIds"] = citations;
        assert!(boundary.read(&value.to_string(), &session).is_err());
    }
    let mut both = response();
    both["evaluatedQuestionIds"] = json!(["q1", "q2"]);
    both["citationIds"] = json!(["e1", "e2"]);
    assert_eq!(
        boundary
            .read(&both.to_string(), &session)
            .expect("both owned choices")
            .citations
            .len(),
        1,
        "Identical canonical quotes are deduplicated"
    );
}

#[test]
fn closing_scores_require_exact_learner_evidence_and_valid_points() {
    let (boundary, session) = fixture();
    for quote in [
        "I coordinated the team.",
        QUOTE,
        "I tested a retry after the acknowledgement disappeared.",
    ] {
        let mut value = response();
        value["scorecard"][0]["learnerQuotes"] = json!([quote]);
        assert!(boundary.read(&value.to_string(), &session).is_err());
    }
    for points in [json!(null), json!(-1), json!(5)] {
        let mut value = response();
        value["scorecard"][0]["points"] = points;
        assert!(boundary.read(&value.to_string(), &session).is_err());
    }
    let mut empty = response();
    empty["scorecard"][0]["learnerQuotes"] = json!([]);
    assert!(boundary.read(&empty.to_string(), &session).is_err());
    let mut zero = response();
    zero["scorecard"][1]["points"] = json!(0);
    assert!(
        boundary.read(&zero.to_string(), &session).is_err(),
        "Unobserved skills cannot receive zero"
    );
    let mut duplicate = response();
    duplicate["scorecard"][1]["criterionId"] = json!("c1");
    assert!(boundary.read(&duplicate.to_string(), &session).is_err());
    let mut missing = response();
    missing["scorecard"].as_array_mut().expect("rows").pop();
    assert!(boundary.read(&missing.to_string(), &session).is_err());
}

#[test]
fn unobserved_feedback_uses_an_app_owned_explanation() -> Result<()> {
    let (boundary, session) = fixture();
    let mut value = response();
    value["scorecard"][1]["feedback"] = json!("0 points: you lack the ability to collaborate.");
    let result = boundary.read(&value.to_string(), &session)?;
    assert!(!result.content.contains("0 points"));
    assert!(!result.content.contains("lack the ability"));
    assert!(result
        .content
        .contains("does not contain enough learner evidence"));
    Ok(())
}
