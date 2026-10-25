//! Regression tests for company identity, round coverage and the generation flow.
use super::super::{generate_activity, INTERVIEW_PLAN_FILE_PATH};
use super::*;
use crate::features::learning::practical_dto::LearningPracticalActivityKind;
use lattice_core::application::ports::llm_port::{CompletionRequest, CompletionResponse};
use serde_json::{json, Value};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Mutex,
};

fn plan_json(round: LearningInterviewRoundKind) -> Value {
    let rounds = if round == LearningInterviewRoundKind::FullLoop {
        vec![
            LearningInterviewRoundKind::TechnicalDeepDive,
            LearningInterviewRoundKind::SystemDesign,
            LearningInterviewRoundKind::Behavioral,
            LearningInterviewRoundKind::DebuggingAndCode,
        ]
    } else {
        vec![round; 3]
    };
    json!({
        "companyName":"Acme", "targetRole":"Staff Engineer", "roundKind":round,
        "companySummary":"Company research is unavailable; constraints are hypothetical.",
        "questions":rounds.into_iter().enumerate().map(|(index, round)| json!({
            "round":round, "question":format!("Practice question {index}?"),
            "companyRationale":"Applies the role's stated constraints.",
            "followUpProbes":["Explain your tradeoffs.", "How would you verify this?"],
            "evaluationSignals":["Explains assumptions.", "Uses concrete evidence."]
        })).collect::<Vec<_>>()
    })
}

#[test]
fn interview_plan_validates_round_coverage_identity_and_real_content() -> Result<()> {
    let target = LearningInterviewTargetDto {
        company_name: Some("Acme".into()),
        target_role: Some("Staff Engineer".into()),
        round_kind: Some(LearningInterviewRoundKind::FullLoop),
        ..Default::default()
    };
    let valid = plan_json(LearningInterviewRoundKind::FullLoop);
    let checked = checked_interview_plan(&valid.to_string(), &target, &[])?;
    assert_eq!(checked.questions.len(), 4);
    assert_eq!(checked.learner_role, "Candidate");
    assert_eq!(checked.counterpart_role, "Interviewer");
    let mut malformed = Vec::new();
    let mut wrong_company = valid.clone();
    wrong_company["companyName"] = json!("Other Company");
    malformed.push(wrong_company);
    let mut missing_round = valid.clone();
    missing_round["questions"][3]["round"] = json!("behavioral");
    malformed.push(missing_round);
    let mut duplicate = valid.clone();
    duplicate["questions"][1]["question"] = duplicate["questions"][0]["question"].clone();
    malformed.push(duplicate);
    let mut no_signals = valid.clone();
    no_signals["questions"][0]["evaluationSignals"] = json!([]);
    malformed.push(no_signals);
    let mut blank_role = valid.clone();
    blank_role["learnerRole"] = json!("  ");
    malformed.push(blank_role);
    let mut too_large = valid.clone();
    too_large["companySummary"] = json!("x".repeat(4_001));
    malformed.push(too_large);
    for invalid_plan in malformed {
        assert!(checked_interview_plan(&invalid_plan.to_string(), &target, &[]).is_err());
    }
    let single = LearningInterviewTargetDto {
        round_kind: Some(LearningInterviewRoundKind::Behavioral),
        ..target
    };
    let mut plan = plan_json(LearningInterviewRoundKind::Behavioral);
    assert!(checked_interview_plan(&plan.to_string(), &single, &[]).is_ok());
    plan["questions"][0]["round"] = json!("system_design");
    assert!(checked_interview_plan(&plan.to_string(), &single, &[]).is_err());
    Ok(())
}

#[test]
fn interview_schema_keeps_local_constrained_output_enabled() {
    fn check(value: &Value) {
        match value {
            Value::Object(fields) => {
                if let Some(maximum) = fields.get("maxLength").and_then(Value::as_u64) {
                    assert!(
                        maximum <= 2_000,
                        "Oversized maxLength disables the local grammar"
                    );
                }
                for child in fields.values() {
                    check(child);
                }
            }
            Value::Array(items) => {
                for child in items {
                    check(child);
                }
            }
            _ => {}
        }
    }
    check(&interview_plan_schema(LearningInterviewRoundKind::FullLoop));
}

#[derive(Default)]
struct InterviewLlm {
    requests: Mutex<Vec<String>>,
    invalid_plan: bool,
    public_files: bool,
    invalid_citation_once: AtomicBool,
}

