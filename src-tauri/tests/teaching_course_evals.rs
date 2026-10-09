//! Opt-in evaluation through an explicitly selected OpenCode provider config.
//! Credentials are read directly into sensitive headers and never logged.
//! LATTICE_TEACHING_CONFIG_PATH=/path/to/opencode.json
//! LATTICE_TEACHING_EVAL_DIR=/path/to/artifacts
//! cargo test --test teaching_course_evals -- --ignored --nocapture --test-threads=1 --skip live_retained_lesson_review
#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::print_stdout,
    clippy::indexing_slicing
)]

use async_trait::async_trait;
use lattice::{
    application::ports::{
        llm_port::{CompletionInput, CompletionRequest, CompletionResponse},
        LLMPort,
    },
    features::learning::{dto::*, practice_generation, repository::LearningRepository, service},
    shared::error::{AppError, Result},
};
use serde_json::{json, Value};
use std::{
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
    time::Instant,
};

struct ConfiguredModel {
    inner: lattice::features::llm::llama_cpp::LlamaCppLlm,
    model: String,
    context: usize,
    output: u32,
    directory: PathBuf,
    calls: AtomicUsize,
}
impl ConfiguredModel {
    fn open(suite: &str) -> Self {
        let path = std::env::var("LATTICE_TEACHING_CONFIG_PATH")
            .expect("Explicit provider config path required");
        let config: Value =
            serde_json::from_slice(&std::fs::read(path).expect("Read provider config"))
                .expect("Provider JSON");
        let model = config["model"].as_str().expect("Default model");
        let (provider, model) = model.split_once('/').expect("Provider/model");
        let p = &config["provider"][provider];
        let mut headers = reqwest::header::HeaderMap::new();
        for (name, value) in p["options"]["headers"].as_object().into_iter().flatten() {
            let value = value.as_str().expect("String header");
            let resolved = if value.starts_with("{env:") && value.ends_with('}') {
                std::env::var(&value[5..value.len() - 1])
                    .expect("Configured header environment variable")
            } else if value.starts_with("{file:") && value.ends_with('}') {
                std::fs::read_to_string(&value[6..value.len() - 1])
                    .expect("Configured header file")
                    .trim()
                    .into()
            } else {
                value.into()
            };
            let mut header =
                reqwest::header::HeaderValue::from_str(&resolved).expect("Valid configured header");
            header.set_sensitive(true);
            headers.insert(
                reqwest::header::HeaderName::from_bytes(name.as_bytes()).expect("Header name"),
                header,
            );
        }
        let directory = PathBuf::from(
            std::env::var("LATTICE_TEACHING_EVAL_DIR")
                .expect("Explicit artifact directory required"),
        )
        .join(suite);
        std::fs::create_dir_all(&directory).expect("Create evaluation artifacts");
        let last_call = std::fs::read_dir(&directory)
            .expect("Read evaluation directory")
            .filter_map(|entry| entry.ok())
            .filter_map(|entry| {
                entry.file_name().to_str().and_then(|name| {
                    name.strip_prefix("call-")?
                        .strip_suffix(".json")?
                        .parse::<usize>()
                        .ok()
                })
            })
            .max()
            .unwrap_or(0);
        assert!(
            headers.len() <= 1,
            "The production adapter supports one configured authentication header"
        );
        let (auth_header_name, auth_header_value) = headers
            .iter()
            .next()
            .map(|(name, value)| {
                (
                    name.as_str().to_owned(),
                    value.to_str().expect("Header text").to_owned(),
                )
            })
            .unwrap_or_default();
        let output = p["models"][model]["limit"]["output"]
            .as_u64()
            .unwrap_or(8192) as u32;
        let context = p["models"][model]["limit"]["context"]
            .as_u64()
            .unwrap_or(32768) as usize;
        let settings = lattice::application::contracts::settings::LLMSettingsDto {
            model: model.into(),
            max_tokens: output,
            context_window: context as u32,
            temperature: 0.2,
            llama_cpp: lattice::application::contracts::settings::LlamaCppSettingsDto {
                url: p["options"]["baseURL"].as_str().expect("Endpoint").into(),
                model: model.into(),
                auth_header_name,
                auth_header_value,
            },
            ..Default::default()
        };
        Self {
            inner: lattice::features::llm::llama_cpp::LlamaCppLlm::new(&settings)
                .expect("Configured production adapter"),
            model: model.into(),
            context,
            output,
            directory,
            calls: AtomicUsize::new(last_call),
        }
    }

