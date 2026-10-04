//! Exercise the real authoring, validation and persistence boundaries without a live model.
use super::{
    dto::*, generation, practice_repository::LearningPracticeRepository,
    repository::LearningRepository, service,
};
use crate::{
    application::ports::LLMPort,
    shared::error::{AppError, Result},
};
use serde_json::json;

struct Model(String);
#[async_trait::async_trait]
impl LLMPort for Model {
    async fn generate(&self, prompt: &str, _: &[String], _: Option<Vec<String>>) -> Result<String> {
        if let Some(reply) = super::content_verification::tests::fixture_response(prompt) {
            return Ok(reply);
        }
        if prompt.starts_with("Source passages:") {
            return Ok("supported\nReason: The fixture reference supports the experimental design.\nSource quote: A randomized comparison isolates the intervention.".into());
        }
        if prompt.starts_with("Verify instructional answer keys independently.") {
            return Ok(json!({"answers":(0..6).map(|index| json!({"index":index,"correctIndices":[0],"reason":"A randomized comparison isolates the intervention."})).collect::<Vec<_>>()} ).to_string());
        }
        if prompt.starts_with("Review instructional quality.") {
            let context: serde_json::Value =
                serde_json::from_str(prompt.rsplit("\n\n").next().unwrap_or("{}"))?;
            let checks: Vec<_> = context["candidate"]["blocks"].as_array().into_iter().flatten().enumerate().map(|(index, block)|json!({"index":index,"quote":block["body"].as_str().unwrap_or_default().chars().take(80).collect::<String>(),"finding":"The comparison and measurement task is consistent with the randomized experiment objective.","hasDefect":false})).collect();
            return Ok(json!({"issues":[],"blockChecks":checks}).to_string());
        }
        Ok(self.0.clone())
    }
    async fn generate_streaming(
        &self,
        _: &str,
        _: &[String],
        _: Option<Vec<String>>,
    ) -> Result<Box<dyn futures::Stream<Item = Result<String>> + Send + Unpin + '_>> {
        Ok(Box::new(futures::stream::iter(vec![Ok(self.0.clone())])))
    }
    fn model_name(&self) -> &str {
        "course-fixture"
    }
    fn max_context_tokens(&self) -> usize {
        128_000
    }
    fn count_tokens(&self, text: &str) -> usize {
        text.len() / 4
    }
    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
}
fn request(depth: LearningCourseDepth) -> GenerateLearningProgramRequestDto {
    GenerateLearningProgramRequestDto {
        goal: "Design experiments and interpret their results".into(),
        prior_knowledge: "Basic algebra".into(),
        minutes_per_session: 30,
        document_ids: vec![],
        source_urls: vec![],
        course_depth: Some(depth),
    }
}
fn outline(modules: usize, lessons: usize) -> Model {
    Model(json!({"modules":(0..modules).map(|m| json!({
        "title":format!("Experimental design module {m}"),
        "prerequisiteIndices":if m == 0 {vec![]} else {vec![m-1]},
        "project":{"title":format!("Experiment project milestone {m}"),"brief":"Develop the next stage of the randomized experiment and revise the previous stage using feedback.","deliverables":["A written protocol identifying treatment and control groups.","A measurement plan and a limitations section for this experiment."],"successCriteria":["The protocol isolates the intervention and explains random assignment.","The analysis distinguishes observed results from causal interpretation."]},
        "summary":{"text":"Build a skill and apply it to an experiment.","sourceIndex":null,"quote":""},
        "outcomes":[{"text":"Design a comparison with a clear outcome.","sourceIndex":null,"quote":""},{"text":"Interpret results and explain uncertainty.","sourceIndex":null,"quote":""}],
        "lessons":(0..lessons).map(|l| json!({"title":format!("Experimental design {m}.{l}"),"objective":"Design a controlled experiment and explain its limitations.","estimatedMinutes":30,"sourceIndex":null,"quote":""})).collect::<Vec<_>>()
    })).collect::<Vec<_>>()} ).to_string())
}
fn task_rubric() -> serde_json::Value {
    json!([
        {"title":"Randomized comparison","description":"Define treatment and comparison groups, specify random allocation, and explain how it limits confounding.","dimension":"application"},
        {"title":"Measurement and limitations","description":"State an observable outcome and explain one limitation of interpreting the difference between groups.","dimension":"explanation"}
    ])
}
fn lesson_response() -> serde_json::Value {
    let teaching = "Start with a measurable question, identify the comparison, and define the outcome before collecting data. Explain the assumptions behind the design. Work through the decisions in order and examine how a confounding variable could change the conclusion. A concrete example should show both the result and the limitations of that result. ";
    json!({
        "blocks":(["explanation","explanation","worked_example","worked_example","guided_practice","independent_practice","reflection","recap"].iter().enumerate().map(|(i,kind)| json!({"kind":kind,"title":format!("Design step {i}"),"body":format!("{teaching}{teaching} Assignment step {i}: produce a design, state your assumptions, and justify your decisions."),"rubric":if ["guided_practice","independent_practice"].contains(kind) { task_rubric() } else { json!([]) },"sourceIndex":null,"quote":""})).collect::<Vec<_>>()),
        "questions":(["practice","quiz","test"].iter().flat_map(|kind| (0..2).map(move |i|json!({"kind":kind,"prompt":format!("Which experimental design decision is appropriate in {kind} scenario {i}?"),"options":["Compare randomized groups","Change several variables together","Select only positive results","Omit the comparison group"],"correctIndex":0,"explanation":"A randomized comparison helps separate the intervention from confounding factors.","sourceIndex":null,"quote":""}))).collect::<Vec<_>>())
    })
}