#[async_trait::async_trait]
impl LLMPort for InterviewLlm {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        let prompt = serde_json::to_string(request).map_err(|e| invalid(e.to_string()))?;
        let properties = request
            .json_schema
            .as_ref()
            .and_then(|schema| schema.get("properties"));
        let plan_request = properties.is_some_and(|fields| fields.get("companySummary").is_some());
        self.requests
            .lock()
            .map_err(|_| invalid("Request lock poisoned"))?
            .push(prompt);
        if plan_request && self.invalid_plan {
            return Ok(CompletionResponse::from_text("{invalid plan}"));
        }
        let response = if plan_request {
            plan_json(LearningInterviewRoundKind::FullLoop)
        } else if properties.is_some_and(|fields| fields.get("content").is_some()) {
            if self.invalid_citation_once.swap(false, Ordering::Relaxed) {
                json!({"content":"Evidence is insufficient.","citations":[{"sourceId":"learner","versionId":"transcript","quote":"I am not sure."}]})
            } else {
                json!({"content":"Explain how you would verify the design.","citations":[]})
            }
        } else {
            json!({
                "title":"Acme staff interview", "brief":"Reason through company constraints.",
                "sourceVersionIds":[], "rubric":[
                    {"title":"Tradeoffs","description":"Explains alternatives.","maxPoints":5},
                    {"title":"Evidence","description":"Verifies assumptions.","maxPoints":5}
                ], "starterFiles":[], "referenceFiles": if self.public_files {
                    json!([{"path":"interview/preparation.md","content":"Private interview coaching must not become a public file."}])
                } else { json!([]) }
            })
        };
        Ok(CompletionResponse::from_text(response.to_string()))
    }
    fn model_name(&self) -> &str {
        "interview-test"
    }
    fn max_context_tokens(&self) -> usize {
        131_072
    }
    fn count_tokens(&self, text: &str) -> usize {
        text.split_whitespace().count()
    }
    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
}

#[tokio::test]
async fn prompt_only_interview_generates_plan_before_its_rubric() -> Result<()> {
    let llm = InterviewLlm {
        requests: Mutex::new(Vec::new()),
        invalid_plan: false,
        public_files: false,
        ..Default::default()
    };
    let program = crate::features::learning::tests::fixture();
    let lesson = program
        .modules
        .iter()
        .flat_map(|module| &module.lessons)
        .next()
        .ok_or_else(|| invalid("Missing lesson fixture"))?;
    let generated = generate_activity(
        &llm,
        LearningPracticalActivityKind::Interview,
        "Prepare for Acme as a Staff Engineer",
        lesson,
        &[],
        None,
        None,
        &[],
    )
    .await?;
    let plan_file = generated
        .files
        .iter()
        .find(|file| file.path == INTERVIEW_PLAN_FILE_PATH)
        .ok_or_else(|| invalid("Interview plan was not generated"))?;
    let plan: LearningInterviewPlanDto =
        serde_json::from_str(&plan_file.content).map_err(|e| invalid(e.to_string()))?;
    assert_eq!(plan.company_name, "Acme");
    assert_eq!(plan.questions.len(), 4);
    let requests = llm
        .requests
        .lock()
        .map_err(|_| invalid("Request lock poisoned"))?;
    assert_eq!(requests.len(), 2);
    assert!(requests.last().is_some_and(
        |request| request.contains("interviewPlan") && request.contains("evaluationSignals")
    ));
    Ok(())
}

#[tokio::test]
async fn failed_interview_planning_never_substitutes_canned_questions() -> Result<()> {
    let llm = InterviewLlm {
        requests: Mutex::new(Vec::new()),
        invalid_plan: true,
        public_files: false,
        ..Default::default()
    };
    let program = crate::features::learning::tests::fixture();
    let lesson = &program.modules[0].lessons[0];
    let result = generate_activity(
        &llm,
        LearningPracticalActivityKind::Interview,
        "Prepare for Acme",
        lesson,
        &[],
        None,
        None,
        &[],
    )
    .await;
    assert!(result.is_err());
    let requests = llm
        .requests
        .lock()
        .map_err(|_| invalid("Request lock poisoned"))?;
    assert_eq!(
        requests.len(),
        2,
        "Retry the invalid plan once, then report failure"
    );
    assert!(requests
        .iter()
        .all(|request| !request.contains("interviewInstructions")));
    Ok(())
}

