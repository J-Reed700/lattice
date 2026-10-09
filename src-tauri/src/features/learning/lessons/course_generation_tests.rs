//! Exercise the real authoring, validation and persistence boundaries without a live model.
use crate::features::learning::content_verification::tests::{fixture_completion, fixture_prompts};
use crate::features::learning::{
    dto::*, generation, practice_repository::LearningPracticeRepository,
    repository::LearningRepository, service,
};
use crate::shared::runtime::jobs::{JobContext, JobStore};
use crate::{
    application::ports::llm_port::{CompletionRequest, CompletionResponse},
    application::ports::LLMPort,
    shared::error::{AppError, Result},
};
use serde_json::json;

struct Model(String);
#[async_trait::async_trait]
impl LLMPort for Model {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        let mut replies = Vec::new();
        for prompt in fixture_prompts(request) {
            replies.push(self.respond(&prompt).await?);
        }
        Ok(fixture_completion(request, replies))
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
impl Model {
    async fn respond(&self, prompt: &str) -> Result<String> {
        if let Some(reply) =
            crate::features::learning::content_verification::tests::fixture_response(prompt)
        {
            return Ok(reply);
        }
        if prompt.starts_with("Source passages:") {
            return Ok("supported\nReason: The fixture reference supports the experimental design.\nSource quote: A randomized comparison isolates the intervention.\nSource passage: passage-0".into());
        }
        if prompt.starts_with("Verify instructional answer keys independently.") {
            return Ok(json!({"answers":(0..6).map(|index| json!({"index":index,"correctIndices":[0],"reason":"A randomized comparison isolates the intervention."})).collect::<Vec<_>>()} ).to_string());
        }
        if prompt.starts_with("Review instructional quality.") {
            let context: serde_json::Value =
                serde_json::from_str(prompt.rsplit("\n\n").next().unwrap_or("{}"))?;
            if context["candidate"].get("blocks").is_none() {
                return Ok(json!({"issues":[]}).to_string());
            }
            let checks: Vec<_> = context["candidate"]["blocks"].as_array().into_iter().flatten().enumerate().map(|(index, block)|json!({"index":index,"passageId":block["bodyPassages"][0]["id"],"finding":"The comparison and measurement task is consistent with the randomized experiment objective.","hasDefect":false})).collect();
            return Ok(json!({"issues":[],"blockChecks":crate::features::learning::teaching_review::fixture_checks(checks)}).to_string());
        }
        Ok(self.0.clone())
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
    let pool = crate::features::learning::tests::pool().await?;
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
            expected_revision: draft.summary.revision,
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
    crate::features::learning::content_verification::tests::install_source(
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
        crate::features::learning::practice_generation::grade(&uncertain, &uncertain_session)
            .await?;
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
    let (_, repaired) =
        crate::features::learning::practice_generation::grade(&repair_model, &attempted).await?;
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
        crate::features::learning::practice_generation::grade(&still_invalid, &attempted)
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

pub(in crate::features::learning) struct ScriptedModel {
    pub(in crate::features::learning) outputs: std::sync::Mutex<std::collections::VecDeque<String>>,
    pub(in crate::features::learning) prompts: std::sync::Mutex<Vec<String>>,
}

#[tokio::test]
async fn failed_lesson_review_retries_the_saved_draft_then_verifies_and_publishes() -> Result<()> {
    use crate::features::learning::{
        curriculum::LearningGenerationJobKind, curriculum_repository::LearningCurriculumRepository,
        plan_dto::*,
    };
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    struct InterruptedReviewer {
        inner: Model,
        fail_review: AtomicBool,
        authored: AtomicUsize,
    }
    #[async_trait::async_trait]
    impl LLMPort for InterruptedReviewer {
        async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
            let mut replies = Vec::new();
            for prompt in fixture_prompts(request) {
                replies.push(self.respond(&prompt).await?);
            }
            Ok(fixture_completion(request, replies))
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
    impl InterruptedReviewer {
        async fn respond(&self, prompt: &str) -> Result<String> {
            if prompt.starts_with("Teach one rigorous, accessible lesson") {
                self.authored.fetch_add(1, Ordering::Relaxed);
            }
            if prompt.starts_with("Review instructional quality.")
                && self.fail_review.load(Ordering::Relaxed)
            {
                return Ok(json!({"issues":[],"blockChecks":crate::features::learning::teaching_review::fixture_checks(vec![])}).to_string());
            }
            self.inner.respond(prompt).await
        }
    }

    let pool = crate::features::learning::tests::pool().await?;
    let repo = LearningRepository::new(pool.clone());
    let draft = service::generate(
        &repo,
        &outline(4, 3),
        request(LearningCourseDepth::Course),
        vec![],
    )
    .await?;
    let accepted = service::accept(
        &repo,
        AcceptLearningProgramRequestDto {
            program_id: draft.summary.id.clone(),
            expected_revision: draft.summary.revision,
            title: "Experimental design".into(),
        },
    )
    .await?;
    let quote = "A randomized comparison isolates the intervention.";
    crate::features::learning::content_verification::tests::install_source(
        &pool,
        &draft.summary.id,
        quote,
    )
    .await?;
    let mut program = repo.get(&draft.summary.id).await?;
    program.sources = repo.verification_sources(&draft.summary.id).await?;
    let lesson = &program.modules[0].lessons[0];
    let mut candidate = lesson_response();
    for field in ["blocks", "questions"] {
        for item in candidate[field].as_array_mut().unwrap() {
            item["sourceIndex"] = json!(0);
            item["quote"] = json!(quote);
        }
    }
    let llm = InterruptedReviewer {
        inner: Model(candidate.to_string()),
        fail_review: AtomicBool::new(true),
        authored: AtomicUsize::new(0),
    };
    let jobs = LearningCurriculumRepository::new(pool.clone());
    jobs.plan(&program.summary.id).await?;
    let job = jobs
        .start_job(&StartLearningGenerationJobRequestDto {
            operation_id: uuid::Uuid::new_v4().to_string(),
            program_id: program.summary.id.clone(),
            expected_revision: accepted.summary.revision,
            kind: LearningGenerationJobKind::LessonPreparation,
            request_json: json!({"lessonIds":[lesson.id]}).to_string(),
            progress_total: 1,
        })
        .await?;
    JobStore::new(pool.clone()).claim(&job.id).await?;
    let first = crate::features::learning::lesson_progress::run(
        &JobContext::detached(&JobStore::new(pool.clone()), &job.id).await?,
        crate::features::learning::lesson_drafts::run(
            &jobs,
            &job.id,
            &lesson.id,
            generation::prepare_lesson(&llm, &program, lesson),
        ),
    )
    .await;
    let error = first.expect_err("missing section reviews must fail");
    assert!(error.to_string().contains("lesson draft is saved"));
    assert_eq!(llm.authored.load(Ordering::Relaxed), 1);
    JobStore::new(pool.clone())
        .fail(&job.id, "generation_failed", &error.to_string())
        .await?;
    assert!(repo.get(&program.summary.id).await?.modules[0].lessons[0]
        .blocks
        .is_empty());

    let retry = jobs
        .retry_job(&LearningGenerationJobActionRequestDto {
            operation_id: uuid::Uuid::new_v4().to_string(),
            program_id: program.summary.id.clone(),
            expected_revision: accepted.summary.revision,
            job_id: job.id,
        })
        .await?;
    JobStore::new(pool.clone()).claim(&retry.id).await?;
    llm.fail_review.store(false, Ordering::Relaxed);
    let prepared = crate::features::learning::lesson_progress::run(
        &JobContext::detached(&JobStore::new(pool.clone()), &retry.id).await?,
        crate::features::learning::lesson_drafts::run(
            &jobs,
            &retry.id,
            &lesson.id,
            generation::prepare_lesson(&llm, &program, lesson),
        ),
    )
    .await?;
    assert_eq!(
        llm.authored.load(Ordering::Relaxed),
        1,
        "Retry must not author the lesson again"
    );
    assert!(
        prepared.verification.is_some(),
        "A draft cannot bypass evidence verification"
    );
    service::validate_prepared(&prepared, &program)?;
    jobs.publish_prepared_lessons(
        &retry.id,
        program.summary.revision,
        &[(lesson.id.clone(), prepared)],
    )
    .await?;
    assert_eq!(
        repo.get(&program.summary.id).await?.modules[0].lessons[0]
            .blocks
            .len(),
        8
    );
    Ok(())
}
#[async_trait::async_trait]
impl LLMPort for ScriptedModel {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        let mut replies = Vec::new();
        for prompt in fixture_prompts(request) {
            replies.push(self.respond(&prompt).await?);
        }
        Ok(fixture_completion(request, replies))
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
impl ScriptedModel {
    async fn respond(&self, prompt: &str) -> Result<String> {
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
}

#[tokio::test]
async fn instructional_review_repairs_once_and_rejects_unresolved_defects() -> Result<()> {
    let issue = json!({"issues":["The exercise answer contradicts its stated condition. Correct the comparison."]}).to_string();
    let repaired = json!({"wrong":false}).to_string();
    let patch = json!({"edits":[{"path":"/wrong","before":true,"after":false}]}).to_string();
    let model = ScriptedModel {
        outputs: std::sync::Mutex::new(
            vec![
                issue.clone(),
                patch.clone(),
                json!({"issues":[]}).to_string(),
            ]
            .into(),
        ),
        prompts: std::sync::Mutex::new(vec![]),
    };
    let actual = crate::features::learning::teaching::review_and_repair(
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
        outputs: std::sync::Mutex::new(vec![issue.clone(), patch, issue].into()),
        prompts: std::sync::Mutex::new(vec![]),
    };
    assert!(crate::features::learning::teaching::review_and_repair(
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
        json!({"issues":[],"blockChecks":crate::features::learning::teaching_review::fixture_checks(vec![json!({"index":0,"passageId":"section-0-passage-0","finding":"The example contradicts its return value; correct the returned total to match the accumulation.","hasDefect":has_defect})])}).to_string()
    };
    let model = ScriptedModel {
        outputs: std::sync::Mutex::new(
            vec![check(true), json!({"edits":[{"path":"/blocks/0/body","before":body,"after":format!("{body}Return the accumulated total.")}]}).to_string(), check(false)].into(),
        ),
        prompts: std::sync::Mutex::new(vec![]),
    };
    crate::features::learning::teaching::review_and_repair(
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
        assert!(prompts[1].contains("Section 1 (candidate blocks-0): The example contradicts"));
    }
    for review in [
        json!({"issues":[],"blockChecks":crate::features::learning::teaching_review::fixture_checks(vec![])}),
        json!({"issues":[],"blockChecks":crate::features::learning::teaching_review::fixture_checks(vec![json!({"index":0,"passageId":"section-0-invented-passage","finding":"This unsupported review cannot establish that the passage was inspected.","hasDefect":false})])}),
    ] {
        let model = ScriptedModel {
            outputs: std::sync::Mutex::new(vec![review.to_string()].into()),
            prompts: std::sync::Mutex::new(vec![]),
        };
        assert!(crate::features::learning::teaching::review_and_repair(
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
                json!({"edits":[{"path":"/questions/0/correctIndex","before":0,"after":1},{"path":"/questions/0/explanation","before":"AUTHOR_KEY_SECRET","after":"Two plus two is four."}]}).to_string(),
                check,
                json!({"issues":[]}).to_string(),
            ]
            .into(),
        ),
        prompts: std::sync::Mutex::new(vec![]),
    };
    let actual = crate::features::learning::teaching::review_and_repair(
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
    assert!(crate::features::learning::teaching::review_and_repair(
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
    use crate::features::learning::{
        curriculum_repository::LearningCurriculumRepository, diagnostic_generation::DiagnosticTask,
        plan_dto::*,
    };
    let pool = crate::features::learning::tests::pool().await?;
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
            expected_revision: draft.summary.revision,
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
        crate::features::learning::diagnostic_generation::validate_findings(
            &saved,
            &[response],
            &[invented]
        )
        .is_err()
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
    let repo = LearningRepository::new(crate::features::learning::tests::pool().await?);
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
    let mut program = crate::features::learning::tests::fixture();
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
    use crate::features::learning::{
        assessment_engine::LearningBlueprintRequirement,
        assessment_repository::LearningAssessmentRepository,
    };
    let pool = crate::features::learning::tests::pool().await?;
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
            expected_revision: draft.summary.revision,
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
    let candidates = crate::features::learning::assessment_generation::generate(
        &model,
        &request,
        &workspace.outcomes,
        &[],
        &[],
    )
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
async fn outline_model_streams_without_a_deadline_and_rejects_truncated_output() -> Result<()> {
    use crate::application::ports::llm_port::{CompletionRequest, CompletionResponse};
    struct StreamingModel;
    #[async_trait::async_trait]
    impl LLMPort for StreamingModel {
        async fn complete(&self, _: &CompletionRequest) -> Result<CompletionResponse> {
            panic!("Outline progress must use the streaming port");
        }
        async fn complete_with_progress(
            &self,
            request: &CompletionRequest,
            on_text: &(dyn Fn(String) -> Result<()> + Send + Sync),
        ) -> Result<CompletionResponse> {
            assert!(request.no_time_limit);
            assert!(request.wall_clock_budget().is_none());
            assert!(request.json_schema.is_some());
            assert!(
                request.max_output_tokens.unwrap() > 4000,
                "the outline estimate must not cap reasoning plus JSON"
            );
            assert!(request.max_output_tokens.unwrap() < self.max_context_tokens() as u32);
            on_text("{\"draft\":".into())?;
            on_text("true}".into())?;
            Ok(CompletionResponse {
                text: "{\"draft\":true}".into(),
                finish_reason: "length".into(),
                ..Default::default()
            })
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
    let progress = crate::features::learning::outline_progress::OutlineProgress::default();
    progress.stage(crate::features::learning::outline_progress::OutlineStage::Drafting);
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

struct DelayedOutlineModel {
    inner: ScriptedModel,
    delays: std::sync::Mutex<std::collections::VecDeque<u64>>,
}
#[async_trait::async_trait]
impl LLMPort for DelayedOutlineModel {
    async fn complete(
        &self,
        request: &crate::application::ports::llm_port::CompletionRequest,
    ) -> Result<crate::application::ports::llm_port::CompletionResponse> {
        use crate::application::ports::llm_port::{CompletionInput, CompletionResponse};
        assert!(
            request.no_time_limit,
            "outline must opt out of provider deadlines"
        );
        assert!(request.wall_clock_budget().is_none());
        let input_tokens: usize = request
            .input
            .iter()
            .filter_map(|part| match part {
                CompletionInput::Message { content, .. } => Some(self.count_tokens(content)),
                _ => None,
            })
            .sum();
        assert_eq!(
            request.max_output_tokens,
            Some((self.max_context_tokens() - input_tokens) as u32)
        );
        let prompt = request
            .input
            .iter()
            .filter_map(|part| match part {
                CompletionInput::Message { content, .. } => Some(content.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n\n");
        let seconds = self.delays.lock().unwrap().pop_front().unwrap();
        tokio::time::pause();
        tokio::time::sleep(std::time::Duration::from_secs(seconds)).await;
        tokio::time::resume();
        Ok(CompletionResponse::from_text(
            self.inner.respond(&prompt).await?,
        ))
    }
    fn model_name(&self) -> &str {
        "delayed-outline-fixture"
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
async fn outline_final_review_and_save_survive_the_old_ten_minute_cutoff() -> Result<()> {
    let pool = crate::features::learning::tests::pool().await?;
    let repo = LearningRepository::new(pool);
    let draft = outline(4, 3).0;
    let mut corrected: serde_json::Value = serde_json::from_str(&draft)?;
    corrected["modules"][0]["lessons"][0]["objective"] =
        json!("Compare the randomized groups using an observable measurement.");
    let model = DelayedOutlineModel {
        inner: ScriptedModel {
            outputs: std::sync::Mutex::new(
                vec![
                    draft.clone(),
                    json!({"issues":[{"path":"/modules/0","claim":"First module objective","reason":"Clarify the observable first module objective."}]}).to_string(),
                    json!({"module":corrected["modules"][0]}).to_string(),
                    json!({"issues":[]}).to_string(),
                ]
                .into(),
            ),
            prompts: std::sync::Mutex::new(vec![]),
        },
        // Every call exceeds the former 300s learning cap and the default
        // 600s provider cap. Checkpoints between steps must remain safe.
        delays: std::sync::Mutex::new(vec![700, 700, 700, 700].into()),
    };
    let progress = crate::features::learning::outline_progress::OutlineProgress::default();
    let program = progress
        .run(service::generate_with_references_and_progress(
            &repo,
            &model,
            request(LearningCourseDepth::Course),
            vec![],
            &[],
            &progress,
            None,
        ))
        .await?;
    assert_eq!(program.summary.lesson_count, 12);
    assert_eq!(repo.list().await?.len(), 1);
    assert_eq!(
        progress.snapshot().stage,
        crate::features::learning::outline_progress::OutlineStage::Completed
    );
    let prompts = model.inner.prompts.lock().unwrap();
    assert_eq!(prompts.len(), 4);
    assert!(prompts[3].contains("Clarify the observable first module objective."));
    assert!(prompts[3].contains("previousFindings"));
    assert_eq!(
        program.outline_review.as_ref().unwrap().status,
        crate::features::learning::outline_draft::LearningOutlineReviewStatus::Passed
    );
    assert!(!prompts[1].contains("inspect EVERY teaching block"));
    assert!(!prompts[3].contains("inspect EVERY teaching block"));
    Ok(())
}

#[tokio::test]
async fn outline_quotes_use_full_captures_and_cannot_bypass_validation() -> Result<()> {
    let quote = "A randomized comparison isolates the intervention.";
    let mut candidate: serde_json::Value = serde_json::from_str(&outline(4, 3).0)?;
    fn cite(value: &mut serde_json::Value, quote: &str) {
        match value {
            serde_json::Value::Object(object) => {
                if object.contains_key("sourceIndex") {
                    object.insert("sourceIndex".into(), json!(0));
                    object.insert("quote".into(), json!(quote));
                }
                for child in object.values_mut() {
                    cite(child, quote);
                }
            }
            serde_json::Value::Array(values) => {
                for child in values {
                    cite(child, quote);
                }
            }
            _ => {}
        }
    }
    cite(&mut candidate, quote);
    let url = "https://example.com/experimental-design";
    let captured = crate::features::learning::source_library::CapturedLearningSource {
        title: "Experimental design textbook".into(),
        publisher: None,
        requested_url: Some(url.into()),
        resolved_url: Some(url.into()),
        text: format!(
            "{}\n\n{quote}",
            "Preface and publishing information.\n".repeat(500)
        ),
        truncated: false,
        extraction_version: "test_full_capture".into(),
    };
    let id = uuid::Uuid::new_v4().to_string();
    let preview = crate::features::learning::sources::preview(&id, &captured);
    assert!(!preview.excerpt.contains(quote));
    let reference = crate::features::learning::sources::InitialReference {
        source_id: id,
        origin: url.into(),
        captured,
    };
    let repo = LearningRepository::new(crate::features::learning::tests::pool().await?);
    let corrected = json!({"module":candidate["modules"][0]}).to_string();
    candidate["modules"][0]["summary"]["quote"] =
        json!("A randomized comparison does not isolate the intervention.");
    let model = ScriptedModel {
        outputs: std::sync::Mutex::new(
            vec![
                candidate.to_string(),
                json!({"issues":[]}).to_string(),
                corrected,
                json!({"issues":[]}).to_string(),
            ]
            .into(),
        ),
        prompts: std::sync::Mutex::new(vec![]),
    };
    let mut request = request(LearningCourseDepth::Course);
    request.goal = "Learn how a randomized comparison isolates an intervention".into();
    let program = service::generate_with_references(
        &repo,
        &model,
        request,
        vec![preview.clone()],
        std::slice::from_ref(&reference),
    )
    .await?;
    assert!(model.prompts.lock().unwrap()[0].contains(quote));
    assert_eq!(model.prompts.lock().unwrap().len(), 4);
    assert!(model.prompts.lock().unwrap()[2].contains("This quotation could not be matched"));
    assert_eq!(program.summary.lesson_count, 12);
    assert!(!program.sources[0].excerpt.contains(quote));
    assert!(repo.verification_sources(&program.summary.id).await?[0]
        .excerpt
        .contains(quote));

    // Approval cannot override a bad quote. The unresolved draft is saved,
    // blocked from acceptance, and repairable after reopening the repository.
    let directory = tempfile::tempdir()?;
    let saved_database = sqlx::sqlite::SqliteConnectOptions::new()
        .filename(directory.path().join("outline-draft.sqlite"))
        .create_if_missing(true)
        .foreign_keys(true);
    let rejected_pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(saved_database.clone())
        .await?;
    sqlx::migrate!("./migrations")
        .run(&rejected_pool)
        .await
        .map_err(|error| AppError::Database(error.to_string()))?;
    let rejected_repo = LearningRepository::new(rejected_pool.clone());
    let unchanged = ScriptedModel {
        outputs: std::sync::Mutex::new(
            vec![
                candidate.to_string(),
                json!({"issues":[]}).to_string(),
                json!({"module":candidate["modules"][0]}).to_string(),
            ]
            .into(),
        ),
        prompts: std::sync::Mutex::new(vec![]),
    };
    let saved = service::generate_with_references(
        &rejected_repo,
        &unchanged,
        crate::features::learning::lessons::course_generation_tests::request(
            LearningCourseDepth::Course,
        ),
        vec![preview],
        &[reference],
    )
    .await?;
    assert_eq!(unchanged.prompts.lock().unwrap().len(), 3);
    assert_eq!(
        saved.outline_review.as_ref().unwrap().status,
        crate::features::learning::outline_draft::LearningOutlineReviewStatus::NeedsRepair
    );
    assert_eq!(saved.outline_review.as_ref().unwrap().issues.len(), 1);
    assert_eq!(rejected_repo.list().await?.len(), 1);
    assert!(service::accept(
        &rejected_repo,
        AcceptLearningProgramRequestDto {
            program_id: saved.summary.id.clone(),
            expected_revision: saved.summary.revision,
            title: saved.summary.title.clone()
        }
    )
    .await
    .is_err());
    // Close every connection and reopen the actual file, as after an app restart.
    rejected_pool.close().await;
    let reopened = LearningRepository::new(
        sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(saved_database)
            .await?,
    );
    let retrieved = reopened.get(&saved.summary.id).await?;
    assert_eq!(
        retrieved.outline_review.as_ref().unwrap().issues[0].quote,
        "A randomized comparison does not isolate the intervention."
    );
    candidate["modules"][0]["summary"]["quote"] = json!(quote);
    let repair = ScriptedModel {
        outputs: std::sync::Mutex::new(
            vec![
                json!({"issues":[]}).to_string(),
                json!({"module":candidate["modules"][0]}).to_string(),
                json!({"issues":[]}).to_string(),
            ]
            .into(),
        ),
        prompts: std::sync::Mutex::new(vec![]),
    };
    let fixed = crate::features::learning::outline_draft::repair_saved(
        &reopened,
        &repair,
        &crate::features::learning::outline_draft::RepairLearningOutlineRequestDto {
            program_id: saved.summary.id.clone(),
            expected_revision: saved.summary.revision,
        },
        &crate::features::learning::outline_progress::OutlineProgress::default(),
        None,
    )
    .await?;
    assert_eq!(
        fixed.outline_review.as_ref().unwrap().status,
        crate::features::learning::outline_draft::LearningOutlineReviewStatus::Passed
    );
    assert!(fixed.outline_review.as_ref().unwrap().issues.is_empty());
    assert_eq!(fixed.modules[0].id, saved.modules[0].id);
    assert_eq!(
        fixed.modules[0].lessons[0].id,
        saved.modules[0].lessons[0].id
    );
    assert!(fixed.summary.revision > saved.summary.revision);
    assert!(crate::features::learning::outline_draft::repair_saved(
        &reopened,
        &repair,
        &crate::features::learning::outline_draft::RepairLearningOutlineRequestDto {
            program_id: saved.summary.id.clone(),
            expected_revision: saved.summary.revision,
        },
        &crate::features::learning::outline_progress::OutlineProgress::default(),
        None
    )
    .await
    .is_err());
    service::accept(
        &reopened,
        AcceptLearningProgramRequestDto {
            program_id: fixed.summary.id.clone(),
            expected_revision: fixed.summary.revision,
            title: fixed.summary.title.clone(),
        },
    )
    .await?;
    Ok(())
}

#[path = "outline_research_tests.rs"]
mod automatic_research;

#[tokio::test]
async fn outline_repair_preserves_completed_module_edits_when_later_call_fails() -> Result<()> {
    use crate::features::learning::outline_draft::LearningOutlineReviewStatus;
    let pool = crate::features::learning::tests::pool().await?;
    let repo = LearningRepository::new(pool.clone());
    let candidate: serde_json::Value = serde_json::from_str(&outline(4, 3).0)?;
    let issues = json!({"issues":[
        {"path":"/modules/0","claim":"Comparison objective","reason":"Specify the measurement used in the comparison."},
        {"path":"/modules/1","claim":"Analysis objective","reason":"Distinguish observation from causal interpretation."}
    ]});
    let mut module = candidate["modules"][0].clone();
    module["lessons"][0]["objective"] =
        json!("Compare the randomized groups using an observable measurement.");
    let model = ScriptedModel {
        outputs: std::sync::Mutex::new(
            vec![
                candidate.to_string(),
                issues.to_string(),
                json!({"module":module}).to_string(),
            ]
            .into(),
        ),
        prompts: std::sync::Mutex::new(vec![]),
    };
    // The fourth request fails. A previous module's correction must survive.
    let saved =
        service::generate(&repo, &model, request(LearningCourseDepth::Course), vec![]).await?;
    assert_eq!(
        saved.outline_review.as_ref().unwrap().status,
        LearningOutlineReviewStatus::Unchecked
    );
    assert_eq!(
        saved.modules[0].lessons[0].objective,
        "Compare the randomized groups using an observable measurement."
    );
    assert!(saved
        .outline_review
        .as_ref()
        .unwrap()
        .note
        .contains("Completed work is saved"));
    let reopened = LearningRepository::new(pool.clone())
        .get(&saved.summary.id)
        .await?;
    assert_eq!(
        reopened.modules[0].lessons[0].objective,
        saved.modules[0].lessons[0].objective
    );
    assert_eq!(reopened.outline_review.as_ref().unwrap().issues.len(), 2);
    let checkpoints: i64 =
        sqlx::query_scalar("SELECT count(*) FROM learning_outline_checkpoints WHERE program_id=?")
            .bind(&saved.summary.id)
            .fetch_one(&pool)
            .await?;
    assert!(checkpoints >= 4);
    Ok(())
}

#[tokio::test]
async fn outline_draft_survives_cancellation_and_resumes_without_redrafting() -> Result<()> {
    use crate::features::learning::outline_progress::{OutlineProgress, OutlineRun};
    struct WaitingModel {
        draft: String,
        entered: std::sync::Arc<tokio::sync::Notify>,
    }
    #[async_trait::async_trait]
    impl LLMPort for WaitingModel {
        async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
            let mut replies = Vec::new();
            for prompt in fixture_prompts(request) {
                replies.push(self.respond(&prompt).await?);
            }
            Ok(fixture_completion(request, replies))
        }
        fn model_name(&self) -> &str {
            "waiting-fixture"
        }
        fn count_tokens(&self, s: &str) -> usize {
            s.len() / 4
        }
        fn max_context_tokens(&self) -> usize {
            128_000
        }
        async fn is_ready(&self) -> Result<bool> {
            Ok(true)
        }
    }
    impl WaitingModel {
        async fn respond(&self, prompt: &str) -> Result<String> {
            if prompt.starts_with("Review instructional quality.") {
                self.entered.notify_one();
                std::future::pending().await
            } else {
                Ok(self.draft.clone())
            }
        }
    }

    let pool = crate::features::learning::tests::pool().await?;
    let repo = LearningRepository::new(pool.clone());
    let entered = std::sync::Arc::new(tokio::sync::Notify::new());
    let model = WaitingModel {
        draft: outline(4, 3).0,
        entered: entered.clone(),
    };
    let id = uuid::Uuid::new_v4().to_string();
    let run = OutlineRun::register(Some(id.clone()), |_| {})?;
    let task_repo = repo.clone();
    let task = tokio::spawn(async move {
        run.0
            .run(service::generate_with_references_and_progress(
                &task_repo,
                &model,
                request(LearningCourseDepth::Course),
                vec![],
                &[],
                &run.0,
                None,
            ))
            .await
    });
    tokio::time::timeout(std::time::Duration::from_secs(10), entered.notified())
        .await
        .unwrap();
    let saved = repo.list().await?.pop().unwrap();
    assert!(crate::features::learning::outline_progress::cancel(&id));
    assert!(task.await.unwrap().is_err());
    let reopened = LearningRepository::new(pool);
    let saved = reopened.get(&saved.id).await?;
    assert_eq!(
        saved.outline_review.as_ref().unwrap().status,
        crate::features::learning::outline_draft::LearningOutlineReviewStatus::Unchecked
    );
    let model = ScriptedModel {
        outputs: std::sync::Mutex::new(vec![json!({"issues":[]}).to_string()].into()),
        prompts: std::sync::Mutex::new(vec![]),
    };
    let resumed = crate::features::learning::outline_draft::repair_saved(
        &reopened,
        &model,
        &crate::features::learning::outline_draft::RepairLearningOutlineRequestDto {
            program_id: saved.summary.id,
            expected_revision: saved.summary.revision,
        },
        &OutlineProgress::default(),
        None,
    )
    .await?;
    assert_eq!(
        resumed.outline_review.as_ref().unwrap().status,
        crate::features::learning::outline_draft::LearningOutlineReviewStatus::Passed
    );
    assert_eq!(model.prompts.lock().unwrap().len(), 1);
    Ok(())
}

#[tokio::test]
async fn outline_automatic_repair_continues_while_findings_decrease() -> Result<()> {
    let repo = LearningRepository::new(crate::features::learning::tests::pool().await?);
    let candidate: serde_json::Value = serde_json::from_str(&outline(4, 3).0)?;
    let first = json!({"path":"/modules/0","claim":"Comparison objective","reason":"Specify the measurement used in the comparison."});
    let second = json!({"path":"/modules/1","claim":"Analysis objective","reason":"Distinguish observation from causal interpretation."});
    let mut module0 = candidate["modules"][0].clone();
    module0["lessons"][0]["objective"] =
        json!("Measure the effect of random assignment on the group comparison.");
    let mut module1 = candidate["modules"][1].clone();
    module1["lessons"][0]["objective"] =
        json!("Compare group measurements and describe the observed difference.");
    let mut final1 = module1.clone();
    final1["lessons"][0]["objective"]=json!("Describe the observed difference and explain why observation alone does not establish causality.");
    let model = ScriptedModel {
        outputs: std::sync::Mutex::new(
            vec![
                candidate.to_string(),
                json!({"issues":[first,second]}).to_string(),
                json!({"module":module0}).to_string(),
                json!({"module":module1}).to_string(),
                json!({"issues":[second]}).to_string(),
                json!({"module":final1}).to_string(),
                json!({"issues":[]}).to_string(),
            ]
            .into(),
        ),
        prompts: std::sync::Mutex::new(vec![]),
    };
    let saved =
        service::generate(&repo, &model, request(LearningCourseDepth::Course), vec![]).await?;
    assert_eq!(
        saved.outline_review.as_ref().unwrap().status,
        crate::features::learning::outline_draft::LearningOutlineReviewStatus::Passed
    );
    assert_eq!(saved.outline_review.as_ref().unwrap().repair_passes, 2);
    assert_eq!(model.prompts.lock().unwrap().len(), 7);
    Ok(())
}

#[tokio::test]
async fn outline_research_saves_full_pages_and_never_uses_snippets_or_truncated_captures(
) -> Result<()> {
    use crate::features::{
        function_calling::dto::{FetchUrlContentOutput, WebSearchResult},
        web::mocks::MockWebService,
    };
    let pool = crate::features::learning::tests::pool().await?;
    let repo = LearningRepository::new(pool.clone());
    let candidate: serde_json::Value = serde_json::from_str(&outline(4, 3).0)?;
    let issue = json!({"issues":[{"path":"/modules/0","claim":"The experimental comparison","reason":"The comparison needs evidence for its factual premise."}]});
    let model = ScriptedModel {
        outputs: std::sync::Mutex::new(
            vec![
                candidate.to_string(),
                issue.to_string(),
                json!({"module":candidate["modules"][0]}).to_string(),
            ]
            .into(),
        ),
        prompts: std::sync::Mutex::new(vec![]),
    };
    let mut program =
        service::generate(&repo, &model, request(LearningCourseDepth::Course), vec![]).await?;
    let mut draft = repo.outline_draft(&program.summary.id).await?.unwrap();
    let web = MockWebService::new();
    let url = "https://example.org/full-reference";
    let truncated = "https://example.org/truncated";
    web.set_search_results(
        vec![url, truncated]
            .into_iter()
            .map(|url| WebSearchResult {
                title: "Reference".into(),
                url: url.into(),
                snippet: "THIS SEARCH SNIPPET IS NOT EVIDENCE".into(),
                published_date: None,
                source: None,
            })
            .collect(),
    );
    let full = "A randomized comparison isolates the intervention. ".repeat(100);
    for (url, is_truncated) in [(url, false), (truncated, true)] {
        web.set_url_content(
            url,
            FetchUrlContentOutput {
                url: url.into(),
                title: Some("Experimental reference".into()),
                content: full.clone(),
                content_truncated: is_truncated,
                word_count: 700,
                fetch_time_ms: 1.,
                content_type: None,
                from_cache: false,
            },
        );
    }
    let mut sources = vec![];
    crate::features::learning::outline_research::research(
        &repo,
        &web,
        &mut program,
        &mut draft,
        &mut sources,
        &crate::features::learning::outline_progress::OutlineProgress::default(),
        &mut crate::features::learning::outline_research::ResearchAttempts::default(),
    )
    .await?;
    assert_eq!(sources.len(), 1);
    assert_eq!(sources[0].excerpt, full);
    let saved = repo.verification_sources(&program.summary.id).await?;
    assert_eq!(saved.len(), 1);
    assert_eq!(saved[0].excerpt, full.trim());
    assert_eq!(program.sources[0].excerpt.chars().count(), 2400);
    assert!(!sources[0].excerpt.contains("SEARCH SNIPPET"));
    let restored = LearningRepository::new(pool)
        .outline_draft(&program.summary.id)
        .await?
        .unwrap();
    assert_eq!(restored.source_ids, vec![sources[0].id.clone()]);
    assert_eq!(
        restored.review.status,
        crate::features::learning::outline_draft::LearningOutlineReviewStatus::Unchecked
    );
    Ok(())
}

/// Explicit live acceptance test: uses the configured remote llama.cpp model,
/// captures the public URL through the real web service, and saves only to a
/// temporary database. Credentials and generated content are never printed.
#[tokio::test]
#[ignore = "requires LATTICE_LLAMACPP_SETTINGS and LATTICE_OUTLINE_REQUEST"]
async fn live_outline_generation_and_save() -> Result<()> {
    use crate::{
        application::contracts::settings::LLMSettingsDto,
        features::{
            llm::llama_cpp::LlamaCppLlm,
            web::{services::web::WebService, WebServiceTrait},
        },
    };
    let settings_path = std::env::var("LATTICE_LLAMACPP_SETTINGS").unwrap();
    let request_path = std::env::var("LATTICE_OUTLINE_REQUEST").unwrap();
    let settings: serde_json::Value = serde_json::from_slice(&std::fs::read(settings_path)?)?;
    let config: LLMSettingsDto = serde_json::from_value(settings["settings"]["llm"].clone())?;
    let request: GenerateLearningProgramRequestDto =
        serde_json::from_slice(&std::fs::read(request_path)?)?;
    assert!(
        request.document_ids.is_empty(),
        "live fixture supports public URLs only"
    );
    service::validate_request(&request)?;
    let temp = tempfile::tempdir()?;
    let web = WebService::new(temp.path())?;
    struct ObservedModel {
        inner: LlamaCppLlm,
        call: std::sync::atomic::AtomicUsize,
        replay_draft: Option<String>,
    }
    #[async_trait::async_trait]
    impl LLMPort for ObservedModel {
        async fn complete(
            &self,
            request: &crate::application::ports::llm_port::CompletionRequest,
        ) -> Result<crate::application::ports::llm_port::CompletionResponse> {
            self.inner.complete(request).await
        }

        async fn complete_with_progress(
            &self,
            request: &crate::application::ports::llm_port::CompletionRequest,
            on_text: &(dyn Fn(String) -> Result<()> + Send + Sync),
        ) -> Result<crate::application::ports::llm_port::CompletionResponse> {
            let call = self.call.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let replayed = call == 0 && self.replay_draft.is_some();
            let response = if let Some(draft) = self.replay_draft.as_ref().filter(|_| call == 0) {
                println!(
                    "Replaying a previously recorded draft; this step makes no model request."
                );
                on_text(draft.clone())?;
                crate::application::ports::llm_port::CompletionResponse {
                    text: draft.clone(),
                    finish_reason: "stop".into(),
                    ..Default::default()
                }
            } else {
                let response = self.inner.complete_with_progress(request, on_text).await?;
                println!(
                    "Live model call {call}: {} input tokens, {} output tokens, finish {}",
                    response.input_tokens, response.output_tokens, response.finish_reason
                );
                response
            };
            // Optional local test artifacts contain only our prompts and public
            // answer text, never provider reasoning state or connection settings.
            if let Ok(directory) = std::env::var("LATTICE_OUTLINE_CALLS") {
                std::fs::create_dir_all(&directory)?;
                std::fs::write(
                    std::path::Path::new(&directory).join(format!("call-{call}.json")),
                    serde_json::to_vec_pretty(&json!({
                        "input":request.input,"answer":response.text,"finishReason":response.finish_reason,
                        "inputTokens":response.input_tokens,"outputTokens":response.output_tokens,"replayed":replayed
                    }))?,
                )?;
            }
            Ok(response)
        }

        fn model_name(&self) -> &str {
            self.inner.model_name()
        }
        fn max_context_tokens(&self) -> usize {
            self.inner.max_context_tokens()
        }
        fn count_tokens(&self, text: &str) -> usize {
            self.inner.count_tokens(text)
        }
        async fn is_ready(&self) -> Result<bool> {
            self.inner.is_ready().await
        }
    }
    let client = ObservedModel {
        inner: LlamaCppLlm::new(&config)?,
        call: std::sync::atomic::AtomicUsize::new(0),
        replay_draft: std::env::var("LATTICE_OUTLINE_REPLAY_DRAFT")
            .ok()
            .map(|path| -> Result<String> {
                let recorded: serde_json::Value = serde_json::from_slice(&std::fs::read(path)?)?;
                recorded["answer"]
                    .as_str()
                    .map(str::to_owned)
                    .ok_or_else(|| {
                        AppError::InvalidInput("Recorded call has no public draft text".into())
                    })
            })
            .transpose()?,
    };
    let pool = crate::features::learning::tests::pool().await?;
    let repo = LearningRepository::new(pool);
    let start = std::time::Instant::now();
    let last_stage = std::sync::Mutex::new((None, 0));
    let run =
        crate::features::learning::outline_progress::OutlineRun::register(None, move |update| {
            let mut last = last_stage.lock().unwrap();
            if last.0 != Some(update.stage) || update.elapsed_seconds >= last.1 + 30 {
                println!(
                    "Live outline: {:?} at {}s, {} response characters",
                    update.stage, update.elapsed_seconds, update.response_characters
                );
                *last = (Some(update.stage), update.elapsed_seconds);
            }
        })?;
    let progress = &run.0;
    let program = progress
        .run(async {
            progress
                .stage(crate::features::learning::outline_progress::OutlineStage::ReadingSources);
            let mut sources = Vec::new();
            let mut references = Vec::new();
            for url in &request.source_urls {
                let article = web.fetch_reference_content(url).await?;
                let captured = crate::features::learning::source_library::CapturedLearningSource {
                    title: article.title.unwrap_or_else(|| url.clone()),
                    publisher: None,
                    requested_url: Some(url.clone()),
                    resolved_url: Some(article.url),
                    text: article.content,
                    truncated: article.content_truncated,
                    extraction_version: "web_reference_v1".into(),
                };
                assert!(!captured.truncated);
                let id = uuid::Uuid::new_v4().to_string();
                sources.push(crate::features::learning::sources::preview(&id, &captured));
                references.push(crate::features::learning::sources::InitialReference {
                    source_id: id,
                    origin: url.clone(),
                    captured,
                });
            }
            progress.model(client.model_name());
            service::generate_with_references_and_progress(
                &repo,
                &client,
                request,
                sources,
                &references,
                progress,
                Some(&web),
            )
            .await
        })
        .await?;
    assert_eq!(repo.list().await?.len(), 1);
    assert!(!program.modules.is_empty());
    println!(
        "Live outline saved: {} modules, {} lessons in {}s",
        program.summary.module_count,
        program.summary.lesson_count,
        start.elapsed().as_secs()
    );
    if let Ok(path) = std::env::var("LATTICE_OUTLINE_OUTPUT") {
        std::fs::write(path, serde_json::to_vec_pretty(&program)?)?;
    }
    Ok(())
}

#[test]
fn program_request_accepts_large_source_lists_but_still_validates_each_entry() -> Result<()> {
    let mut input = request(LearningCourseDepth::Focused);
    input.document_ids = (0..105).map(|_| uuid::Uuid::new_v4().to_string()).collect();
    input.source_urls = (0..105)
        .map(|i| format!("https://example.org/source/{i}"))
        .collect();
    service::validate_request(&input)?;
    input.source_urls.push("file:///private/invalid".into());
    assert!(service::validate_request(&input).is_err());
    input.source_urls.pop();
    input.document_ids.push(input.document_ids[0].clone());
    assert!(service::validate_request(&input).is_err());
    Ok(())
}

#[test]
fn program_request_goal_and_prior_knowledge_have_no_character_limit() -> Result<()> {
    let mut input = request(LearningCourseDepth::Focused);
    input.goal = "Detailed learning goal. ".repeat(1_000);
    input.prior_knowledge = "Relevant experience and context. ".repeat(1_000);
    service::validate_request(&input)
}
