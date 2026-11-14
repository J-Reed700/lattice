//! Exercise the real authoring, validation and persistence boundaries without a live model.
use crate::content_verification::tests::{fixture_completion, fixture_prompts, fixture_task};
use crate::lessons::course_plan::CoursePlan;
use crate::{
    dto::*, generation, practice_repository::LearningPracticeRepository,
    repository::LearningRepository, service,
};
use lattice_core::shared::persistence::Database;
use lattice_core::shared::runtime::jobs::{JobContext, JobStore};
use lattice_core::{
    application::ports::llm_port::{CompletionRequest, CompletionResponse},
    application::ports::LLMPort,
    shared::error::{AppError, Result},
};
use serde_json::json;

pub(crate) struct Model(pub(crate) String);
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
        if let Some(reply) = crate::content_verification::tests::fixture_response(prompt) {
            return Ok(reply);
        }
        if prompt.starts_with("Source passages:") {
            return Ok("supported\nReason: The fixture reference supports the experimental design.\nSource quote: A randomized comparison isolates the intervention.\nSource passage: passage-0".into());
        }
        if prompt.starts_with("Review instructional quality.") {
            let context: serde_json::Value = fixture_task(prompt).expect("a JSON task");
            // Only the outline plan review numbers the modules it shows;
            // the teaching reviews keep their own response shape.
            if context.get("moduleNumbering").is_some() {
                return Ok(json!({"newFindings":[]}).to_string());
            }
            if context["candidate"].get("blocks").is_none() {
                return Ok(json!({"issues":[]}).to_string());
            }
            let checks: Vec<_> = context["candidate"]["blocks"].as_array().into_iter().flatten().enumerate().map(|(index, block)|json!({"index":index,"passageId":block["bodyPassages"][0]["id"],"finding":"The comparison and measurement task is consistent with the randomized experiment objective.","hasDefect":false})).collect();
            return Ok(
                json!({"issues":[],"blockChecks":crate::teaching_review::fixture_checks(checks)})
                    .to_string(),
            );
        }
        Ok(self.0.clone())
    }
}

pub(crate) fn request(depth: LearningCourseDepth) -> GenerateLearningProgramRequestDto {
    GenerateLearningProgramRequestDto {
        goal: "Design experiments and interpret their results".into(),
        prior_knowledge: "Basic algebra".into(),
        minutes_per_session: 30,
        document_ids: vec![],
        source_urls: vec![],
        course_depth: Some(depth),
    }
}
/// A repair answer whose lessons revise the existing lessons at `froms`
/// (`None` for a new lesson), in answer order.
pub(crate) fn repair_answer_with(
    mut module: serde_json::Value,
    froms: &[Option<usize>],
) -> serde_json::Value {
    if let Some(lessons) = module["lessons"].as_array_mut() {
        for (lesson, from) in lessons.iter_mut().zip(froms) {
            lesson["from"] = json!(from);
        }
    }
    json!({"module":module})
}

/// A repair answer that keeps every lesson of `module` where it is.
pub(crate) fn repair_answer(module: serde_json::Value) -> serde_json::Value {
    let count = module["lessons"].as_array().map_or(0, Vec::len);
    let froms: Vec<_> = (0..count).map(Some).collect();
    repair_answer_with(module, &froms)
}

pub(crate) fn outline(modules: usize, lessons: usize) -> Model {
    Model(json!({"modules":(0..modules).map(|m| json!({
        "title":format!("Experimental design module {m}"),
        "prerequisiteIndices":if m == 0 {vec![]} else {vec![m-1]},
        "project":{"title":format!("Experiment project milestone {m}"),"brief":"Develop the next stage of the randomized experiment and revise the previous stage using feedback.","deliverables":["A written protocol identifying treatment and control groups.","A measurement plan and a limitations section for this experiment."],"successCriteria":["The protocol isolates the intervention and explains random assignment.","The analysis distinguishes observed results from causal interpretation."]},
        "summary":{"text":"Build a skill and apply it to an experiment.","sourceIndex":null,"quote":""},
        "outcomes":[{"text":"Design a comparison with a clear outcome.","sourceIndex":null,"quote":""},{"text":"Interpret results and explain uncertainty.","sourceIndex":null,"quote":""}],
        "lessons":(0..lessons).map(|l| json!({"title":format!("Experimental design {m}.{l}"),"objective":"Design a controlled experiment and explain its limitations.","estimatedMinutes":30,"sourceIndex":null,"quote":""})).collect::<Vec<_>>()
    })).collect::<Vec<_>>()} ).to_string())
}
/// The one reference a topic course's lesson is prepared from: it holds the
/// lessons' quote, and shares words with what the lessons teach.
const REFERENCE: &str =
    "A randomized comparison isolates the intervention. A controlled experiment states its limitations.";

fn task_rubric() -> serde_json::Value {
    json!([
        {"title":"Randomized comparison","description":"Define treatment and comparison groups, specify random allocation, and explain how it limits confounding.","dimension":"application"},
        {"title":"Measurement and limitations","description":"State an observable outcome and explain one limitation of interpreting the difference between groups.","dimension":"explanation"}
    ])
}
/// The sections of a lesson in the mix it is written for: two ideas, each
/// explained, worked and checked, one contrasted, then practice, recall and
/// recap.
const LESSON_SHAPE: [&str; 11] = [
    "explanation",
    "worked_example",
    "contrast",
    "check",
    "explanation",
    "worked_example",
    "check",
    "guided_practice",
    "independent_practice",
    "reflection",
    "recap",
];

fn lesson_response() -> serde_json::Value {
    let teaching = |step: usize, pass: usize| {
        format!("Start with a measurable question, identify the comparison, and define the outcome before collecting data in design step {step}, pass {pass}. Explain the assumptions behind the design in design step {step}, pass {pass}. Work through the decisions in order and examine how a confounding variable could change the conclusion of design step {step}, pass {pass}. A concrete example should show both the result and the limitations of that result in design step {step}, pass {pass}. ")
    };
    json!({
        "blocks":(LESSON_SHAPE.iter().enumerate().map(|(i,kind)| json!({"kind":kind,"title":format!("Design step {i}"),"body":if *kind == "check" { format!("{}\n\n{}\n\n{}", teaching(i, 0), crate::generation::CHECK_REVEAL, teaching(i, 1)) } else { format!("{}{} Assignment step {}: produce a design, state your assumptions, and justify your decisions.", teaching(i, 0), teaching(i, 1), char::from(b'a' + u8::try_from(i).unwrap_or(0))) },"rubric":if ["guided_practice","independent_practice"].contains(kind) { task_rubric() } else { json!([]) },"sourceIndex":null,"quote":"","citations":[]})).collect::<Vec<_>>()),
        "questions":(["practice","quiz","test"].iter().flat_map(|kind| (0..2).map(move |i|json!({"kind":kind,"prompt":format!("Which experimental design decision is appropriate in {kind} scenario {i}?"),"options":["Compare randomized groups","Change several variables together","Select only positive results","Omit the comparison group"],"correctIndex":0,"rationales":["","Several changed variables confound the result.","Selecting only positive results biases the sample.","Without a comparison group nothing separates the effect."],"explanation":"A randomized comparison helps separate the intervention from confounding factors.","sourceIndex":null,"quote":"","citations":[]}))).collect::<Vec<_>>())
    })
}