#[tokio::test]
async fn topic_course_with_acquired_reference_runs_through_assignment_and_recall() -> Result<()> {
    let pool = super::tests::pool().await?;
    let repo = LearningRepository::new(pool.clone());
    let draft = service::generate(
        &repo,
        &outline(4, 3),
        request(LearningCourseDepth::Course),
        vec![],
    )
    .await?;
    assert_eq!(draft.summary.lesson_count, 12);
    assert!(draft.modules.iter().all(|m| m.project.is_some()));
    assert_eq!(
        draft.modules[1].prerequisite_module_ids,
        vec![draft.modules[0].id.clone()]
    );
    assert!(draft.sources.is_empty());
    let accepted = service::accept(
        &repo,
        AcceptLearningProgramRequestDto {
            program_id: draft.summary.id.clone(),
            expected_revision: 0,
            title: "Experimental design".into(),
        },
    )
    .await?;
    let lesson_id = accepted
        .summary
        .current_lesson_id
        .clone()
        .ok_or_else(|| AppError::InvalidState("missing lesson".into()))?;
    // Topic outlines need evidence before they can become ready lessons.
    super::content_verification::tests::install_source(
        &pool,
        &draft.summary.id,
        "A randomized comparison isolates the intervention.",
    )
    .await?;
    let mut lesson_candidate = lesson_response();
    for field in ["blocks", "questions"] {
        for item in lesson_candidate[field].as_array_mut().unwrap() {
            item["sourceIndex"] = json!(0);
            item["quote"] = json!("A randomized comparison isolates the intervention.");
        }
    }
    let prepared = service::prepare(
        &repo,
        &Model(lesson_candidate.to_string()),
        PrepareLearningLessonRequestDto {
            program_id: draft.summary.id.clone(),
            lesson_id: lesson_id.clone(),
            expected_revision: accepted.summary.revision,
        },
    )
    .await?;
    let lesson = prepared
        .modules
        .iter()
        .flat_map(|module| &module.lessons)
        .find(|lesson| lesson.id == lesson_id)
        .ok_or_else(|| AppError::InvalidState("missing lesson".into()))?;
    assert_eq!(lesson.blocks.len(), 8);
    assert_eq!(lesson.questions.len(), 6);
    assert!(lesson
        .blocks
        .iter()
        .all(|block| !block.source_ids.is_empty()));
    assert!(!serde_json::to_string(&prepared)
        .map_err(|error| AppError::Serialization(error.to_string()))?
        .contains("correctIndex"));
    let assignment = lesson
        .blocks
        .iter()
        .find(|block| block.kind == LearningBlockKind::IndependentPractice)
        .ok_or_else(|| AppError::InvalidState("missing assignment".into()))?;
    let session_id = uuid::Uuid::new_v4().to_string();
    let practice = LearningPracticeRepository::new(pool.clone());
    practice
        .start(
            &StartLearningPracticeSessionRequestDto {
                operation_id: uuid::Uuid::new_v4().to_string(),
                session_id: session_id.clone(),
                program_id: prepared.summary.id.clone(),
                lesson_id: lesson_id.clone(),
                expected_program_revision: prepared.summary.revision,
                mode: LearningPracticeMode::Practice,
                task_kind: LearningPracticeTaskKind::Independent,
                revises_session_id: None,
            },
            "topic-start",
        )
        .await?;
    let frozen = practice.get_session(&session_id).await?;
    assert_eq!(frozen.task_prompt, assignment.body);
    assert_eq!(frozen.rubric.len(), 2);
    assert_eq!(frozen.rubric[0].title, "Randomized comparison");
    assert!(!frozen.summary.source_version_ids.is_empty());
    let mut uncertain_session = frozen.clone();
    uncertain_session.artifact.text = "I am not sure how to start.".into();
    let uncertain = Model(json!({"status":"uncertain","criteria":frozen.rubric.iter().map(|criterion| json!({"criterion_id":criterion.id,"score":0,"observation":"No assessable reasoning is present. Try identifying the comparison groups first.","evidence_quote":"I am not sure how to start."})).collect::<Vec<_>>()} ).to_string());
    let (status, criteria) =
        super::practice_generation::grade(&uncertain, &uncertain_session).await?;
    assert_eq!(status, LearningPracticeGradeStatus::Uncertain);
    assert!(criteria.iter().all(|criterion| criterion.score.is_none()));

    let mut attempted = frozen.clone();
    attempted.artifact.text =
        "I assigned groups randomly.\nI measured the outcome before comparing the groups.".into();
    let feedback = |quote: &str| {
        json!({"status":"provisional","criteria":frozen.rubric.iter().map(|criterion|json!({"criterion_id":criterion.id,"score":2,"observation":"The design includes random assignment; explain the remaining limitations.","evidence_quote":quote})).collect::<Vec<_>>()} ).to_string()
    };
    let shortened = feedback("I assigned groups randomly ... I measured the outcome");
    let exact = feedback("I assigned groups randomly.");
    let repair_model = ScriptedModel {
        outputs: std::sync::Mutex::new(vec![shortened.clone(), exact].into()),
        prompts: std::sync::Mutex::new(vec![]),
    };
    let (_, repaired) = super::practice_generation::grade(&repair_model, &attempted).await?;
    assert!(repaired.iter().all(
        |criterion| criterion.evidence_quote.as_deref() == Some("I assigned groups randomly.")
    ));
    assert_eq!(
        repair_model
            .prompts
            .lock()
            .map_err(|_| AppError::InternalError("fixture lock".into()))?
            .len(),
        2
    );
    let still_invalid = ScriptedModel {
        outputs: std::sync::Mutex::new(vec![shortened.clone(), shortened].into()),
        prompts: std::sync::Mutex::new(vec![]),
    };
    assert!(
        super::practice_generation::grade(&still_invalid, &attempted)
            .await
            .is_err()
    );

    let recall = Model(json!({"cards":[
        {"question":"When should the outcome be defined?","answer":"Before collecting data.","explanation":"Define it when designing the experiment.","sourceIds":[prepared.sources[0].id],"quote":"A randomized comparison isolates the intervention."},
        {"question":"What should a concrete example show?","answer":"The result and its limitations.","explanation":"Interpret the result within its limits.","sourceIds":[prepared.sources[0].id],"quote":"A randomized comparison isolates the intervention."}
    ]}).to_string());
    let cards = generation::generate_recall_drafts(&recall, &prepared, &lesson_id, 2).await?;
    assert_eq!(cards.len(), 2);
    assert!(cards.iter().all(|card| !card.source_ids.is_empty()));
    repo.save_generated_drafts(&prepared.summary.id, &lesson_id, &cards)
        .await?;
    let draft_id: String =
        sqlx::query_scalar("SELECT id FROM learning_card_drafts WHERE program_id=? LIMIT 1")
            .bind(&prepared.summary.id)
            .fetch_one(&pool)
            .await
            .map_err(|error| AppError::Database(error.to_string()))?;
    assert!(!repo
        .accept_card_draft(&prepared.summary.id, &draft_id)
        .await?
        .is_empty());
    let module_id = prepared
        .modules
        .first()
        .ok_or_else(|| AppError::InvalidState("missing module".into()))?
        .id
        .clone();
    let graded = service::submit(
        &repo,
        SubmitLearningAttemptRequestDto {
            attempt_id: uuid::Uuid::new_v4().to_string(),
            program_id: prepared.summary.id.clone(),
            expected_revision: prepared.summary.revision,
            module_id,
            lesson_id: Some(lesson_id.clone()),
            kind: LearningAssessmentKind::Quiz,
            answers: lesson
                .questions
                .iter()
                .filter(|question| question.kind == LearningAssessmentKind::Quiz)
                .map(|question| LearningAnswerDto {
                    question_id: question.id.clone(),
                    selected_index: 0,
                })
                .collect(),
        },
    )
    .await?;
    assert_eq!(
        graded.attempts.first().map(|attempt| attempt.correct),
        Some(2)
    );
    Ok(())
}