    fn save(&self, name: &str, value: &impl serde::Serialize) {
        std::fs::write(
            self.directory.join(name),
            serde_json::to_vec_pretty(value).expect("Serialize evaluation artifact"),
        )
        .expect("Save evaluation artifact");
    }
}
#[async_trait]
impl LLMPort for ConfiguredModel {
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        let messages: Vec<_> = request
            .input
            .iter()
            .filter_map(|i| match i {
                CompletionInput::Message { role, content } => {
                    Some(json!({"role":role,"content":content}))
                }
                _ => None,
            })
            .collect();
        if std::env::var("LATTICE_TEACHING_REPLAY").as_deref() == Ok("1") {
            let mut paths: Vec<_> = std::fs::read_dir(&self.directory)
                .expect("Read retained calls")
                .filter_map(|entry| entry.ok().map(|entry| entry.path()))
                .filter(|path| {
                    path.file_name()
                        .and_then(|name| name.to_str())
                        .is_some_and(|name| name.starts_with("call-") && name.ends_with(".json"))
                })
                .collect();
            paths.sort();
            for path in paths {
                let saved: Value =
                    serde_json::from_slice(&std::fs::read(&path).expect("Read retained call"))
                        .expect("Retained call JSON");
                if saved["transport"] == "production llama.cpp streaming adapter"
                    && saved["model"] == self.model
                    && saved["input"] == json!(messages)
                    && saved["reasoningEffort"] == json!(request.reasoning_effort)
                    && saved["maxOutputTokens"] == request.effective_max_output_tokens(self.output)
                    && saved["finishReason"] == "stop"
                {
                    println!(
                        "Reused exact recorded request from {}",
                        path.file_name().unwrap().to_string_lossy()
                    );
                    return Ok(CompletionResponse {
                        text: saved["output"].as_str().expect("Retained output").into(),
                        finish_reason: "stop".into(),
                        input_tokens: saved["usage"]["prompt_tokens"].as_u64().unwrap_or(0),
                        output_tokens: saved["usage"]["completion_tokens"].as_u64().unwrap_or(0),
                        ..Default::default()
                    });
                }
            }
        }
        let number = self.calls.fetch_add(1, Ordering::SeqCst) + 1;
        let began = Instant::now();
        println!("Model call {number} started");
        let response = self.inner.complete(request).await?;
        self.save(&format!("call-{number:03}.json"), &json!({"transport":"production llama.cpp streaming adapter","model":self.model,"reasoningEffort":request.reasoning_effort,"maxOutputTokens":request.effective_max_output_tokens(self.output),"latencyMs":began.elapsed().as_millis(),"input":messages,"output":response.text,"finishReason":response.finish_reason,"usage":{"prompt_tokens":response.input_tokens,"completion_tokens":response.output_tokens}}));
        println!(
            "Model call {number} completed in {}s ({})",
            began.elapsed().as_secs(),
            response.finish_reason
        );
        Ok(response)
    }

    fn model_name(&self) -> &str {
        &self.model
    }
    fn max_context_tokens(&self) -> usize {
        self.context
    }
    fn count_tokens(&self, text: &str) -> usize {
        text.chars().count().div_ceil(3)
    }
    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
}