#[tokio::test]
async fn topic_course_with_acquired_reference_runs_through_assignment_and_recall() -> Result<()> {
    let pool = crate::tests::pool().await?;
    let db = Database::new(pool.clone());
    let repo = LearningRepository::new(db.clone());
    let drafts = crate::outline_draft::DraftRunRegistry::new();
    let draft = service::generate(
        &repo,
        &drafts,
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
        &drafts,
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
    // Topic outlines need evidence before they can become ready lessons. The
    // reference shares the lesson's own words ("controlled experiment"): a
    // full-text match counts whole words of two letters or more, as the
    // library's does, so a lone "a" is no match.
    crate::content_verification::tests::install_source(&db, &draft.summary.id, REFERENCE).await?;
    let mut lesson_candidate = lesson_response();
    for field in ["blocks", "questions"] {
        for item in lesson_candidate[field].as_array_mut().unwrap() {
            item["sourceIndex"] = json!(0);
            item["quote"] = json!("A randomized comparison isolates the intervention.");
        }
    }
    crate::tests::assert_conflict(
        service::prepare(
            &db,
            &repo,
            &Model(lesson_candidate.to_string()),
            &crate::embedded_runtime::BuiltinRuntimes::for_tests(),
            &Default::default(),
            PrepareLearningLessonRequestDto {
                program_id: draft.summary.id.clone(),
                lesson_id: lesson_id.clone(),
                expected_revision: accepted.summary.revision + 1,
            },
        )
        .await,
    );
    let prepared = service::prepare(
        &db,
        &repo,
        &Model(lesson_candidate.to_string()),
        &crate::embedded_runtime::BuiltinRuntimes::for_tests(),
        &Default::default(),
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
    assert_eq!(lesson.blocks.len(), LESSON_SHAPE.len());
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
    let practice = LearningPracticeRepository::new(db.clone());
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
        crate::practice_generation::grade(&uncertain, &uncertain_session).await?;
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
    let (_, repaired) = crate::practice_generation::grade(&repair_model, &attempted).await?;
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
    assert!(matches!(
        crate::practice_generation::grade(&still_invalid, &attempted).await,
        Err(AppError::ModelAnswerUnusable(_))
    ));

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
            .await?;
    assert!(!repo
        .accept_card_draft(&prepared.summary.id, &draft_id)
        .await?
        .is_empty());
    // The lesson's own reviewed questions joined its quiz bank.
    let banked: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM learning_assessment_candidates WHERE lesson_id=? AND origin='lesson'",
    )
    .bind(&lesson_id)
    .fetch_one(&pool)
    .await?;
    assert!(banked > 0);
    Ok(())
}

pub(crate) struct ScriptedModel {
    pub(crate) outputs: std::sync::Mutex<std::collections::VecDeque<String>>,
    pub(crate) prompts: std::sync::Mutex<Vec<String>>,
}

/// Where `schema` bounds a string's length above a blank: each `maxLength`
/// and each `minLength` over one, as `path: bound`.
pub(crate) fn length_bounds(schema: &serde_json::Value) -> Vec<String> {
    fn walk(value: &serde_json::Value, path: &str, found: &mut Vec<String>) {
        match value {
            serde_json::Value::Object(fields) => {
                for (key, inner) in fields {
                    let at = format!("{path}/{key}");
                    let above_one = inner.as_u64().is_some_and(|bound| bound > 1);
                    if key == "maxLength" || (key == "minLength" && above_one) {
                        found.push(format!("{at}: {inner}"));
                    }
                    walk(inner, &at, found);
                }
            }
            serde_json::Value::Array(items) => {
                for (position, inner) in items.iter().enumerate() {
                    walk(inner, &format!("{path}/{position}"), found);
                }
            }
            _ => {}
        }
    }
    let mut found = Vec::new();
    walk(schema, "", &mut found);
    found
}

/// How many entries the `properties` of `schema` hold in all, at any depth.
pub(crate) fn property_count(schema: &serde_json::Value) -> usize {
    match schema {
        serde_json::Value::Object(fields) => fields
            .iter()
            .map(|(key, inner)| {
                let own = match inner {
                    serde_json::Value::Object(properties) if key == "properties" => {
                        properties.len()
                    }
                    _ => 0,
                };
                own + property_count(inner)
            })
            .sum(),
        serde_json::Value::Array(items) => items.iter().map(property_count).sum(),
        _ => 0,
    }
}

#[tokio::test]
async fn failed_lesson_review_retries_the_saved_draft_then_verifies_and_publishes() -> Result<()> {
    use crate::{
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
                return Ok(json!({"issues":[],"blockChecks":crate::teaching_review::fixture_checks(vec![])}).to_string());
            }
            self.inner.respond(prompt).await
        }
    }

    let pool = crate::tests::pool().await?;
    let db = Database::new(pool.clone());
    let repo = LearningRepository::new(db.clone());
    let drafts = crate::outline_draft::DraftRunRegistry::new();
    let draft = service::generate(
        &repo,
        &drafts,
        &outline(4, 3),
        request(LearningCourseDepth::Course),
        vec![],
    )
    .await?;
    let accepted = service::accept(
        &repo,
        &drafts,
        AcceptLearningProgramRequestDto {
            program_id: draft.summary.id.clone(),
            expected_revision: draft.summary.revision,
            title: "Experimental design".into(),
        },
    )
    .await?;
    let quote = "A randomized comparison isolates the intervention.";
    crate::content_verification::tests::install_source(&db, &draft.summary.id, REFERENCE).await?;
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
    let jobs = LearningCurriculumRepository::new(db.clone());
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
    JobStore::new(db.clone()).claim(&job.id).await?;
    let runtimes = crate::embedded_runtime::BuiltinRuntimes::for_tests();
    let first = crate::lesson_progress::run(
        &JobContext::detached(&JobStore::new(db.clone()), &job.id).await?,
        crate::lesson_drafts::run(
            &db,
            &jobs,
            &job.id,
            &lesson.id,
            generation::prepare_lesson(
                &llm,
                &runtimes,
                &program,
                lesson,
                &CoursePlan::blank(&program),
            ),
        ),
    )
    .await;
    let error = first.expect_err("missing section reviews must fail");
    assert!(error.to_string().contains("still needs a valid check"));
    assert_eq!(llm.authored.load(Ordering::Relaxed), 1);
    JobStore::new(db.clone())
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
    JobStore::new(db.clone()).claim(&retry.id).await?;
    llm.fail_review.store(false, Ordering::Relaxed);
    let prepared = crate::lesson_progress::run(
        &JobContext::detached(&JobStore::new(db.clone()), &retry.id).await?,
        crate::lesson_drafts::run(
            &db,
            &jobs,
            &retry.id,
            &lesson.id,
            generation::prepare_lesson(
                &llm,
                &runtimes,
                &program,
                lesson,
                &CoursePlan::blank(&program),
            ),
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
        LESSON_SHAPE.len()
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
/// A teaching-review verdict on the finding `id`: "present" or "resolved".
pub(crate) fn verdict(id: &str, verdict: &str) -> serde_json::Value {
    json!({"id":id,"quote":"","verdict":verdict})
}

/// A "present" verdict on the finding `id` that quotes the words showing it.
pub(crate) fn present(id: &str, quote: &str) -> serde_json::Value {
    json!({"id":id,"quote":quote,"verdict":"present"})
}

/// A teaching-review reply that reports no issue, finds no defect in the
/// sections it checks, and gives `verdicts` on the findings it was asked about.
pub(crate) fn clean_review(sections: &[usize], verdicts: &[serde_json::Value]) -> String {
    review_reply(
        &[],
        sections
            .iter()
            .map(|index| section_check(*index, None))
            .collect(),
        verdicts,
    )
}

/// The check of section `index`: a defect when `defect` is given, naming the
/// lesson concern it fails. A clean check names none, as every review's does.
pub(crate) fn section_check(index: usize, defect: Option<&str>) -> serde_json::Value {
    let finding = defect.unwrap_or("No defect was found in this section.");
    let mut check = json!({"index":index,"passageId":format!("section-{index}-passage-0"),"finding":finding,"hasDefect":defect.is_some()});
    if defect.is_some() {
        check["concerns"] = json!(["promise"]);
    }
    check
}

/// A teaching-review reply as it is written: `checks` are for exactly the
/// sections the review asks about, `verdicts` for exactly the findings it asks
/// about (none: no `openFindings`).
pub(crate) fn review_reply(
    issues: &[&str],
    checks: Vec<serde_json::Value>,
    verdicts: &[serde_json::Value],
) -> String {
    let mut reply =
        json!({"issues":issues,"blockChecks":crate::teaching_review::fixture_checks(checks)});
    if !verdicts.is_empty() {
        reply["openFindings"] = json!(verdicts);
    }
    reply.to_string()
}

impl ScriptedModel {
    async fn respond(&self, prompt: &str) -> Result<String> {
        self.prompts
            .lock()
            .map_err(|_| AppError::InternalError("fixture lock".into()))?
            .push(prompt.into());
        let mut outputs = self
            .outputs
            .lock()
            .map_err(|_| AppError::InternalError("fixture lock".into()))?;
        let linking = prompt.starts_with(crate::outline_linking::LINK_SYSTEM);
        // A links sentinel answers every linking batch of one run and is spent
        // when a call of another kind arrives.
        if !linking {
            while outputs
                .front()
                .is_some_and(|output| output.starts_with(LINKS_SENTINEL))
            {
                outputs.pop_front();
            }
        }
        let front = outputs
            .front()
            .ok_or_else(|| AppError::InvalidInput("Unexpected extra model call".into()))?;
        if linking && front.starts_with(LINKS_SENTINEL) {
            return Ok(link_reply(prompt, front));
        }
        Ok(outputs.pop_front().unwrap_or_default())
    }
}

/// [`ScriptedModel`] with a window of `.1` tokens.
pub(crate) struct Windowed<'a>(pub(crate) &'a ScriptedModel, pub(crate) usize);

#[async_trait::async_trait]
impl LLMPort for Windowed<'_> {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        self.0.complete(request).await
    }
    fn model_name(&self) -> &str {
        self.0.model_name()
    }
    fn count_tokens(&self, text: &str) -> usize {
        self.0.count_tokens(text)
    }
    fn max_context_tokens(&self) -> usize {
        self.1
    }
    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
}