struct ScriptedModel {
    outputs: std::sync::Mutex<std::collections::VecDeque<String>>,
    prompts: std::sync::Mutex<Vec<String>>,
}
#[async_trait::async_trait]
impl LLMPort for ScriptedModel {
    async fn generate(&self, prompt: &str, _: &[String], _: Option<Vec<String>>) -> Result<String> {
        self.prompts
            .lock()
            .map_err(|_| AppError::InternalError("fixture lock".into()))?
            .push(prompt.into());
        self.outputs
            .lock()
            .map_err(|_| AppError::InternalError("fixture lock".into()))?
            .pop_front()
            .ok_or_else(|| AppError::InvalidInput("Unexpected extra model call".into()))
    }
    async fn generate_streaming(
        &self,
        _: &str,
        _: &[String],
        _: Option<Vec<String>>,
    ) -> Result<Box<dyn futures::Stream<Item = Result<String>> + Send + Unpin + '_>> {
        Err(AppError::InvalidInput("fixture".into()))
    }
    fn model_name(&self) -> &str {
        "scripted-review"
    }
    fn count_tokens(&self, text: &str) -> usize {
        text.len() / 4
    }
    fn max_context_tokens(&self) -> usize {
        128_000
    }
    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
}

#[tokio::test]
async fn instructional_review_repairs_once_and_rejects_unresolved_defects() -> Result<()> {
    let issue = json!({"issues":["The exercise answer contradicts its stated condition. Correct the comparison."]}).to_string();
    let repaired = json!({"corrected":true}).to_string();
    let model = ScriptedModel {
        outputs: std::sync::Mutex::new(
            vec![
                issue.clone(),
                repaired.clone(),
                json!({"issues":[]}).to_string(),
            ]
            .into(),
        ),
        prompts: std::sync::Mutex::new(vec![]),
    };
    let actual = super::teaching::review_and_repair(
        &model,
        "Author a lesson",
        "Require a correct comparison",
        &json!({"type":"object"}),
        json!({"wrong":true}).to_string(),
        2000,
    )
    .await?;
    assert_eq!(actual, repaired);
    {
        let prompts = model
            .prompts
            .lock()
            .map_err(|_| AppError::InternalError("fixture lock".into()))?;
        assert_eq!(prompts.len(), 3);
        assert!(prompts[1].contains("Correct the comparison"));
        assert!(prompts[2].contains("originalIssuesToRecheck"));
        assert!(prompts[2].contains("final publication check"));
        assert!(prompts[2].contains("Correct the comparison"));
    }
    let model = ScriptedModel {
        outputs: std::sync::Mutex::new(vec![issue.clone(), repaired, issue].into()),
        prompts: std::sync::Mutex::new(vec![]),
    };
    assert!(super::teaching::review_and_repair(
        &model,
        "Author",
        "Context",
        &json!({}),
        json!({"wrong":true}).to_string(),
        2000
    )
    .await
    .is_err());
    Ok(())
}