#[tokio::test]
#[ignore = "Uses an explicitly configured provider and generates a complete synthetic course"]
async fn live_complete_python_course() -> Result<()> {
    let model = ConfiguredModel::open("course");
    let database = model.directory.join("course.sqlite");
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            sqlx::sqlite::SqliteConnectOptions::new()
                .filename(database)
                .create_if_missing(true)
                .foreign_keys(true),
        )
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;
    let repo = LearningRepository::new(pool.clone());
    let mut program = if std::env::var("LATTICE_TEACHING_RESUME").as_deref() == Ok("1")
        && model.directory.join("outline.json").exists()
    {
        let draft: LearningProgramDto = serde_json::from_slice(
            &std::fs::read(model.directory.join("outline.json")).expect("Read retained outline"),
        )
        .expect("Retained outline");
        repo.get(&draft.summary.id).await?
    } else {
        let draft=service::generate(&repo,&model,GenerateLearningProgramRequestDto { goal:"Build a small Python expense summarizer from supplied CSV text using the standard library. Use a single analyzer.py file and a provided three-line runner. Practice dictionaries to total amounts by category, validate empty category names, and write assert checks. Limit the CSV to category and integer cents; quoted fields use csv.DictReader. No file I/O, CLI argument parsing, classes, or external packages.".into(), prior_knowledge:"I can write small functions, iterate lists with for loops, use conditionals, and do basic arithmetic. I need practice using dictionaries, reading CSV text with the standard library, and writing assert checks.".into(),minutes_per_session:30,course_depth:Some(LearningCourseDepth::Focused),document_ids:vec![],source_urls:vec![] },vec![]).await?;
        model.save("outline.json", &draft);
        service::accept(
            &repo,
            AcceptLearningProgramRequestDto {
                program_id: draft.summary.id.clone(),
                expected_revision: draft.summary.revision,
                title: "Python expense analyzer".into(),
            },
        )
        .await?
    };
    model.save("course.json", &program);
    let lesson_ids: Vec<_> = program
        .modules
        .iter()
        .flat_map(|m| m.lessons.iter().map(|l| l.id.clone()))
        .collect();
    for (index, lesson_id) in lesson_ids.iter().enumerate() {
        if program
            .modules
            .iter()
            .flat_map(|m| &m.lessons)
            .any(|lesson| {
                lesson.id == *lesson_id && lesson.preparation == LearningPreparation::Ready
            })
        {
            continue;
        }
        println!("Preparing lesson {}/{}", index + 1, lesson_ids.len());
        program = service::prepare(
            &repo,
            &model,
            PrepareLearningLessonRequestDto {
                program_id: program.summary.id.clone(),
                lesson_id: lesson_id.clone(),
                expected_revision: program.summary.revision,
            },
        )
        .await?;
        model.save("course.json", &program);
    }
    let assessment =
        lattice::features::learning::assessment_repository::LearningAssessmentRepository::new(
            pool.clone(),
        )
        .workspace(&program.summary.id)
        .await?;
    let tasks = lattice::features::learning::diagnostic_generation::author(
        &model,
        &program,
        &assessment.outcomes,
    )
    .await?;
    model.save("diagnostic-authoring-private.json", &tasks);
    assert!(program.modules.iter().all(|m| m.project.is_some()
        && m.lessons
            .iter()
            .all(|l| l.preparation == LearningPreparation::Ready)));
    let blocks = program
        .modules
        .iter()
        .flat_map(|m| &m.lessons)
        .flat_map(|l| &l.blocks)
        .count();
    model.save("course-result.json",&json!({"model":model.model_name(),"lessons":lesson_ids.len(),"teachingSections":blocks,"allLessonsPrepared":true,"diagnosticTasks":tasks.len(),"calls":model.calls.load(Ordering::SeqCst),"expertReview":"not measured","learnerOutcomes":"not measured"}));
    pool.close().await;
    Ok(())
}

fn sample_session() -> LearningPracticeSessionDto {
    serde_json::from_value(json!({"summary":{"id":"00000000-0000-4000-8000-000000000001","lessonId":"00000000-0000-4000-8000-000000000002","lessonTitle":"Choose the right accumulator","status":"active","mode":"practice","revision":0,"artifactRevision":0,"sourceVersionIds":[],"createdAt":0,"updatedAt":0,"submittedAt":null,"gradeStatus":null,"taskKind":"guided"},"taskPrompt":"Write a Python function total(values) that returns the sum of all nonnegative numbers, including zero, and ignores negative numbers. Explain your loop and give tests for a mixed list and an empty list. Do not use sum().","lessonObjective":"Accumulate a filtered total and check edge cases.","rubric":[{"id":"filter","dimension":"application","title":"Filter and accumulate","description":"Award 4 for an accumulator initialized to zero, adding every value greater than or equal to zero and returning the total. Award 2 for a correct filter with a broken accumulation or return. Award 0 when the computation contradicts the task. Use null when there is no assessable attempt.","maxPoints":4},{"id":"tests","dimension":"explanation","title":"Explain and test the edge cases","description":"Award 4 for correct mixed-list and empty-list assertions and an explanation of why the initial zero handles the empty list. Award 2 for only one correct test or an explanation without both tests. Award 0 for incorrect expected results. Use null when no reasoning or tests are offered.","maxPoints":4}],"artifact":{"revision":0,"text":"","sha256":"fixture","updatedAt":0},"assistance":[],"tutorTurns":[],"proposals":[],"revealedSolution":null,"revealedSolutionCitations":[],"result":null})).expect("Synthetic session")
}