/// Prefix of a scripted evidence-linking reply. How many batches a run makes
/// and which items each holds depend on retrieval, so the reply is built from
/// the prompt: the sentinel maps item paths to the passage text to choose and
/// every other item gets `none`.
pub(crate) const LINKS_SENTINEL: &str = "{\"@links\":";

fn link_reply(prompt: &str, sentinel: &str) -> String {
    let picks: serde_json::Value = serde_json::from_str(sentinel).expect("links sentinel");
    let payload: serde_json::Value = fixture_task(prompt).expect("link prompt");
    let passages = payload["passages"].as_array().cloned().unwrap_or_default();
    let text_of = |id: &str| {
        passages
            .iter()
            .find(|p| p["id"] == id)
            .and_then(|p| p["passage"].as_str())
            .unwrap_or_default()
    };
    let links: Vec<_> = payload["items"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|item| {
            let id = item["id"].as_str().unwrap_or_default();
            let candidates: Vec<&str> = item["candidates"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|c| c.as_str())
                .collect();
            let chosen = picks["@links"][id].as_str().and_then(|wanted| {
                candidates
                    .iter()
                    .find(|c| text_of(c) == wanted)
                    .or_else(|| candidates.iter().find(|c| text_of(c).contains(wanted)))
                    .copied()
            });
            json!({"id":id,"candidate":chosen.unwrap_or("none"),"reason":if chosen.is_some() {"The passage addresses this item."} else {"No candidate addresses this item."}})
        })
        .collect();
    json!({"links":links}).to_string()
}