#[tokio::test]
async fn lesson_review_requires_evidence_for_every_section_and_repairs_reported_defects(
) -> Result<()> {
    let body =
        "A running total is initialized to zero and each accepted value is added exactly once. "
            .repeat(10);
    let candidate = json!({"blocks":[{"kind":"explanation","body":body}]});
    let check = |has_defect| {
        json!({"issues":[],"blockChecks":[{"index":0,"quote":"A running total is initialized to zero","finding":"The example contradicts its return value; correct the returned total to match the accumulation.","hasDefect":has_defect}]}).to_string()
    };
    let model = ScriptedModel {
        outputs: std::sync::Mutex::new(
            vec![check(true), candidate.to_string(), check(false)].into(),
        ),
        prompts: std::sync::Mutex::new(vec![]),
    };
    super::teaching::review_and_repair(
        &model,
        "Author",
        "Context",
        &json!({}),
        candidate.to_string(),
        2000,
    )
    .await?;
    {
        let prompts = model
            .prompts
            .lock()
            .map_err(|_| AppError::InternalError("fixture lock".into()))?;
        assert!(prompts[1].contains("Section 1: The example contradicts"));
    }
    for review in [
        json!({"issues":[],"blockChecks":[]}),
        json!({"issues":[],"blockChecks":[{"index":0,"quote":"An invented quote that never occurred.","finding":"This unsupported review cannot establish that the passage was inspected.","hasDefect":false}]}),
    ] {
        let model = ScriptedModel {
            outputs: std::sync::Mutex::new(vec![review.to_string()].into()),
            prompts: std::sync::Mutex::new(vec![]),
        };
        assert!(super::teaching::review_and_repair(
            &model,
            "Author",
            "Context",
            &json!({}),
            candidate.to_string(),
            2000
        )
        .await
        .is_err());
    }
    Ok(())
}