#[tokio::test]
#[ignore = "Grades synthetic correct, incorrect, partial and uncertain work through the configured provider"]
async fn live_feedback_and_hint_matrix() -> Result<()> {
    let model = ConfiguredModel::open("feedback");
    let cases=[("correct","def total(values):\n    result = 0\n    for value in values:\n        if value >= 0:\n            result += value\n    return result\nassert total([-3, 0, 2, 4]) == 6\nassert total([]) == 0\nThe initial zero remains unchanged for an empty list, so its total is zero."),("incorrect","def total(values):\n    result = 1\n    for value in values:\n        if value < 0:\n            result += value\n    return result\nassert total([-3, 0, 2, 4]) == -2\nassert total([]) == 1\nI add negative numbers and begin at one."),("partial","def total(values):\n    result = 0\n    for value in values:\n        if value >= 0:\n            result += value\n    return result\nassert total([1, -1]) == 1"),("uncertain","I am not sure how to start.")];
    let mut records = Vec::new();
    for (name, answer) in cases {
        let mut session = sample_session();
        session.artifact.text = answer.into();
        let (status, criteria) = practice_generation::grade(&model, &session).await?;
        let total: i64 = criteria.iter().filter_map(|c| c.score).sum();
        let passed = match name {
            "correct" => total >= 7,
            "incorrect" => total <= 2,
            "partial" => (4..8).contains(&total),
            _ => {
                status == LearningPracticeGradeStatus::Uncertain
                    && criteria.iter().all(|c| c.score.is_none())
            }
        };
        records.push(
            json!({"case":name,"passed":passed,"status":status,"total":total,"criteria":criteria}),
        );
    }
    model.save("feedback-matrix.json", &records);
    assert!(
        records.iter().all(|r| r["passed"] == true),
        "Review feedback-matrix.json for incorrect grading"
    );
    let mut session = sample_session();
    session.artifact.text =
        "I set result to zero, but I do not know which condition to use.".into();
    let tutor = practice_generation::tutor(
        &model,
        &session,
        &RequestLearningTutorResponseRequestDto {
            operation_id: uuid::Uuid::new_v4().to_string(),
            program_id: uuid::Uuid::new_v4().to_string(),
            session_id: session.summary.id.clone(),
            expected_revision: 0,
            request_kind: LearningTutorRequestKind::Hint,
            prompt: "Give me an orienting question without solving the exercise.".into(),
            hint_level: Some(LearningPracticeHintLevel::OrientingQuestion),
        },
        &[],
    )
    .await?;
    model.save("guided-hint.json",&json!({"response":tutor.response,"fullFunctionLeaked":tutor.response.contains("def total") && tutor.response.contains("return")}));
    assert!(!(tutor.response.contains("def total") && tutor.response.contains("return")));
    Ok(())
}