#[tokio::test]
async fn interview_rejects_public_preparation_even_when_provider_ignores_schema() -> Result<()> {
    let llm = InterviewLlm {
        requests: Mutex::new(Vec::new()),
        invalid_plan: false,
        public_files: true,
        ..Default::default()
    };
    let program = crate::features::learning::tests::fixture();
    let result = generate_activity(
        &llm,
        LearningPracticalActivityKind::Interview,
        "Prepare for Acme",
        &program.modules[0].lessons[0],
        &[],
        None,
        None,
        &[],
    )
    .await;
    assert!(
        result.is_err(),
        "Public interview preparation must never be persisted"
    );
    Ok(())
}

#[tokio::test]
async fn simulation_corrects_a_fabricated_learner_citation_before_returning_feedback() -> Result<()>
{
    use super::super::{generate_simulation_turn, SimulationActivityContext};
    use crate::features::learning::practical_dto::*;
    let llm = InterviewLlm {
        invalid_citation_once: AtomicBool::new(true),
        ..Default::default()
    };
    let session = LearningSimulationSessionDto {
        id: uuid::Uuid::new_v4().to_string(),
        program_id: uuid::Uuid::new_v4().to_string(),
        activity_id: uuid::Uuid::new_v4().to_string(),
        activity_revision: 0,
        practice_session_id: None,
        learner_role: "Candidate".into(),
        counterpart_role: "Interviewer".into(),
        status: LearningSimulationStatus::Active,
        revision: 0,
        turns: vec![LearningSimulationTurnDto {
            id: uuid::Uuid::new_v4().to_string(),
            ordinal: 0,
            speaker: LearningSimulationSpeaker::Learner,
            content: "I am not sure.".into(),
            citations: vec![],
            model_name: None,
            created_at: 0,
        }],
        created_at: 0,
        updated_at: 0,
        submitted_at: None,
    };
    let context = SimulationActivityContext {
        title: "Interview".into(),
        brief: "Practice reasoning".into(),
        rubric: vec![],
        interview_plan: None,
    };
    let feedback = generate_simulation_turn(&llm, &context, &session, &[], true).await?;
    assert!(feedback.citations.is_empty());
    assert_eq!(
        llm.requests
            .lock()
            .map_err(|_| invalid("Request lock poisoned"))?
            .len(),
        2
    );
    Ok(())
}

#[tokio::test]
async fn long_interviews_can_continue_and_still_receive_final_feedback() -> Result<()> {
    use super::super::{generate_simulation_turn, SimulationActivityContext};
    use crate::features::learning::practical_dto::*;
    let llm = InterviewLlm {
        requests: Mutex::new(Vec::new()),
        invalid_plan: false,
        public_files: false,
        ..Default::default()
    };
    let mut session = LearningSimulationSessionDto {
        id: uuid::Uuid::new_v4().to_string(),
        program_id: uuid::Uuid::new_v4().to_string(),
        activity_id: uuid::Uuid::new_v4().to_string(),
        activity_revision: 0,
        practice_session_id: None,
        learner_role: "Candidate".into(),
        counterpart_role: "Interviewer".into(),
        status: LearningSimulationStatus::Active,
        revision: 60,
        turns: Vec::new(),
        created_at: 0,
        updated_at: 0,
        submitted_at: None,
    };
    let context = SimulationActivityContext {
        title: "Interview".into(),
        brief: "Practice reasoning".into(),
        rubric: vec![],
        interview_plan: None,
    };
    for ordinal in 0..120 {
        session.turns.push(LearningSimulationTurnDto {
            id: uuid::Uuid::new_v4().to_string(),
            ordinal,
            speaker: if ordinal % 2 == 0 {
                LearningSimulationSpeaker::Learner
            } else {
                LearningSimulationSpeaker::Counterpart
            },
            content: "A short interview turn.".into(),
            citations: vec![],
            model_name: None,
            created_at: 0,
        });
    }
    assert!(
        generate_simulation_turn(&llm, &context, &session, &[], false)
            .await
            .is_ok()
    );
    session.turns = session.turns.iter().cycle().take(162).cloned().collect();
    assert!(
        generate_simulation_turn(&llm, &context, &session, &[], false)
            .await
            .is_err()
    );
    assert!(
        generate_simulation_turn(&llm, &context, &session, &[], true)
            .await
            .is_ok()
    );
    Ok(())
}