#[tokio::test]
async fn disputed_answer_keys_are_blinded_repaired_and_checked_again() -> Result<()> {
    let candidate = json!({"questions":[{"prompt":"What is two plus two?","options":["Three","Four","Five","Six"],"correctIndex":0,"explanation":"AUTHOR_KEY_SECRET","quote":""}]});
    let mut repaired = candidate.clone();
    repaired["questions"][0]["correctIndex"] = json!(1);
    repaired["questions"][0]["explanation"] = json!("Two plus two is four.");
    let check = json!({"answers":[{"index":0,"correctIndices":[1],"reason":"Adding two pairs produces four items."}]}).to_string();
    let model = ScriptedModel {
        outputs: std::sync::Mutex::new(
            vec![
                check.clone(),
                json!({"issues":[]}).to_string(),
                repaired.to_string(),
                check,
                json!({"issues":[]}).to_string(),
            ]
            .into(),
        ),
        prompts: std::sync::Mutex::new(vec![]),
    };
    let actual = super::teaching::review_and_repair(
        &model,
        "Author",
        "Context",
        &json!({"type":"object"}),
        candidate.to_string(),
        2000,
    )
    .await?;
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&actual)?,
        repaired
    );
    {
        let prompts = model
            .prompts
            .lock()
            .map_err(|_| AppError::InternalError("fixture lock".into()))?;
        assert_eq!(prompts.len(), 5);
        assert!(!prompts[0].contains("AUTHOR_KEY_SECRET"));
        assert!(!prompts[0].contains("\"correctIndex\":"));
        assert!(prompts[2].contains("disputed or ambiguous answer key"));
        assert!(prompts[3].starts_with("Verify instructional answer keys independently."));
    }
    let incomplete = ScriptedModel {
        outputs: std::sync::Mutex::new(vec![json!({"answers":[]}).to_string()].into()),
        prompts: std::sync::Mutex::new(vec![]),
    };
    assert!(super::teaching::review_and_repair(
        &incomplete,
        "Author",
        "Context",
        &json!({}),
        candidate.to_string(),
        2000
    )
    .await
    .is_err());
    Ok(())
}