#[tokio::test]
#[ignore = "Authors and persists a checkpoint against the previously completed synthetic course"]
async fn live_saved_course_checkpoint() -> Result<()> {
    use lattice::features::learning::{
        assessment_engine::LearningBlueprintRequirement, assessment_generation,
        assessment_repository::LearningAssessmentRepository,
    };
    let model = ConfiguredModel::open("checkpoint");
    let course_dir = model.directory.parent().unwrap().join("course");
    let saved: LearningProgramDto = serde_json::from_slice(
        &std::fs::read(course_dir.join("course.json"))
            .expect("Run complete course evaluation first"),
    )
    .expect("Saved course");
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            sqlx::sqlite::SqliteConnectOptions::new()
                .filename(course_dir.join("course.sqlite"))
                .foreign_keys(true),
        )
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;
    let repository = LearningAssessmentRepository::new(pool.clone());
    let program = LearningRepository::new(pool.clone())
        .get(&saved.summary.id)
        .await?;
    assert!(program
        .modules
        .iter()
        .flat_map(|m| &m.lessons)
        .all(|l| l.preparation == LearningPreparation::Ready));
    let workspace = repository.workspace(&program.summary.id).await?;
    let module = &program.modules[0];
    let outcome = workspace
        .outcomes
        .iter()
        .find(|o| o.module_id.as_deref() == Some(&module.id) && o.lesson_id.is_none())
        .expect("Module outcome");
    let request = CreateLearningAssessmentBlueprintRequestDto {
        operation_id:uuid::Uuid::new_v4().to_string(), program_id:program.summary.id.clone(), blueprint_id:uuid::Uuid::new_v4().to_string(), revision:1, predecessor_revision:None,
        purpose:LearningAssessmentPurpose::ModuleTest,title:format!("{} checkpoint",module.title),instructions:"Explain your reasoning and produce a small artifact applying the learned skills to a new case. Include all input data in the questions.".into(),expected_minutes:25,allowed_aids:vec![],passing_score:0.7,feedback_timing:LearningFeedbackTiming::AfterSubmission,
        rubric:vec![LearningRubricCriterion {id:uuid::Uuid::new_v4().to_string(),title:"Reasoning and application".into(),description:"Apply the learned skill, explain the choices, and check the result.".into(),max_points:4}],source_version_ids:vec![],
        requirements:vec![LearningItemFormat::Explanation,LearningItemFormat::Artifact].into_iter().map(|format|LearningBlueprintRequirement {outcome_id:outcome.id.clone(),format,count:1,difficulty_min:2,difficulty_max:3}).collect(),change_reason:"Live synthetic checkpoint evaluation".into()
    };
    let candidates = assessment_generation::generate(
        &model,
        &request,
        &workspace.outcomes,
        &[],
        &module.lessons,
    )
    .await?;
    model.save("checkpoint-candidates-private.json", &candidates);
    assert!(candidates.iter().all(|c| c.rubric.len() >= 2));
    repository
        .create_blueprint(
            &request,
            &candidates,
            model.model_name(),
            &request.operation_id,
        )
        .await?;
    let form = repository
        .start_form(
            &StartLearningAssessmentFormRequestDto {
                operation_id: uuid::Uuid::new_v4().to_string(),
                form_id: uuid::Uuid::new_v4().to_string(),
                program_id: program.summary.id.clone(),
                blueprint_id: request.blueprint_id,
                blueprint_revision: 1,
                retake_of_form_id: None,
            },
            "live-checkpoint-form",
        )
        .await?;
    model.save("checkpoint-form-public.json", &form);
    assert_eq!(form.items.len(), 2);
    pool.close().await;
    Ok(())
}

#[tokio::test]
#[ignore = "Rechecks a retained synthetic lesson through the production content review"]
async fn live_retained_lesson_review() -> Result<()> {
    let model = ConfiguredModel::open("content-review");
    let path =
        std::env::var("LATTICE_TEACHING_REVIEW_CALL").expect("Explicit retained call required");
    let retained: Value =
        serde_json::from_slice(&std::fs::read(path).expect("Read retained candidate"))?;
    let candidate = retained["output"].as_str().expect("Candidate JSON");
    let raw_context = retained["input"][1]["content"]
        .as_str()
        .expect("Original context");
    let context: Value = serde_json::from_str(raw_context)?;
    let context = context
        .get("originalRequirements")
        .unwrap_or(&context)
        .to_string();
    let repaired = lattice::features::learning::teaching::review_and_repair(
        &model,
        "Teach a rigorous lesson. Return only the corrected lesson JSON, preserving the original block and question fields.",
        &context,
        &json!({"type":"object","required":["blocks","questions"],"properties":{"blocks":{"type":"array","minItems":8,"maxItems":12},"questions":{"type":"array","minItems":6,"maxItems":6}}}),
        candidate.into(),
        8192,
    ).await?;
    let lesson: Value = serde_json::from_str(&repaired)?;
    model.save("reviewed-lesson.json", &lesson);
    let bodies: String = lesson["blocks"]
        .as_array()
        .expect("Lesson sections")
        .iter()
        .filter_map(|block| block["body"].as_str())
        .collect::<Vec<_>>()
        .join("\n");
    // Retained regressions found by tracing the original examples in Python.
    for rejected_claim in [
        "splits it on commas, strips whitespace",
        "iterating lazily (as in the loop above) avoids loading everything",
        "The tutor will run the script",
    ] {
        assert!(
            !bodies.contains(rejected_claim),
            "Known incorrect claim survived: {rejected_claim}"
        );
    }
    Ok(())
}
