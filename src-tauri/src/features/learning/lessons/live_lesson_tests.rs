//! Opt-in full production-worker exercise against an explicitly chosen library.
//! This retries and publishes the chosen job through repositories, so callers
//! must deliberately select the database. No model responses are mocked.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]
use super::*;
use crate::application::ports::llm_port::{CompletionRequest, CompletionResponse};
use crate::features::llm::llama_cpp::LlamaCppLlm;

struct ObservedModel {
    inner: LlamaCppLlm,
    directory: std::path::PathBuf,
    call: std::sync::atomic::AtomicUsize,
}
impl ObservedModel {
    fn record(
        &self,
        call: usize,
        request: &CompletionRequest,
        response: &CompletionResponse,
    ) -> Result<()> {
        std::fs::write(
            self.directory.join(format!("call-{call}.json")),
            serde_json::to_vec_pretty(&serde_json::json!({
                "input":request.input,"answer":response.text,"finishReason":response.finish_reason,
                "inputTokens":response.input_tokens,"outputTokens":response.output_tokens,
                "firstTokenLogprobs":response.first_token_logprobs
            }))?,
        )?;
        println!(
            "Live call {call} completed: {} input / {} output tokens, {}",
            response.input_tokens, response.output_tokens, response.finish_reason
        );
        Ok(())
    }
}
#[async_trait::async_trait]
impl LLMPort for ObservedModel {
    fn supports_typed_completions(&self) -> bool {
        true
    }
    async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        let call = self.call.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let response = self.inner.complete(request).await?;
        self.record(call, request, &response)?;
        Ok(response)
    }
    async fn complete_with_progress(
        &self,
        request: &CompletionRequest,
        on_text: &(dyn Fn(String) -> Result<()> + Send + Sync),
    ) -> Result<CompletionResponse> {
        let call = self.call.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let response = self.inner.complete_with_progress(request, on_text).await?;
        self.record(call, request, &response)?;
        Ok(response)
    }
    async fn generate(&self, _: &str, _: &[String], _: Option<Vec<String>>) -> Result<String> {
        unreachable!()
    }
    async fn complete_with_retry_progress(
        &self,
        request: &CompletionRequest,
        on_text: &(dyn Fn(String) -> Result<()> + Send + Sync),
        on_retry: &(dyn Fn(usize) -> Result<()> + Send + Sync),
    ) -> Result<CompletionResponse> {
        let call = self.call.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let response = self
            .inner
            .complete_with_retry_progress(request, on_text, on_retry)
            .await?;
        self.record(call, request, &response)?;
        Ok(response)
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
        self.inner.model_name()
    }
    fn count_tokens(&self, text: &str) -> usize {
        self.inner.count_tokens(text)
    }
    fn max_context_tokens(&self) -> usize {
        self.inner.max_context_tokens()
    }
    async fn is_ready(&self) -> Result<bool> {
        self.inner.is_ready().await
    }
}