#[tokio::test]
async fn placement_tasks_keep_keys_private_and_save_answers_with_replay_and_conflict_checks(
) -> Result<()> {
    use super::{
        curriculum_repository::LearningCurriculumRepository, diagnostic_generation::DiagnosticTask,
        plan_dto::*,
    };
    let pool = super::tests::pool().await?;
    let repository = LearningRepository::new(pool.clone());
    let draft = service::generate(
        &repository,
        &outline(2, 2),
        request(LearningCourseDepth::Focused),
        vec![],
    )
    .await?;
    let program = service::accept(
        &repository,
        AcceptLearningProgramRequestDto {
            program_id: draft.summary.id.clone(),
            expected_revision: 0,
            title: "Experiment design".into(),
        },
    )
    .await?;
    let repo = LearningCurriculumRepository::new(pool.clone());
    let plan = repo.plan(&program.summary.id).await?;
    let outcome = plan
        .accepted_revision
        .ok_or_else(|| AppError::InvalidState("missing plan".into()))?
        .modules[0]
        .outcome_ids[0]
        .clone();
    let task=DiagnosticTask {outcome_id:outcome.clone(),prompt:"Two groups receive different treatments but volunteers select their own group. Explain the problem and improve this design.".into(),expected_answer:"PRIVATE_PLACEMENT_KEY: use randomized assignment to reduce selection confounding.".into(),criteria:"Identify selection confounding and explain why randomized assignment helps.".into()};
    let start = StartLearningDiagnosticRequestDto {
        operation_id: uuid::Uuid::new_v4().to_string(),
        program_id: program.summary.id.clone(),
        expected_revision: program.summary.revision,
    };
    let attempt = repo
        .start_diagnostic_authored(&start, Some(&[task]))
        .await?;
    assert!(!serde_json::to_string(&attempt)
        .map_err(|e| AppError::Serialization(e.to_string()))?
        .contains("PRIVATE_PLACEMENT_KEY"));
    assert_eq!(
        repo.start_diagnostic_authored(&start, None).await?.id,
        attempt.id
    );
    let response = LearningDiagnosticResponseDto {
        prompt_id: attempt.prompts[0].id.clone(),
        response: "Use random allocation so treatment choice does not determine group membership."
            .into(),
    };
    let save = SubmitLearningDiagnosticRequestDto {
        operation_id: uuid::Uuid::new_v4().to_string(),
        program_id: program.summary.id.clone(),
        diagnostic_id: attempt.id.clone(),
        expected_revision: program.summary.revision,
        responses: vec![response.clone()],
        save_only: true,
        expected_diagnostic_revision: Some(0),
    };
    let saved = repo.submit_diagnostic(&save).await?;
    assert_eq!(saved.status, LearningDiagnosticStatus::Active);
    assert_eq!(saved.revision, 1);
    assert_eq!(repo.submit_diagnostic(&save).await?.revision, 1);
    let mut stale = save.clone();
    stale.operation_id = uuid::Uuid::new_v4().to_string();
    assert!(repo.submit_diagnostic(&stale).await.is_err());
    let finding=LearningDiagnosticFindingDto {prompt_id:response.prompt_id.clone(),outcome_id:outcome,signal:LearningDiagnosticSignal::ReadyForChallenge,feedback:"You identified selection bias and explained a useful fix. Try applying it in a more complex scenario.".into(),evidence_quote:Some("Use random allocation".into())};
    let mut invented = finding.clone();
    invented.evidence_quote = Some("words never submitted".into());
    assert!(
        super::diagnostic_generation::validate_findings(&saved, &[response], &[invented]).is_err()
    );
    let finish = SubmitLearningDiagnosticRequestDto {
        operation_id: uuid::Uuid::new_v4().to_string(),
        save_only: false,
        expected_diagnostic_revision: Some(1),
        ..save
    };
    let submitted = repo
        .submit_diagnostic_evaluated(&finish, Some(&[finding]))
        .await?;
    assert_eq!(submitted.status, LearningDiagnosticStatus::Submitted);
    assert_eq!(submitted.findings.len(), 1);
    assert_eq!(submitted.revision, 2);
    assert!(submitted
        .interpretation
        .contains("does not establish mastery"));
    assert_eq!(
        repository
            .get(&program.summary.id)
            .await?
            .summary
            .completed_lessons,
        0
    );
    Ok(())
}