#[tokio::test]
async fn a_question_may_cite_a_reference_verification_saved_for_the_program() -> Result<()> {
    use crate::content_verification::research::{self, tests as research_tests};
    const CLAIM: &str =
        "Random assignment reduces confounding in comparisons between treatment groups.";
    struct Researching {
        inner: Model,
    }
    #[async_trait::async_trait]
    impl LLMPort for Researching {
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
    impl Researching {
        async fn respond(&self, prompt: &str) -> Result<String> {
            if prompt.starts_with("Judge which candidate passages substantively cover") {
                return Ok(json!({"topics":[{"id":"t1","covering":["p1"]}]}).to_string());
            }
            if prompt.starts_with("Plan focused reference searches") {
                return Ok(json!({"needs":[{"gap":0,"query":"random assignment confounding official guidance","kind":"web"}]}).to_string());
            }
            if prompt.starts_with("Select trustworthy references") {
                return Ok(research_tests::selection(&[0]));
            }
            if prompt.starts_with("Source passages:") {
                let (passages, tail) = prompt
                    .split_once("\n\nClaim: ")
                    .ok_or_else(|| AppError::InvalidInput("claim check without a claim".into()))?;
                if tail.split("\n\n").next() == Some(CLAIM) {
                    return Ok(if passages.contains(CLAIM) {
                        format!("supported\nReason: The complete capture directly states the claim.\nSource quote: {CLAIM}\nSource passage: passage-0")
                    } else {
                        "unsupported\nReason: The saved reference does not state the claim.\nSource quote: none".into()
                    });
                }
            }
            self.inner.respond(prompt).await
        }
    }

    let pool = crate::tests::pool().await?;
    let db = Database::new(pool.clone());
    let repo = LearningRepository::new(db.clone());
    let drafts = crate::outline_draft::DraftRunRegistry::new();
    let draft = service::generate(
        &repo,
        &drafts,
        &outline(4, 3),
        request(LearningCourseDepth::Course),
        vec![],
    )
    .await?;
    let accepted = service::accept(
        &repo,
        &drafts,
        AcceptLearningProgramRequestDto {
            program_id: draft.summary.id.clone(),
            expected_revision: draft.summary.revision,
            title: "Experimental design".into(),
        },
    )
    .await?;
    crate::content_verification::tests::install_source(&db, &draft.summary.id, REFERENCE).await?;
    let mut program = repo.get(&draft.summary.id).await?;
    program.sources = repo.verification_sources(&draft.summary.id).await?;
    let lesson = &program.modules[0].lessons[0];
    let mut candidate = lesson_response();
    for field in ["blocks", "questions"] {
        for item in candidate[field].as_array_mut().unwrap() {
            item["sourceIndex"] = json!(0);
            item["quote"] = json!("A randomized comparison isolates the intervention.");
        }
    }
    candidate["questions"][0]["explanation"] = json!(CLAIM);
    let llm = Researching {
        inner: Model(candidate.to_string()),
    };
    let runtimes = crate::embedded_runtime::BuiltinRuntimes::for_tests();

    let prepared = Box::pin(research::run(
        db.clone(),
        accepted.summary.id.clone(),
        accepted.summary.goal.clone(),
        Some(research_tests::web(false)),
        generation::prepare_lesson(
            &llm,
            &runtimes,
            &program,
            lesson,
            &CoursePlan::blank(&program),
        ),
    ))
    .await?;

    let saved = repo.verification_sources(&program.summary.id).await?;
    let added = saved
        .iter()
        .find(|source| !program.sources.iter().any(|known| known.id == source.id))
        .expect("verification's research saved a reference for the program");
    assert!(
        prepared.questions[0].source_ids.contains(&added.id),
        "{:?}",
        prepared.questions[0].source_ids
    );
    assert!(prepared.questions[0]
        .source_ids
        .contains(&program.sources[0].id));
    Ok(())
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
                json!({"issues":[],"openFindings":[verdict("reviewer/1", "resolved")]}).to_string(),
            ]
            .into(),
        ),
        prompts: std::sync::Mutex::new(vec![]),
    };
    let actual = crate::teaching::review_and_repair(
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
        assert!(prompts[2].contains("openFindings"));
        assert!(prompts[2].contains("judge the correction"));
        assert!(prompts[2].contains("Correct the comparison"));
    }
    let model = ScriptedModel {
        outputs: std::sync::Mutex::new(
            vec![
                issue.clone(),
                patch,
                json!({"issues":["The exercise answer contradicts its stated condition. Correct the comparison."],"openFindings":[present("reviewer/1", "The exercise answer contradicts its stated condition.")]}).to_string(),
            ]
            .into(),
        ),
        prompts: std::sync::Mutex::new(vec![]),
    };
    assert!(crate::teaching::review_and_repair(
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
    let check = |has_defect, verdicts: &[serde_json::Value]| {
        review_reply(
            &[],
            vec![
                json!({"index":0,"passageId":"section-0-passage-0","finding":"The example contradicts its return value; correct the returned total to match the accumulation.","hasDefect":has_defect}),
            ],
            verdicts,
        )
    };
    let model = ScriptedModel {
        outputs: std::sync::Mutex::new(
            vec![check(true, &[]), json!({"edits":[{"path":"/blocks/0/body","before":body,"after":format!("{body}Return the accumulated total.")}]}).to_string(), check(false, &[verdict("reviewer/1", "resolved")])].into(),
        ),
        prompts: std::sync::Mutex::new(vec![]),
    };
    crate::teaching::review_and_repair(
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
        json!({"issues":[],"blockChecks":crate::teaching_review::fixture_checks(vec![])}),
        json!({"issues":[],"blockChecks":crate::teaching_review::fixture_checks(vec![json!({"index":0,"passageId":"section-0-invented-passage","finding":"This unsupported review cannot establish that the passage was inspected.","hasDefect":false})])}),
    ] {
        let model = ScriptedModel {
            outputs: std::sync::Mutex::new(vec![review.to_string()].into()),
            prompts: std::sync::Mutex::new(vec![]),
        };
        assert!(crate::teaching::review_and_repair(
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
    let candidate = json!({"questions":[{"kind":"quiz","prompt":"What is two plus two?","options":["Three","Four","Five","Six"],"correctIndex":0,"explanation":"AUTHOR_KEY_SECRET","quote":""}]});
    let mut repaired = candidate.clone();
    repaired["questions"][0]["correctIndex"] = json!(1);
    repaired["questions"][0]["explanation"] = json!("Two plus two is four.");
    let solved = json!({"answers":[{"index":0,"reason":"Adding two pairs produces four items.","correctIndices":[1],"answer":"","blankAnswers":[],"matches":[],"orderedValues":[]}]}).to_string();
    let judged =
        json!({"checks":[{"id":"new-0","finding":"","hasDefect":false,"repeats":[]}]}).to_string();
    let model = ScriptedModel {
        outputs: std::sync::Mutex::new(
            vec![
                solved.clone(),
                judged.clone(),
                json!({"issues":[]}).to_string(),
                json!({"edits":[{"path":"/questions/0/correctIndex","before":0,"after":1},{"path":"/questions/0/explanation","before":"AUTHOR_KEY_SECRET","after":"Two plus two is four."}]}).to_string(),
                solved,
                judged,
                json!({"issues":[]}).to_string(),
            ]
            .into(),
        ),
        prompts: std::sync::Mutex::new(vec![]),
    };
    let actual = crate::teaching::review_and_repair(
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
        assert_eq!(prompts.len(), 7);
        assert!(!prompts[0].contains("AUTHOR_KEY_SECRET"));
        assert!(!prompts[0].contains("\"correctIndex\":"));
        assert!(prompts[3].contains("An independent solution answered options [1]"));
        assert!(prompts[4].starts_with("Solve quiz questions independently"));
    }
    let incomplete = ScriptedModel {
        outputs: std::sync::Mutex::new(vec![json!({"answers":[]}).to_string()].into()),
        prompts: std::sync::Mutex::new(vec![]),
    };
    assert!(crate::teaching::review_and_repair(
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
    use crate::{
        curriculum_repository::LearningCurriculumRepository, diagnostic_generation::DiagnosticTask,
        plan_dto::*,
    };
    let pool = crate::tests::pool().await?;
    let db = Database::new(pool.clone());
    let repository = LearningRepository::new(db.clone());
    let drafts = crate::outline_draft::DraftRunRegistry::new();
    let draft = service::generate(
        &repository,
        &drafts,
        &outline(2, 2),
        request(LearningCourseDepth::Focused),
        vec![],
    )
    .await?;
    let program = service::accept(
        &repository,
        &drafts,
        AcceptLearningProgramRequestDto {
            program_id: draft.summary.id.clone(),
            expected_revision: draft.summary.revision,
            title: "Experiment design".into(),
        },
    )
    .await?;
    let repo = LearningCurriculumRepository::new(db.clone());
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
        crate::diagnostic_generation::validate_findings(&saved, &[response], &[invented]).is_err()
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
    let repo = LearningRepository::new(Database::new(crate::tests::pool().await?));
    let mut grounded = request(LearningCourseDepth::Course);
    grounded
        .source_urls
        .push("https://example.com/reference".into());
    let drafts = crate::outline_draft::DraftRunRegistry::new();
    assert!(
        service::generate(&repo, &drafts, &outline(4, 3), grounded, vec![])
            .await
            .is_err()
    );
    assert!(repo.list().await?.is_empty());
    Ok(())
}

/// A lesson off its shape, or with a citation that does not match, now
/// publishes with findings waiting for the user (`generation/lesson_tests.rs`,
/// `teaching/loop_tests.rs`). What cannot be saved at all is still refused
/// before persistence, as the model's answer, never as the user's input.
#[tokio::test]
async fn a_lesson_that_cannot_be_saved_is_refused_before_persistence() -> Result<()> {
    let mut program = crate::tests::fixture();
    program.sources.clear();
    let empty = crate::dto::PreparedLearningLesson {
        blocks: vec![],
        questions: vec![],
        keys: vec![],
        verification: None,
    };
    let error = service::validate_generated(&empty, &program).err();
    assert!(
        matches!(&error, Some(AppError::ModelAnswerUnusable(_))),
        "{error:?}"
    );
    assert!(!error
        .map(|error| error.to_string())
        .unwrap_or_default()
        .contains("check your input"));
    Ok(())
}

#[tokio::test]
async fn a_written_quiz_item_is_authored_from_lesson_text_without_sources() -> Result<()> {
    use crate::assessment_engine::{LearningBlueprintRequirement, LearningItemFormat};
    use crate::assessment_generation::{author_items, AuthoringNeed, MaterialPassage};
    let passage = "A randomized comparison assigns units to conditions by chance, so differences in outcome can be attributed to the condition.";
    let model = Model(json!({"items":[{"prompt":"Explain how you would design a randomized comparison and identify its limitations.","rubric":task_rubric(),"explanation":"The answer should define the comparison, the measurement, the assumptions and the limits.","blockIndex":0,"quote":"assigns units to conditions by chance","difficulty":3}]}).to_string());
    let need = AuthoringNeed {
        requirement: LearningBlueprintRequirement {
            outcome_id: uuid::Uuid::new_v4().to_string(),
            format: LearningItemFormat::Explanation,
            count: 1,
            difficulty_min: 3,
            difficulty_max: 4,
        },
        outcome_title: "Design a fair comparison".into(),
        outcome_description: "Explain how randomization supports a causal comparison.".into(),
        lesson_id: None,
        count: 1,
        material: vec![MaterialPassage {
            lesson_title: "Experiments".into(),
            title: "Randomization".into(),
            text: passage.into(),
            source_version_ids: vec![],
        }],
        existing_prompts: vec![],
    };
    let authored = author_items(&model, &need).await?;
    assert_eq!(authored.items.len(), 1, "{:?}", authored.dropped);
    assert!(authored.items[0].candidate.source_version_ids.is_empty());
    assert_eq!(
        authored.items[0].source_quote,
        "assigns units to conditions by chance"
    );
    Ok(())
}

#[tokio::test]
async fn outline_model_streams_without_a_deadline_and_rejects_truncated_output() -> Result<()> {
    use lattice_core::application::ports::llm_port::{CompletionRequest, CompletionResponse};
    use lattice_core::application::services::model_contract::{
        Contract, ContractCall, ContractId, PromptInput,
    };
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
    struct StreamedOutline;
    impl Contract for StreamedOutline {
        const ID: ContractId = ContractId::new("streamed-outline");
        type Input = PromptInput;
    }
    let call = ContractCall::of::<StreamedOutline>(&PromptInput {
        system: "Generate".into(),
        prompt: "An outline".into(),
    })?;
    let progress = crate::outline_progress::OutlineProgress::default();
    progress.stage(crate::outline_progress::OutlineStage::Drafting);
    let error = crate::lessons::structured_call::complete_json_with_progress(
        &StreamingModel,
        &call,
        "Generate",
        "An outline".into(),
        json!({"type":"object"}),
        4000,
        Some(&progress),
    )
    .await
    .unwrap_err();
    assert!(error.to_string().contains("cut off"));
    // The cut-off answer streamed once. Under outline progress it already had
    // the whole window, so twice the reservation would give it no more room
    // and the same answer: the retry is skipped.
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
        request: &lattice_core::application::ports::llm_port::CompletionRequest,
    ) -> Result<lattice_core::application::ports::llm_port::CompletionResponse> {
        use lattice_core::application::ports::llm_port::{CompletionInput, CompletionResponse};
        // A prose step keeps its reservation rather than taking the window;
        // the delays and scripted replies belong to the structured answers.
        if crate::lessons::structured_call::is_draft_request(request) {
            return Ok(CompletionResponse::from_text(
                crate::lessons::structured_call::FIXTURE_DRAFT,
            ));
        }
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
        // The answer takes the window the prompt leaves, less only the
        // planner's safety margin, and never runs past the window.
        let max_output = request.max_output_tokens.unwrap() as usize;
        assert!(max_output + input_tokens <= self.max_context_tokens());
        assert!(max_output + input_tokens >= self.max_context_tokens() * 9 / 10);
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
    let db = Database::new(crate::tests::pool().await?);
    let repo = LearningRepository::new(db);
    let draft = outline(4, 3).0;
    let mut corrected: serde_json::Value = serde_json::from_str(&draft)?;
    corrected["modules"][0]["lessons"][0]["objective"] =
        json!("Compare the randomized groups using an observable measurement.");
    let model = DelayedOutlineModel {
        inner: ScriptedModel {
            outputs: std::sync::Mutex::new(
                vec![
                    draft.clone(),
                    json!({"newFindings":[{"path":"/modules/0","kind":"plan","claim":"First module objective","reason":"Clarify the observable first module objective."}]}).to_string(),
                    repair_answer(corrected["modules"][0].clone()).to_string(),
                    json!({"verdicts":[{"id":"finding-1","status":"resolved","reason":"The objective now names an observable measurement."}],"newFindings":[]}).to_string(),
                ]
                .into(),
            ),
            prompts: std::sync::Mutex::new(vec![]),
        },
        // Every call exceeds the former 300s learning cap and the default
        // 600s provider cap. Checkpoints between steps must remain safe.
        delays: std::sync::Mutex::new(vec![700, 700, 700, 700].into()),
    };
    let progress = crate::outline_progress::OutlineProgress::default();
    let drafts = crate::outline_draft::DraftRunRegistry::new();
    let program = progress
        .run(service::generate_with_references_and_progress(
            &repo,
            &drafts,
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
        crate::outline_progress::OutlineStage::Completed
    );
    let prompts = model.inner.prompts.lock().unwrap();
    assert_eq!(prompts.len(), 4);
    assert!(prompts[3].contains("Clarify the observable first module objective."));
    assert!(prompts[3].contains("openFindings"));
    assert!(!prompts[3].contains("previousFindings"));
    assert_eq!(
        program.outline_review.as_ref().unwrap().status,
        crate::outline_draft::LearningOutlineReviewStatus::Passed
    );
    assert!(!prompts[1].contains("inspect EVERY teaching block"));
    assert!(!prompts[3].contains("inspect EVERY teaching block"));
    Ok(())
}

#[tokio::test]
async fn outline_citations_are_optional_and_unmatched_ones_are_stripped_not_reported() -> Result<()>
{
    let quote = "A randomized comparison isolates the intervention.";
    let mut candidate: serde_json::Value = serde_json::from_str(&outline(4, 3).0)?;
    // One citation quotes the capture exactly; one does not. Neither is a finding.
    candidate["modules"][0]["outcomes"][0]["sourceIndex"] = json!(0);
    candidate["modules"][0]["outcomes"][0]["quote"] = json!(quote);
    candidate["modules"][0]["summary"]["sourceIndex"] = json!(0);
    candidate["modules"][0]["summary"]["quote"] =
        json!("A randomized comparison does not isolate the intervention.");
    let url = "https://example.com/experimental-design";
    let captured = crate::source_library::CapturedLearningSource {
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
        signals: crate::references::trust::capture::not_extracted("test fixture"),
    };
    let id = uuid::Uuid::new_v4().to_string();
    let preview = crate::sources::preview(&id, &captured);
    assert!(!preview.excerpt.contains(quote));
    let reference = crate::sources::InitialReference {
        source_id: id,
        origin: url.into(),
        captured,
        found: false,
    };
    let repo = LearningRepository::new(Database::new(crate::tests::pool().await?));
    // Linking keeps the author's exact citation (offered as a candidate) and
    // links nothing else.
    let model = ScriptedModel {
        outputs: std::sync::Mutex::new(
            vec![
                candidate.to_string(),
                json!({"@links":{"/modules/0/outcomes/0":quote}}).to_string(),
                json!({"newFindings":[]}).to_string(),
            ]
            .into(),
        ),
        prompts: std::sync::Mutex::new(vec![]),
    };
    let mut request = request(LearningCourseDepth::Course);
    request.goal = "Learn how a randomized comparison isolates an intervention".into();
    let drafts = crate::outline_draft::DraftRunRegistry::new();
    let program = service::generate_with_references(
        &repo,
        &drafts,
        &model,
        request,
        vec![preview.clone()],
        std::slice::from_ref(&reference),
    )
    .await?;
    let all = model.prompts.lock().unwrap().clone();
    let (linking, prompts): (Vec<&String>, Vec<&String>) = all
        .iter()
        .partition(|p| p.starts_with(crate::outline_linking::LINK_SYSTEM));
    // Authoring retrieves from the complete capture, not the 2,400-char preview.
    assert!(prompts[0].contains(quote));
    assert_eq!(prompts.len(), 2);
    // Only the exact-match citation is offered to the chooser; the inexact
    // one was removed by code and never becomes a passage.
    assert!(!linking.is_empty());
    assert!(linking.iter().any(|p| p.contains(quote)));
    assert!(!linking.iter().any(|p| p.contains("does not isolate")));
    // The reviewer sees no citation fields on items and no quote findings.
    let review_context: serde_json::Value = fixture_task(prompts[1]).expect("a JSON task");
    assert!(review_context["candidate"]["modules"][0]["summary"]
        .get("sourceIndex")
        .is_none());
    assert!(review_context["candidate"]["modules"][0]["outcomes"][0]
        .get("quote")
        .is_none());
    assert!(!prompts[1].contains("could not be matched"));
    drop(all);
    let review = program.outline_review.as_ref().unwrap();
    assert_eq!(
        review.status,
        crate::outline_draft::LearningOutlineReviewStatus::Passed
    );
    assert!(review.issues.is_empty());
    assert_eq!(review.citations_verified, 1);
    assert_eq!(review.citations_removed, 1);
    assert_eq!(program.summary.lesson_count, 12);
    assert!(!program.sources[0].excerpt.contains(quote));
    assert!(repo.verification_sources(&program.summary.id).await?[0]
        .excerpt
        .contains(quote));
    let saved = repo.outline_draft(&program.summary.id).await?.unwrap();
    assert_eq!(
        saved.candidate["modules"][0]["summary"]["sourceIndex"],
        serde_json::Value::Null
    );
    assert_eq!(saved.candidate["modules"][0]["summary"]["quote"], "");
    assert_eq!(saved.candidate["modules"][0]["outcomes"][0]["quote"], quote);
    assert_eq!(
        saved
            .citation_receipts
            .values()
            .filter(|r| r.covers)
            .count(),
        1
    );
    assert!(review
        .note
        .contains("1 of 24 outline items linked to references"));
    service::accept(
        &repo,
        &drafts,
        AcceptLearningProgramRequestDto {
            program_id: program.summary.id.clone(),
            expected_revision: program.summary.revision,
            title: program.summary.title.clone(),
        },
    )
    .await?;
    Ok(())
}

#[tokio::test]
async fn outline_repair_preserves_completed_module_edits_when_later_call_fails() -> Result<()> {
    use crate::outline_draft::LearningOutlineReviewStatus;
    let pool = crate::tests::pool().await?;
    let db = Database::new(pool.clone());
    let repo = LearningRepository::new(db.clone());
    let candidate: serde_json::Value = serde_json::from_str(&outline(4, 3).0)?;
    let issues = json!({"newFindings":[
        {"path":"/modules/0","kind":"plan","claim":"Comparison objective","reason":"Specify the measurement used in the comparison."},
        {"path":"/modules/1","kind":"plan","claim":"Analysis objective","reason":"Distinguish observation from causal interpretation."}
    ]});
    let mut module = candidate["modules"][0].clone();
    module["lessons"][0]["objective"] =
        json!("Compare the randomized groups using an observable measurement.");
    let model = ScriptedModel {
        outputs: std::sync::Mutex::new(
            vec![
                candidate.to_string(),
                issues.to_string(),
                repair_answer(module.clone()).to_string(),
            ]
            .into(),
        ),
        prompts: std::sync::Mutex::new(vec![]),
    };
    // The fourth request fails. A previous module's correction must survive.
    let drafts = crate::outline_draft::DraftRunRegistry::new();
    let saved = service::generate(
        &repo,
        &drafts,
        &model,
        request(LearningCourseDepth::Course),
        vec![],
    )
    .await?;
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
    let reopened = LearningRepository::new(db.clone())
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
    use crate::outline_progress::{OutlineProgress, OutlineRunRegistry};
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

    let pool = crate::tests::pool().await?;
    let db = Database::new(pool.clone());
    let repo = LearningRepository::new(db.clone());
    let entered = std::sync::Arc::new(tokio::sync::Notify::new());
    let model = WaitingModel {
        draft: outline(4, 3).0,
        entered: entered.clone(),
    };
    let id = uuid::Uuid::new_v4().to_string();
    let registry = OutlineRunRegistry::new();
    let run = registry.register(Some(id.clone()), |_| {})?;
    let task_repo = repo.clone();
    let drafts = crate::outline_draft::DraftRunRegistry::new();
    let task_drafts = drafts.clone();
    let task = tokio::spawn(async move {
        run.progress
            .run(service::generate_with_references_and_progress(
                &task_repo,
                &task_drafts,
                &model,
                request(LearningCourseDepth::Course),
                vec![],
                &[],
                &run.progress,
                None,
            ))
            .await
    });
    tokio::time::timeout(std::time::Duration::from_secs(10), entered.notified())
        .await
        .unwrap();
    let saved = repo.list().await?.pop().unwrap();
    assert!(registry.cancel(&id));
    assert!(task.await.unwrap().is_err());
    let reopened = LearningRepository::new(db.clone());
    let saved = reopened.get(&saved.id).await?;
    assert_eq!(
        saved.outline_review.as_ref().unwrap().status,
        crate::outline_draft::LearningOutlineReviewStatus::Unchecked
    );
    let model = ScriptedModel {
        outputs: std::sync::Mutex::new(vec![json!({"newFindings":[]}).to_string()].into()),
        prompts: std::sync::Mutex::new(vec![]),
    };
    let resumed = crate::outline_draft::repair_saved(
        &reopened,
        &drafts,
        &model,
        &crate::outline_draft::RepairLearningOutlineRequestDto {
            program_id: saved.summary.id,
            expected_revision: saved.summary.revision,
        },
        &OutlineProgress::default(),
        None,
    )
    .await?;
    assert_eq!(
        resumed.outline_review.as_ref().unwrap().status,
        crate::outline_draft::LearningOutlineReviewStatus::Passed
    );
    assert_eq!(model.prompts.lock().unwrap().len(), 1);
    Ok(())
}

#[tokio::test]
async fn outline_automatic_repair_is_bounded_by_attempts_and_repair_starts_them_afresh(
) -> Result<()> {
    use crate::outline_draft::LearningOutlineReviewStatus;
    let repo = LearningRepository::new(Database::new(crate::tests::pool().await?));
    let candidate: serde_json::Value = serde_json::from_str(&outline(4, 3).0)?;
    let raised = json!({"newFindings":[{"path":"/modules/0/lessons/0","kind":"plan","claim":"Design a controlled experiment and explain its limitations.","reason":"Specify the measurement used in the comparison."}]});
    let unresolved = |reason: &str| json!({"verdicts":[{"id":"finding-1","status":"unresolved","reason":reason}],"newFindings":[]});
    let resolved = json!({"verdicts":[{"id":"finding-1","status":"resolved","reason":"The objective now names the measurement."}],"newFindings":[]});
    let mut first = candidate["modules"][0].clone();
    first["lessons"][0]["objective"] =
        json!("Measure the effect of random assignment on the group comparison.");
    let mut second = first.clone();
    second["lessons"][0]["objective"] =
        json!("Measure the group difference with a stated outcome.");
    let mut third = second.clone();
    third["lessons"][0]["objective"] =
        json!("Measure the group difference with a stated, observable outcome.");
    // Two unresolved verdicts spend the finding's two attempts; the loop then
    // stops with the finding exhausted instead of rewriting a third time.
    let model = ScriptedModel {
        outputs: std::sync::Mutex::new(
            vec![
                candidate.to_string(),
                raised.to_string(),
                repair_answer(first.clone()).to_string(),
                unresolved("The objective still does not name an observable measurement.")
                    .to_string(),
                repair_answer(second.clone()).to_string(),
                unresolved("The outcome is named but still not observable.").to_string(),
            ]
            .into(),
        ),
        prompts: std::sync::Mutex::new(vec![]),
    };
    let drafts = crate::outline_draft::DraftRunRegistry::new();
    let saved = service::generate(
        &repo,
        &drafts,
        &model,
        request(LearningCourseDepth::Course),
        vec![],
    )
    .await?;
    let review = saved.outline_review.as_ref().unwrap();
    // A plan finding automatic repair could not settle is advice, not a block.
    assert_eq!(review.status, LearningOutlineReviewStatus::Passed);
    assert_eq!(review.repair_passes, 2);
    assert_eq!(review.issues.len(), 1);
    assert_eq!(review.issues[0].id, "finding-1");
    assert_eq!(review.issues[0].attempts, 2);
    assert!(review.issues[0].exhausted);
    assert_eq!(
        review.issues[0].claim,
        "Measure the group difference with a stated outcome."
    );
    assert!(review.note.starts_with("Outline checks passed."));
    assert!(review
        .note
        .contains("1 plan suggestion could not be settled automatically"));
    assert!(review
        .note
        .contains("Module 1 · Lesson 1: The outcome is named but still not observable."));
    assert!(review.note.contains("use Repair to try again"));
    let prompts = model.prompts.lock().unwrap().clone();
    assert_eq!(prompts.len(), 6);
    // The second rewrite carries the last-attempt instruction; the second
    // review was told how many attempts the finding had used.
    assert!(prompts[4].contains("This is the last attempt"));
    assert!(!prompts[2].contains("This is the last attempt"));
    let second_review: serde_json::Value = fixture_task(&prompts[3]).expect("a JSON task");
    assert_eq!(second_review["openFindings"][0]["attempts"], 1);
    assert_eq!(
        second_review["openFindings"][0]["claim"],
        "Measure the effect of random assignment on the group comparison."
    );
    drop(prompts);
    // Repair starts the attempts afresh: one more rewrite, then the verdict.
    let model = ScriptedModel {
        outputs: std::sync::Mutex::new(
            vec![
                unresolved("Still not observable.").to_string(),
                repair_answer(third.clone()).to_string(),
                resolved.to_string(),
            ]
            .into(),
        ),
        prompts: std::sync::Mutex::new(vec![]),
    };
    let repaired = crate::outline_draft::repair_saved(
        &repo,
        &drafts,
        &model,
        &crate::outline_draft::RepairLearningOutlineRequestDto {
            program_id: saved.summary.id.clone(),
            expected_revision: saved.summary.revision,
        },
        &crate::outline_progress::OutlineProgress::default(),
        None,
    )
    .await?;
    let review = repaired.outline_review.as_ref().unwrap();
    assert_eq!(review.status, LearningOutlineReviewStatus::Passed);
    assert!(review.issues.is_empty());
    assert_eq!(review.repair_passes, 3);
    let prompts = model.prompts.lock().unwrap();
    assert_eq!(prompts.len(), 3);
    let reopened: serde_json::Value = fixture_task(&prompts[0]).expect("a JSON task");
    assert_eq!(reopened["openFindings"][0]["attempts"], 0);
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

#[tokio::test]
async fn a_drafts_citation_list_is_gone_before_review_and_verification_read_it() -> Result<()> {
    use crate::{
        curriculum::LearningGenerationJobKind, curriculum_repository::LearningCurriculumRepository,
        plan_dto::*,
    };
    use std::sync::Mutex;
    struct Recording {
        inner: Model,
        drafted: Mutex<bool>,
        later: Mutex<Vec<String>>,
    }
    #[async_trait::async_trait]
    impl LLMPort for Recording {
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
    impl Recording {
        async fn respond(&self, prompt: &str) -> Result<String> {
            {
                let mut drafted = self
                    .drafted
                    .lock()
                    .map_err(|_| AppError::InternalError("fixture lock".into()))?;
                if *drafted {
                    self.later
                        .lock()
                        .map_err(|_| AppError::InternalError("fixture lock".into()))?
                        .push(prompt.into());
                }
                if prompt.starts_with("Teach one rigorous, accessible lesson") {
                    *drafted = true;
                }
            }
            self.inner.respond(prompt).await
        }
    }

    let pool = crate::tests::pool().await?;
    let db = Database::new(pool.clone());
    let repo = LearningRepository::new(db.clone());
    let drafts = crate::outline_draft::DraftRunRegistry::new();
    let draft = service::generate(
        &repo,
        &drafts,
        &outline(4, 3),
        request(LearningCourseDepth::Course),
        vec![],
    )
    .await?;
    let accepted = service::accept(
        &repo,
        &drafts,
        AcceptLearningProgramRequestDto {
            program_id: draft.summary.id.clone(),
            expected_revision: draft.summary.revision,
            title: "Experimental design".into(),
        },
    )
    .await?;
    let quote = "A randomized comparison isolates the intervention.";
    crate::content_verification::tests::install_source(&db, &draft.summary.id, REFERENCE).await?;
    let mut program = repo.get(&draft.summary.id).await?;
    program.sources = repo.verification_sources(&draft.summary.id).await?;
    let lesson = &program.modules[0].lessons[0];
    let mut candidate = lesson_response();
    for item in candidate["blocks"].as_array_mut().unwrap() {
        let body = item["body"].as_str().unwrap_or_default().to_owned();
        item["body"] = json!(format!(
            "{body}\n\n**Citations:**\n*   \"{quote}\" (SourceIndex: 0)\n*   \"A controlled experiment states its limitations.\" (SourceIndex: 0)"
        ));
    }
    for field in ["blocks", "questions"] {
        for item in candidate[field].as_array_mut().unwrap() {
            item["sourceIndex"] = json!(0);
            item["quote"] = json!(quote);
        }
    }
    let llm = Recording {
        inner: Model(candidate.to_string()),
        drafted: Mutex::new(false),
        later: Mutex::new(Vec::new()),
    };
    let jobs = LearningCurriculumRepository::new(db.clone());
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
    JobStore::new(db.clone()).claim(&job.id).await?;
    let runtimes = crate::embedded_runtime::BuiltinRuntimes::for_tests();
    let prepared = crate::lesson_progress::run(
        &JobContext::detached(&JobStore::new(db.clone()), &job.id).await?,
        crate::lesson_drafts::run(
            &db,
            &jobs,
            &job.id,
            &lesson.id,
            generation::prepare_lesson(
                &llm,
                &runtimes,
                &program,
                lesson,
                &CoursePlan::blank(&program),
            ),
        ),
    )
    .await?;

    assert_eq!(prepared.blocks.len(), LESSON_SHAPE.len());
    for block in &prepared.blocks {
        assert!(!block.body.contains("SourceIndex"), "{}", block.body);
        assert!(!block.body.contains("Citations:"), "{}", block.body);
    }
    let later = llm
        .later
        .lock()
        .map_err(|_| AppError::InternalError("fixture lock".into()))?;
    assert!(!later.is_empty(), "review and verification made no calls");
    let injected = [
        "(SourceIndex:",
        "Citations:",
        "\"A randomized comparison isolates the intervention.\" (SourceIndex",
        "\"A controlled experiment states its limitations.\" (SourceIndex",
    ];
    let leak = later.iter().find_map(|prompt| {
        injected.iter().find_map(|needle| {
            prompt.find(needle).map(|at| {
                let mut from = at.saturating_sub(40);
                while !prompt.is_char_boundary(from) {
                    from -= 1;
                }
                let mut to = (at + 40).min(prompt.len());
                while !prompt.is_char_boundary(to) {
                    to += 1;
                }
                let head: String = prompt.chars().take(80).collect();
                format!("{head:?} holds {:?}", &prompt[from..to])
            })
        })
    });
    assert!(
        leak.is_none(),
        "a prompt after the draft still holds the citation apparatus: {}",
        leak.unwrap_or_default()
    );
    Ok(())
}