#[tokio::test]
#[ignore = "requires explicit live settings, evidence cases and artifact directory"]
async fn live_source_passage_judgments() -> Result<()> {
    use crate::application::{
        ports::llm_port::SamplingOverride,
        services::claim_verification::{CheckPolicy, ClaimChecker, ClaimJudgment},
    };
    let settings: serde_json::Value = serde_json::from_slice(&std::fs::read(
        std::env::var("LATTICE_LLAMACPP_SETTINGS").unwrap(),
    )?)?;
    let config = serde_json::from_value(settings["settings"]["llm"].clone())?;
    let directory = std::path::PathBuf::from(std::env::var("LATTICE_LESSON_CALLS").unwrap());
    std::fs::create_dir_all(&directory)?;
    let model = ObservedModel {
        inner: LlamaCppLlm::new(&config)?,
        directory: directory.clone(),
        call: Default::default(),
    };
    let cases: Vec<serde_json::Value> = serde_json::from_slice(&std::fs::read(
        std::env::var("LATTICE_CLAIM_CASES").unwrap(),
    )?)?;
    let policy = match std::env::var("LATTICE_CLAIM_POLICY").as_deref() {
        Ok("fidelity") => CheckPolicy::Fidelity,
        Ok("strict") | Err(_) => CheckPolicy::Strict,
        Ok(other) => panic!("Unknown claim-check policy: {other}"),
    };
    let checker = ClaimChecker::new(&model, SamplingOverride::deterministic(), 512, policy);
    let mut results = Vec::new();
    let mut failures = Vec::new();
    for case in cases {
        let passages: Vec<String> = serde_json::from_value(case["passages"].clone())?;
        let judgment = if let Some(context) = case.get("context") {
            checker
                .check_fidelity_in_context(
                    case["claim"].as_str().unwrap(),
                    &passages,
                    &context.to_string(),
                )
                .await
        } else {
            checker
                .check_passages_without_deadline(case["claim"].as_str().unwrap(), &passages)
                .await
        };
        let ClaimJudgment::Judged(outcome) = judgment else {
            panic!("No valid evidence verdict: {judgment:?}")
        };
        let verdict = serde_json::to_value(outcome.verdict)?;
        results.push(serde_json::json!({"claim":case["claim"],"verdict":verdict,"reason":outcome.reason,"quote":outcome.quote,"confidence":outcome.confidence}));
        std::fs::write(
            directory.join("judgments.json"),
            serde_json::to_vec_pretty(&results)?,
        )?;
        let matches = if case["expected"] == "not_supported" {
            matches!(
                outcome.verdict,
                crate::application::services::claim_verification::ClaimVerdict::Unsupported
                    | crate::application::services::claim_verification::ClaimVerdict::Contradicted
            )
        } else {
            verdict == case["expected"]
        };
        if !matches {
            failures.push(format!(
                "{}: expected {}, received {}",
                case["claim"], case["expected"], verdict
            ));
        }
        if let Some(quote) = outcome.quote {
            assert!(passages.iter().any(|p| p == &quote));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    Ok(())
}

#[tokio::test]
#[ignore = "requires explicit live settings, search candidates and artifact directory"]
async fn live_reference_selection() -> Result<()> {
    let settings: serde_json::Value = serde_json::from_slice(&std::fs::read(
        std::env::var("LATTICE_LLAMACPP_SETTINGS").unwrap(),
    )?)?;
    let config = serde_json::from_value(settings["settings"]["llm"].clone())?;
    let directory = std::path::PathBuf::from(std::env::var("LATTICE_LESSON_CALLS").unwrap());
    std::fs::create_dir_all(&directory)?;
    let model = ObservedModel {
        inner: LlamaCppLlm::new(&config)?,
        directory: directory.clone(),
        call: Default::default(),
    };
    let cases: Vec<serde_json::Value> = serde_json::from_slice(&std::fs::read(
        std::env::var("LATTICE_REFERENCE_CASES").unwrap(),
    )?)?;
    for (index, case) in cases.into_iter().enumerate() {
        let results = serde_json::from_value(case["results"].clone())?;
        let chosen = crate::features::learning::content_verification::research::select_references(
            &model,
            case["topic"].as_str().unwrap(),
            case["query"].as_str().unwrap(),
            results,
        )
        .await?;
        let mut urls: Vec<_> = chosen.into_iter().map(|result| result.url).collect();
        std::fs::write(
            directory.join(format!("selection-{index}.json")),
            serde_json::to_vec_pretty(&urls)?,
        )?;
        urls.sort();
        let mut expected: Vec<String> = serde_json::from_value(case["expectedUrls"].clone())?;
        expected.sort();
        assert_eq!(urls, expected);
    }
    Ok(())
}

#[tokio::test]
#[ignore = "requires explicit live settings, a recorded teaching review and artifact directory"]
async fn live_teaching_review_evidence_scope() -> Result<()> {
    use crate::application::ports::llm_port::CompletionInput;
    let settings: serde_json::Value = serde_json::from_slice(&std::fs::read(
        std::env::var("LATTICE_LLAMACPP_SETTINGS").unwrap(),
    )?)?;
    let config = serde_json::from_value(settings["settings"]["llm"].clone())?;
    let directory = std::path::PathBuf::from(std::env::var("LATTICE_LESSON_CALLS").unwrap());
    std::fs::create_dir_all(&directory)?;
    let model = ObservedModel {
        inner: LlamaCppLlm::new(&config)?,
        directory: directory.clone(),
        call: Default::default(),
    };
    let recorded: serde_json::Value = serde_json::from_slice(&std::fs::read(
        std::env::var("LATTICE_TEACHING_REVIEW_CALL").unwrap(),
    )?)?;
    let input: Vec<CompletionInput> = serde_json::from_value(recorded["input"].clone())?;
    let message = |role: &str| {
        input
            .iter()
            .find_map(|item| match item {
                CompletionInput::Message {
                    role: found,
                    content,
                } if found == role => Some(content.clone()),
                _ => None,
            })
            .unwrap()
    };
    let system = format!(
        "{}\n{}",
        message("system"),
        crate::features::learning::teaching::LESSON_REVIEW_EVIDENCE_SCOPE
    );
    let progress = crate::features::learning::outline_progress::OutlineProgress::default();
    let prompt: serde_json::Value = serde_json::from_str(&message("user"))?;
    let mut candidate = prompt["candidate"].clone();
    for block in candidate["blocks"].as_array_mut().unwrap() {
        let body: String = block["bodyPassages"]
            .as_array()
            .unwrap()
            .iter()
            .map(|passage| passage["text"].as_str().unwrap())
            .collect();
        block.as_object_mut().unwrap().remove("bodyPassages");
        block.as_object_mut().unwrap().remove("index");
        block["body"] = serde_json::json!(body);
    }
    let issues = crate::features::learning::teaching_review::review(
        &model,
        &system,
        prompt,
        &candidate,
        Some(&progress),
    )
    .await?;
    std::fs::write(
        directory.join("replay-issues.json"),
        serde_json::to_vec_pretty(&issues)?,
    )?;
    // Other concrete defects (for example an attached citation mismatch) must
    // remain reportable. This replay only targets the original false objection
    // that RUSTUP_AUTO_INSTALL is absent from the authoring excerpts.
    let stale_excerpt_objection = issues.iter().any(|issue| {
        let issue = issue.to_lowercase();
        issue.contains("rustup_auto_install")
            && [
                "not present",
                "not provided",
                "not described",
                "absent",
                "no source",
                "unsupported",
            ]
            .iter()
            .any(|phrase| issue.contains(phrase))
    });

    // Evidence scope must not excuse a demonstrably wrong worked result.
    let wrong = serde_json::json!({"blocks":[{"kind":"explanation","title":"Adding whole numbers","body":"To add two whole numbers, combine their quantities. Worked example: start with two apples and add two more apples. The total is five apples, so 2 + 2 = 5. This is ordinary integer addition, with no rounding or special convention.","rubric":[]}],"questions":[]});
    let issues = crate::features::learning::teaching_review::review(&model, &system, serde_json::json!({"task":"Review instructional quality","authoringContext":{"goal":"Learn ordinary whole-number addition","sources":[]}}), &wrong, Some(&progress)).await?;
    std::fs::write(
        directory.join("incorrect-example-issues.json"),
        serde_json::to_vec_pretty(&issues)?,
    )?;
    assert!(
        !issues.is_empty(),
        "The evidence-scope clarification excused a concrete arithmetic error"
    );
    assert!(
        !stale_excerpt_objection,
        "The reviewer again treated partial authoring excerpts as the entire evidence collection"
    );
    Ok(())
}

#[tokio::test]
#[ignore = "requires live settings, a recorded coverage audit and artifact directory"]
async fn live_passage_coverage_preserves_complete_assertions() -> Result<()> {
    let settings: serde_json::Value = serde_json::from_slice(&std::fs::read(
        std::env::var("LATTICE_LLAMACPP_SETTINGS").unwrap(),
    )?)?;
    let config = serde_json::from_value(settings["settings"]["llm"].clone())?;
    let directory = std::path::PathBuf::from(std::env::var("LATTICE_LESSON_CALLS").unwrap());
    std::fs::create_dir_all(&directory)?;
    let model = ObservedModel {
        inner: LlamaCppLlm::new(&config)?,
        directory: directory.clone(),
        call: Default::default(),
    };
    let recorded: serde_json::Value = serde_json::from_slice(&std::fs::read(
        std::env::var("LATTICE_COVERAGE_CALL").unwrap(),
    )?)?;
    let input = recorded["input"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["role"] == "user")
        .unwrap()["content"]
        .as_str()
        .unwrap();
    let input: serde_json::Value = serde_json::from_str(input)?;
    let indices: Vec<usize> =
        serde_json::from_str(&std::env::var("LATTICE_COVERAGE_UNITS").unwrap())?;
    let units: Vec<_> = indices.iter().map(|index| {
        let claims = input["inventory"].as_array().unwrap().iter().find(|unit| unit["index"].as_u64()==Some(*index as u64)).unwrap();
        serde_json::json!({"content":input["units"][*index]["content"],"statements":claims["statements"],"nonFactualReason":claims["nonFactualReason"]})
    }).collect();
    let required: serde_json::Value = serde_json::from_str(
        &std::env::var("LATTICE_COVERAGE_REQUIRED_MISSING").unwrap_or_else(|_| "{}".into()),
    )?;
    let mut cases = vec![
        serde_json::json!({"units":units,"expectedIncomplete":(0..indices.len()).collect::<Vec<_>>(),"requiredMissingTerms":required}),
    ];
    cases.extend(serde_json::from_str::<Vec<serde_json::Value>>(
        include_str!("content_verification/coverage_scope_cases.json"),
    )?);
    let progress = crate::features::learning::outline_progress::OutlineProgress::default();
    for (index, case) in cases.into_iter().enumerate() {
        let report = crate::features::learning::content_verification::live_coverage_fixture(
            &model, &case, &progress,
        )
        .await?;
        std::fs::write(
            directory.join(format!("coverage-{index}.json")),
            serde_json::to_vec_pretty(&report)?,
        )?;
        let incomplete: Vec<_> = report["units"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|unit| unit["complete"] == false)
            .map(|unit| unit["index"].clone())
            .collect();
        assert_eq!(
            serde_json::json!(incomplete),
            case["expectedIncomplete"],
            "Coverage case {index}"
        );
        if let Some(required) = case["requiredMissingTerms"].as_object() {
            for (unit, terms) in required {
                let unit: usize = unit.parse().unwrap();
                let reason = report["units"][unit]["reason"]
                    .as_str()
                    .unwrap()
                    .to_lowercase();
                for term in terms.as_array().unwrap() {
                    assert!(reason.contains(&term.as_str().unwrap().to_lowercase()), "Coverage case {index}, unit {unit} missed the specific regression: {reason}");
                }
            }
        }
    }
    Ok(())
}

#[tokio::test]
#[ignore = "requires live settings, saved draft, review call, database and artifact directory"]
async fn live_review_findings_require_evidence() -> Result<()> {
    use crate::features::learning::{reference_collection::ReferenceCollection, review_evidence};
    let settings: serde_json::Value = serde_json::from_slice(&std::fs::read(
        std::env::var("LATTICE_LLAMACPP_SETTINGS").unwrap(),
    )?)?;
    let config = serde_json::from_value(settings["settings"]["llm"].clone())?;
    let directory = std::path::PathBuf::from(std::env::var("LATTICE_LESSON_CALLS").unwrap());
    std::fs::create_dir_all(&directory)?;
    let model = ObservedModel {
        inner: LlamaCppLlm::new(&config)?,
        directory: directory.clone(),
        call: Default::default(),
    };
    let draft: crate::features::learning::lesson_drafts::Draft = serde_json::from_slice(
        &std::fs::read(std::env::var("LATTICE_REVIEW_DRAFT").unwrap())?,
    )?;
    let recorded: serde_json::Value = serde_json::from_slice(&std::fs::read(
        std::env::var("LATTICE_TEACHING_REVIEW_CALL").unwrap(),
    )?)?;
    let review: serde_json::Value = serde_json::from_str(recorded["answer"].as_str().unwrap())?;
    let issues: Vec<String> = serde_json::from_value(review["issues"].clone())?;
    let authoring = serde_json::from_str(&draft.authoring.unwrap().prompt)?;
    let candidate = serde_json::from_str(&draft.candidate)?;
    let options = sqlx::sqlite::SqliteConnectOptions::new()
        .filename(std::env::var("LATTICE_LESSON_DATABASE").unwrap())
        .foreign_keys(true)
        .create_if_missing(false);
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;
    let models = crate::infrastructure::persistence::repositories::DownloadedModelRepository::new(
        pool.clone(),
    );
    let embedding_model = models.get_active_embedding_model().await?.unwrap();
    let location = embedding_model.location().enclosing_dir().unwrap();
    let identity = embedding_model
        .embedding_artifact_identity()
        .cloned()
        .unwrap();
    let embedding = tokio::task::spawn_blocking(move || {
        crate::features::embedding::candle_service::CandleEmbeddingService::open(location, identity)
    })
    .await
    .unwrap()?;
    let program = std::env::var("LATTICE_REVIEW_PROGRAM").unwrap();
    let sources = LearningRepository::new(pool.clone())
        .verification_sources(&program)
        .await?;
    let references = ReferenceCollection::load(&pool, &program, &sources, Some(&embedding)).await?;
    let progress = crate::features::learning::outline_progress::OutlineProgress::default();
    let accepted = review_evidence::check(
        &model,
        &issues,
        &authoring,
        &candidate,
        &references,
        Some(&progress),
    )
    .await?;
    std::fs::write(
        directory.join("accepted-replay-issues.json"),
        serde_json::to_vec_pretty(&accepted)?,
    )?;
    assert!(
        accepted.is_empty(),
        "False reviewer assertions triggered editing: {accepted:?}"
    );
    // The expected findings live only in test data and are never sent to the model.
    let cases: Vec<serde_json::Value> =
        serde_json::from_str(include_str!("content_verification/review_scope_cases.json"))?;
    for (index, case) in cases.into_iter().enumerate() {
        let sources = serde_json::from_value::<
            Vec<crate::features::learning::dto::LearningSourceDto>,
        >(case["sources"].clone())?;
        let references = ReferenceCollection::lexical(&sources)?;
        let issues: Vec<String> = serde_json::from_value(case["issues"].clone())?;
        let accepted = review_evidence::check(
            &model,
            &issues,
            &case["requirements"],
            &case["candidate"],
            &references,
            Some(&progress),
        )
        .await?;
        std::fs::write(
            directory.join(format!("accepted-control-{index}.json")),
            serde_json::to_vec_pretty(&accepted)?,
        )?;
        let expected: Vec<String> = serde_json::from_value(case["expected"].clone())?;
        assert_eq!(accepted, expected);
    }
    Ok(())
}

#[tokio::test]
#[ignore = "writes to LATTICE_LESSON_DATABASE; requires settings, failed job and artifact directory"]
async fn live_retry_saved_lesson_through_publication() -> Result<()> {
    use crate::features::learning::{
        curriculum::LearningGenerationJobStatus, dto::LearningSourcePolicy,
        plan_dto::LearningGenerationJobActionRequestDto,
    };
    let _ = tracing_subscriber::fmt().with_env_filter("lattice::features::learning=info,lattice::application::services::claim_verification=warn").try_init();
    let settings: serde_json::Value = serde_json::from_slice(&std::fs::read(
        std::env::var("LATTICE_LLAMACPP_SETTINGS").unwrap(),
    )?)?;
    let config = serde_json::from_value(settings["settings"]["llm"].clone())?;
    let directory = std::path::PathBuf::from(std::env::var("LATTICE_LESSON_CALLS").unwrap());
    std::fs::create_dir_all(&directory)?;
    let llm: Arc<dyn LLMPort> = Arc::new(ObservedModel {
        inner: LlamaCppLlm::new(&config)?,
        directory: directory.clone(),
        call: std::sync::atomic::AtomicUsize::new(0),
    });
    let options = sqlx::sqlite::SqliteConnectOptions::new()
        .filename(std::env::var("LATTICE_LESSON_DATABASE").unwrap())
        .foreign_keys(true)
        .create_if_missing(false);
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;
    let repo = LearningCurriculumRepository::new(pool.clone());
    let old = repo
        .job(&std::env::var("LATTICE_LESSON_JOB").unwrap())
        .await?;
    // Explicitly recover only the selected orphan after a crashed test process.
    // Never run this option while its original worker is still alive.
    if std::env::var("LATTICE_LESSON_RECOVER_ORPHAN").as_deref() == Ok("1") {
        assert_eq!(old.status, LearningGenerationJobStatus::Running);
        repo.interrupt_job(&old.id).await?;
    }
    let program = LearningRepository::new(pool.clone())
        .get(&old.program_id)
        .await?;
    assert!(
        !repo.jobs(&old.program_id).await?.iter().any(|j| matches!(
            j.status,
            LearningGenerationJobStatus::Pending | LearningGenerationJobStatus::Running
        )),
        "Another job is active"
    );
    // Explicit cleanup of captures produced by a failed research run. Keep the
    // immutable snapshots/tombstones and use the same repository operation as
    // the source UI; never mutate job approval or lesson content here.
    if let Ok(path) = std::env::var("LATTICE_REMOVE_RESEARCH_SOURCES") {
        let removals: Vec<
            crate::features::learning::portability_dto::DeleteLearningSourceRequestDto,
        > = serde_json::from_slice(&std::fs::read(path)?)?;
        let library =
            crate::features::learning::source_library::LearningSourceLibraryRepository::new(
                pool.clone(),
            );
        for removal in removals {
            assert_eq!(removal.program_id, old.program_id);
            library.delete_source(&removal).await?;
            println!("Removed irrelevant research capture {}", removal.source_id);
        }
    }
    let models = crate::infrastructure::persistence::repositories::DownloadedModelRepository::new(
        pool.clone(),
    );
    let model = models
        .get_active_embedding_model()
        .await?
        .expect("active embedding model");
    let model_dir = model.location().enclosing_dir().unwrap();
    let identity = model.embedding_artifact_identity().cloned().unwrap();
    let embedding: Arc<dyn crate::application::ports::EmbeddingPort> = Arc::new(
        tokio::task::spawn_blocking(move || {
            crate::features::embedding::candle_service::CandleEmbeddingService::open(
                model_dir, identity,
            )
        })
        .await
        .unwrap()?,
    );
    let source_pool = pool.clone();
    let web_dir = tempfile::tempdir()?;
    let worker = LessonGenerationWorker {
        research_web: Some(Arc::new(
            crate::features::web::services::web::WebService::new(web_dir.path())?,
        )),
        pool: pool.clone(),
        load_llm: Arc::new(move || {
            let llm = llm.clone();
            Box::pin(async move { Ok(llm) })
        }),
        load_embedding: Arc::new(move || {
            let embedding = embedding.clone();
            Box::pin(async move { Some(embedding) })
        }),
        refresh_sources: Arc::new(move |program_id| {
            let pool = source_pool.clone();
            Box::pin(async move {
                let workspace = crate::features::learning::source_library::LearningSourceLibraryRepository::new(pool).workspace(&program_id).await?;
                assert!(
                    workspace
                        .sources
                        .iter()
                        .all(|s| s.freshness_policy != LearningSourcePolicy::BeforeUse),
                    "This runner requires pinned sources; use native app for before-use refresh"
                );
                Ok(())
            })
        }),
    };
    let job = repo
        .retry_job(&LearningGenerationJobActionRequestDto {
            operation_id: uuid::Uuid::new_v4().to_string(),
            program_id: old.program_id,
            job_id: old.id,
            expected_revision: program.summary.revision,
        })
        .await?;
    std::fs::write(directory.join("job-id.txt"), &job.id)?;
    println!("Live lesson job {} started", job.id);
    worker.run(&job.id, CancellationToken::new()).await;
    let result = repo.job(&job.id).await?;
    std::fs::write(
        directory.join("result.json"),
        serde_json::to_vec_pretty(&result)?,
    )?;
    assert_eq!(
        result.status,
        LearningGenerationJobStatus::Completed,
        "Full live worker did not publish: {:?}",
        result
    );
    let published = LearningRepository::new(pool).get(&job.program_id).await?;
    std::fs::write(
        directory.join("program.json"),
        serde_json::to_vec_pretty(&published)?,
    )?;
    println!(
        "Published verified lesson through the production worker: {}",
        job.id
    );
    Ok(())
}