#[tokio::test]
async fn depth_is_enforced_and_failed_source_acquisition_cannot_become_a_topic_course() -> Result<()>
{
    assert!(generation::generate_outline(
        &outline(2, 2),
        &request(LearningCourseDepth::Course),
        &[]
    )
    .await
    .is_err());
    assert_eq!(
        generation::generate_outline(&outline(6, 4), &request(LearningCourseDepth::DeepDive), &[])
            .await?
            .len(),
        6
    );
    let repo = LearningRepository::new(super::tests::pool().await?);
    let mut grounded = request(LearningCourseDepth::Course);
    grounded
        .source_urls
        .push("https://example.com/reference".into());
    assert!(service::generate(&repo, &outline(4, 3), grounded, vec![])
        .await
        .is_err());
    assert!(repo.list().await?.is_empty());
    Ok(())
}

#[tokio::test]
async fn thin_lessons_and_fabricated_topic_citations_are_rejected_before_persistence() -> Result<()>
{
    let mut program = super::tests::fixture();
    program.sources.clear();
    let lesson = program
        .modules
        .first()
        .and_then(|module| module.lessons.first())
        .ok_or_else(|| AppError::InvalidState("missing fixture lesson".into()))?;
    let mut thin = lesson_response();
    thin["blocks"] = json!([]);
    assert!(
        generation::prepare_lesson(&Model(thin.to_string()), &program, lesson)
            .await
            .is_err()
    );
    let mut shallow = lesson_response();
    shallow["blocks"][4]["body"] = json!("Try this.");
    assert!(
        generation::prepare_lesson(&Model(shallow.to_string()), &program, lesson)
            .await
            .is_err()
    );
    let mut ambiguous = lesson_response();
    let mut duplicate_assignment = ambiguous["blocks"][5].clone();
    duplicate_assignment["title"] = json!("A second independent assignment");
    if let Some(blocks) = ambiguous["blocks"].as_array_mut() {
        blocks.push(duplicate_assignment);
    }
    assert!(
        generation::prepare_lesson(&Model(ambiguous.to_string()), &program, lesson)
            .await
            .is_err()
    );
    let mut invented = lesson_response();
    invented["blocks"][0]["sourceIndex"] = json!(0);
    invented["blocks"][0]["quote"] = json!("An imaginary textbook says this is true.");
    assert!(
        generation::prepare_lesson(&Model(invented.to_string()), &program, lesson)
            .await
            .is_err()
    );
    Ok(())
}

#[tokio::test]
async fn topic_checkpoint_can_be_generated_and_persisted_without_sources() -> Result<()> {
    use super::{
        assessment_engine::LearningBlueprintRequirement,
        assessment_repository::LearningAssessmentRepository,
    };
    let pool = super::tests::pool().await?;
    let repo = LearningRepository::new(pool.clone());
    let draft = service::generate(
        &repo,
        &outline(4, 3),
        request(LearningCourseDepth::Course),
        vec![],
    )
    .await?;
    service::accept(
        &repo,
        AcceptLearningProgramRequestDto {
            program_id: draft.summary.id.clone(),
            expected_revision: 0,
            title: "Experiments".into(),
        },
    )
    .await?;
    let assessments = LearningAssessmentRepository::new(pool);
    let workspace = assessments.workspace(&draft.summary.id).await?;
    let outcome = workspace
        .outcomes
        .first()
        .ok_or_else(|| AppError::InvalidState("missing outcome".into()))?;
    let request = CreateLearningAssessmentBlueprintRequestDto {
        operation_id: uuid::Uuid::new_v4().to_string(),
        program_id: draft.summary.id.clone(),
        blueprint_id: uuid::Uuid::new_v4().to_string(),
        revision: 1,
        predecessor_revision: None,
        purpose: LearningAssessmentPurpose::ModuleTest,
        title: "Experiment checkpoint".into(),
        instructions: "Explain your experimental design and justify the comparison.".into(),
        expected_minutes: 30,
        allowed_aids: vec![],
        passing_score: 0.7,
        feedback_timing: LearningFeedbackTiming::AfterSubmission,
        rubric: vec![LearningRubricCriterion {
            id: uuid::Uuid::new_v4().to_string(),
            title: "Reasoning".into(),
            description: "Justify the design decisions and state limitations.".into(),
            max_points: 4,
        }],
        source_version_ids: vec![],
        requirements: vec![LearningBlueprintRequirement {
            outcome_id: outcome.id.clone(),
            format: LearningItemFormat::Explanation,
            count: 1,
            difficulty_min: 2,
            difficulty_max: 3,
        }],
        change_reason: "Initial checkpoint".into(),
    };
    assert!(assessments
        .preflight_blueprint(&request, "checkpoint")
        .await?
        .is_none());
    let model = Model(json!({"items":[{"requirementIndex":0,"rubric":task_rubric(),"prompt":"Explain how you would design a randomized comparison and identify its limitations.","explanation":"The response should define the comparison, measurement, assumptions, and limitations.","options":[],"answerIndex":0,"acceptedAnswers":[],"orderedValues":[],"sourceIndex":null,"quote":""}]}).to_string());
    let candidates =
        super::assessment_generation::generate(&model, &request, &workspace.outcomes, &[], &[])
            .await?;
    assert!(candidates
        .iter()
        .all(|candidate| candidate.source_version_ids.is_empty()));
    let stored = assessments
        .create_blueprint(&request, &candidates, "fixture", "checkpoint")
        .await?;
    assert_eq!(stored.blueprints.len(), 1);
    assert!(assessments
        .preflight_blueprint(&request, "checkpoint")
        .await?
        .is_some());
    let form = assessments
        .start_form(
            &StartLearningAssessmentFormRequestDto {
                operation_id: uuid::Uuid::new_v4().to_string(),
                form_id: uuid::Uuid::new_v4().to_string(),
                program_id: draft.summary.id.clone(),
                blueprint_id: request.blueprint_id.clone(),
                blueprint_revision: 1,
                retake_of_form_id: None,
            },
            "start-checkpoint",
        )
        .await?;
    assert_eq!(form.items.len(), 1);
    Ok(())
}

#[tokio::test]
async fn outline_model_streams_activity_with_a_matching_deadline_and_rejects_cutoff() -> Result<()>
{
    use crate::application::ports::llm_port::{CompletionRequest, CompletionResponse};
    struct StreamingModel;
    #[async_trait::async_trait]
    impl LLMPort for StreamingModel {
        fn supports_typed_completions(&self) -> bool {
            true
        }
        async fn complete(&self, _: &CompletionRequest) -> Result<CompletionResponse> {
            panic!("Outline progress must use the streaming port");
        }
        async fn complete_with_progress(
            &self,
            request: &CompletionRequest,
            on_text: &(dyn Fn(String) -> Result<()> + Send + Sync),
        ) -> Result<CompletionResponse> {
            assert_eq!(
                request.effective_time_budget(),
                super::outline_progress::CALL_BUDGET
            );
            assert!(request.json_schema.is_some());
            on_text("{\"draft\":".into())?;
            on_text("true}".into())?;
            Ok(CompletionResponse {
                text: "{\"draft\":true}".into(),
                finish_reason: "length".into(),
                ..Default::default()
            })
        }
        async fn generate(&self, _: &str, _: &[String], _: Option<Vec<String>>) -> Result<String> {
            unreachable!()
        }
        async fn generate_streaming(
            &self,
            _: &str,
            _: &[String],
            _: Option<Vec<String>>,
        ) -> Result<Box<dyn futures::Stream<Item = Result<String>> + Send + Unpin + '_>> {
            unreachable!()
        }
        fn model_name(&self) -> &str {
            "stream-fixture"
        }
        fn max_context_tokens(&self) -> usize {
            128_000
        }
        fn count_tokens(&self, text: &str) -> usize {
            text.len() / 4
        }
        async fn is_ready(&self) -> Result<bool> {
            Ok(true)
        }
    }
    let progress = super::outline_progress::OutlineProgress::default();
    progress.stage(super::outline_progress::OutlineStage::Drafting);
    let error = generation::complete_json_with_progress(
        &StreamingModel,
        "Generate",
        "An outline".into(),
        json!({"type":"object"}),
        4000,
        Some(&progress),
    )
    .await
    .unwrap_err();
    assert!(error.to_string().contains("cut off"));
    assert_eq!(progress.snapshot().response_characters, 14);
    Ok(())
}
