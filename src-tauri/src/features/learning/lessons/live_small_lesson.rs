//! A small, live, end-to-end publication test in a new isolated database.
//! Only the course outline and source material are fixtures. Authoring,
//! review, repairs, extraction, verification and publication are production code.
use super::*;
use crate::features::learning::{
    curriculum::{LearningGenerationJobKind, LearningGenerationJobStatus},
    dto::*,
    plan_dto::{LearningGenerationJobActionRequestDto, StartLearningGenerationJobRequestDto},
};

#[tokio::test]
#[ignore = "real-model controls for requirements versus empirical claims; does not publish content"]
async fn live_contract_fidelity_controls() -> Result<()> {
    use crate::application::ports::llm_port::SamplingOverride;
    use crate::application::services::claim_verification::{
        CheckPolicy, ClaimChecker, ClaimJudgment,
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
    let cases: Vec<serde_json::Value> = match std::env::var("LATTICE_FIDELITY_CASES") {
        Ok(path) => serde_json::from_slice(&std::fs::read(path)?)?,
        Err(_) => serde_json::from_str(include_str!("live_fixtures/verification_controls.json"))?,
    };
    let mut batched = if std::env::var_os("LATTICE_CHECK_BATCH").is_some() {
        use crate::application::services::claim_verification::LocatedClaim;
        let targets = cases
            .iter()
            .map(|case| {
                assert_eq!(case["policy"], "strict");
                LocatedClaim {
                    claim: case["claim"].as_str().unwrap().into(),
                    passages: serde_json::from_value(case["statements"].clone()).unwrap(),
                }
            })
            .collect::<Vec<_>>();
        ClaimChecker::new(
            &model,
            SamplingOverride::deterministic(),
            512,
            CheckPolicy::Strict,
        )
        .check_batch_without_deadline(&targets, &|_| Ok(()), &|_| Ok(()))
        .await?
        .into_iter()
        .collect::<std::collections::VecDeque<_>>()
    } else {
        Default::default()
    };
    for (index, case) in cases.iter().enumerate() {
        let strict = case["policy"] == "strict";
        let checker = ClaimChecker::new(
            &model,
            SamplingOverride::deterministic(),
            512,
            if strict {
                CheckPolicy::Strict
            } else {
                CheckPolicy::Fidelity
            },
        );
        let statements: Vec<String> = serde_json::from_value(case["statements"].clone())?;
        let judgment = if let Some(judgment) = batched.pop_front() {
            judgment
        } else if strict {
            checker
                .check_passages_without_deadline(case["claim"].as_str().unwrap(), &statements)
                .await
        } else if case["kind"] == "assessment" {
            checker
                .check_fidelity_in_context(
                    case["claim"].as_str().unwrap(),
                    &statements,
                    case["context"].as_str().unwrap(),
                )
                .await
        } else {
            checker
                .check_fidelity_in_teaching_context(
                    case["claim"].as_str().unwrap(),
                    &statements,
                    case["context"].as_str().unwrap(),
                )
                .await
        };
        let ClaimJudgment::Judged(finding) = judgment else {
            panic!("Live fidelity control {index} did not return a usable judgment: {judgment:?}");
        };
        let result = serde_json::json!({"verdict":finding.verdict,"reason":finding.reason});
        std::fs::write(
            directory.join(format!("case-{index}.json")),
            serde_json::to_vec_pretty(&result)?,
        )?;
        assert_eq!(
            result["verdict"], case["expected"],
            "Live fidelity control {index}"
        );
        println!(
            "LIVE FIDELITY CONTROL {index} PASSED: {}",
            result["verdict"]
        );
    }
    Ok(())
}

#[tokio::test]
#[ignore = "replays captured sections through real model extraction; does not approve lesson content"]
async fn live_inventory_slots_keep_every_section() -> Result<()> {
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
    let content: Vec<serde_json::Value> = serde_json::from_slice(&std::fs::read(
        std::env::var("LATTICE_INVENTORY_SECTIONS").unwrap(),
    )?)?;
    let result =
        crate::features::learning::content_verification::live_inventory_fixture(&model, &content)
            .await?;
    assert_eq!(result["units"].as_array().unwrap().len(), content.len());
    assert_eq!(model.call.load(std::sync::atomic::Ordering::Relaxed), 1);
    std::fs::write(
        directory.join("inventory.json"),
        serde_json::to_vec_pretty(&result)?,
    )?;
    println!(
        "LIVE INVENTORY SLOTS PASSED: {} sections in one model call",
        content.len()
    );
    Ok(())
}

#[tokio::test]
#[ignore = "requires live settings and review controls; makes real model requests"]
async fn live_review_schema_preserves_grounding() -> Result<()> {
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
        std::env::var("LATTICE_REVIEW_CASES").unwrap(),
    )?)?;
    let progress = crate::features::learning::outline_progress::OutlineProgress::default();
    for (index, case) in cases.into_iter().enumerate() {
        let sources: Vec<LearningSourceDto> = serde_json::from_value(case["sources"].clone())?;
        let references =
            crate::features::learning::reference_collection::ReferenceCollection::lexical(
                &sources,
            )?;
        let issues: Vec<String> = serde_json::from_value(case["issues"].clone())?;
        let accepted = crate::features::learning::review_evidence::check(
            &model,
            &issues,
            &case["requirements"],
            &case["candidate"],
            &references,
            Some(&progress),
        )
        .await?;
        std::fs::write(
            directory.join(format!("case-{index}.json")),
            serde_json::to_vec_pretty(&accepted)?,
        )?;
        assert_eq!(
            serde_json::to_value(&accepted)?,
            case["expected"],
            "Review grounding control {index}"
        );
        println!(
            "LIVE REVIEW CONTROL {index} PASSED: {} of {} findings actionable",
            accepted.len(),
            issues.len()
        );
    }
    Ok(())
}

#[tokio::test]
#[ignore = "requires live settings and a recorded inventory basis/draft; never modifies the library"]
async fn live_saved_inventory_revision_finishes() -> Result<()> {
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
    let basis = serde_json::from_slice(&std::fs::read(
        std::env::var("LATTICE_INVENTORY_BASIS").unwrap(),
    )?)?;
    let draft: serde_json::Value = serde_json::from_slice(&std::fs::read(
        std::env::var("LATTICE_INVENTORY_DRAFT").unwrap(),
    )?)?;
    let candidate = serde_json::from_str(draft["candidate"].as_str().unwrap())?;
    let result = crate::features::learning::content_verification::live_revision_fixture(
        &model, basis, candidate,
    )
    .await?;
    std::fs::write(
        directory.join("revision.json"),
        serde_json::to_vec_pretty(&result)?,
    )?;
    println!("LIVE INVENTORY REVISION FINISHED: {} original claims, {} revised, {} changed statements, {} model calls including resume",result["originalCount"],result["revisedCount"],result["changedStatements"],model.call.load(std::sync::atomic::Ordering::Relaxed));
    Ok(())
}

#[tokio::test]
#[ignore = "requires live settings, a small outline/source fixture, and a fresh artifact directory"]
async fn live_small_lesson_reaches_ready() -> Result<()> {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("lattice::features::learning=info")
        .try_init();
    let settings: serde_json::Value = serde_json::from_slice(&std::fs::read(
        std::env::var("LATTICE_LLAMACPP_SETTINGS").unwrap(),
    )?)?;
    let config = serde_json::from_value(settings["settings"]["llm"].clone())?;
    let directory = std::path::PathBuf::from(std::env::var("LATTICE_LESSON_CALLS").unwrap());
    std::fs::create_dir_all(&directory)?;
    let database = directory.join("lesson.sqlite");
    // Never attach to, overwrite or run a second worker on the user's library.
    let _ = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&database)?;
    let options = sqlx::sqlite::SqliteConnectOptions::new()
        .filename(&database)
        .foreign_keys(true);
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options.clone())
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;
    let model = Arc::new(ObservedModel {
        inner: LlamaCppLlm::new(&config)?,
        directory: directory.clone(),
        call: Default::default(),
    });
    let fixture: serde_json::Value = serde_json::from_slice(&std::fs::read(
        std::env::var("LATTICE_LESSON_FIXTURE").unwrap(),
    )?)?;
    let mut program = crate::features::learning::tests::fixture();
    program.modules.truncate(1);
    program.modules[0].lessons.truncate(1);
    program.summary.title = fixture["title"].as_str().unwrap().into();
    program.summary.goal = fixture["goal"].as_str().unwrap().into();
    program.prior_knowledge = fixture["priorKnowledge"].as_str().unwrap().into();
    program.minutes_per_session = 10;
    program.model_name = model.model_name().into();
    program.summary.module_count = 1;
    program.summary.lesson_count = 1;
    program.modules[0].title = program.summary.title.clone();
    program.modules[0].summary = program.summary.goal.clone();
    program.modules[0].outcomes = vec![fixture["objective"].as_str().unwrap().into()];
    let lesson = &mut program.modules[0].lessons[0];
    lesson.title = program.summary.title.clone();
    lesson.objective = fixture["objective"].as_str().unwrap().into();
    lesson.estimated_minutes = 10;
    let lesson_id = lesson.id.clone();
    program.sources = fixture["sources"]
        .as_array()
        .unwrap()
        .iter()
        .map(|source| LearningSourceDto {
            id: uuid::Uuid::new_v4().to_string(),
            title: source["title"].as_str().unwrap().into(),
            url: source["url"].as_str().map(str::to_owned),
            excerpt: source["text"].as_str().unwrap().into(),
            acquired_at: chrono::Utc::now().timestamp_millis(),
        })
        .collect();
    let programs = LearningRepository::new(pool.clone());
    let references: Vec<_> = program
        .sources
        .iter()
        .map(
            |source| crate::features::learning::sources::InitialReference {
                source_id: source.id.clone(),
                origin: format!("fixture:{}", source.id),
                captured: crate::features::learning::source_library::CapturedLearningSource {
                    title: source.title.clone(),
                    publisher: None,
                    requested_url: None,
                    resolved_url: None,
                    text: source.excerpt.clone(),
                    truncated: false,
                    extraction_version: "live-fixture-v1".into(),
                },
            },
        )
        .collect();
    programs
        .create_with_references(&program, &references)
        .await?;
    programs
        .accept(&AcceptLearningProgramRequestDto {
            program_id: program.summary.id.clone(),
            expected_revision: 0,
            title: program.summary.title.clone(),
        })
        .await?;
    let program = programs.get(&program.summary.id).await?;
    let repo = LearningCurriculumRepository::new(pool.clone());
    let job = repo
        .start_job(&StartLearningGenerationJobRequestDto {
            operation_id: uuid::Uuid::new_v4().to_string(),
            program_id: program.summary.id.clone(),
            expected_revision: program.summary.revision,
            kind: LearningGenerationJobKind::LessonPreparation,
            request_json: serde_json::json!({"lessonIds":[lesson_id]}).to_string(),
            progress_total: 1,
        })
        .await?;
    std::fs::write(directory.join("job-id.txt"), &job.id)?;
    drop(repo);
    drop(programs);
    run_and_confirm(pool, options, model, &job.id, &directory).await
}

async fn run_and_confirm(
    pool: sqlx::SqlitePool,
    options: sqlx::sqlite::SqliteConnectOptions,
    model: Arc<ObservedModel>,
    job_id: &str,
    directory: &std::path::Path,
) -> Result<()> {
    let repo = LearningCurriculumRepository::new(pool.clone());
    let job = repo.job(job_id).await?;
    let live = model.clone();
    let web_dir = tempfile::tempdir()?;
    let worker = LessonGenerationWorker {
        pool: pool.clone(),
        load_llm: Arc::new(move || {
            let llm: Arc<dyn LLMPort> = live.clone();
            Box::pin(async move { Ok(llm) })
        }),
        load_library: Arc::new(|| Box::pin(async { None })),
        refresh_sources: Arc::new(|_| Box::pin(async { Ok(()) })),
        research_web: Some(Arc::new(
            crate::features::web::services::web::WebService::new(web_dir.path())?,
        )),
    };
    // Run through the native job runtime: transient network/model failures
    // stay queued, and the runtime redelivers them when their retry is due.
    let runtime = crate::shared::runtime::jobs::JobRuntime::new(pool.clone());
    runtime
        .register(
            crate::features::learning::curriculum_repository::LESSON_PREPARATION,
            Arc::new(worker),
            crate::features::learning::curriculum_repository::lesson_job_config(),
        )
        .await?;
    runtime.dispatch(&job.id);
    while matches!(
        repo.job(&job.id).await?.status,
        LearningGenerationJobStatus::Pending | LearningGenerationJobStatus::Running
    ) {
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
    }
    runtime.close();
    runtime.drain().await;
    let result = repo.job(&job.id).await?;
    std::fs::write(
        directory.join("result.json"),
        serde_json::to_vec_pretty(&result)?,
    )?;
    assert_eq!(
        result.status,
        LearningGenerationJobStatus::Completed,
        "Production worker did not publish; see result.json"
    );
    drop(repo);
    pool.close().await;
    // Read readiness from a new connection after closing the worker's pool.
    let reopened = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;
    let published = LearningRepository::new(reopened.clone())
        .get(&job.program_id)
        .await?;
    let lesson = &published.modules[0].lessons[0];
    assert_eq!(lesson.preparation, LearningPreparation::Ready);
    assert!(!lesson.blocks.is_empty() && !lesson.questions.is_empty());
    std::fs::write(
        directory.join("program.json"),
        serde_json::to_vec_pretty(&published)?,
    )?;
    let first_started = LearningCurriculumRepository::new(reopened.clone())
        .jobs(&job.program_id)
        .await?
        .iter()
        .map(|job| job.created_at)
        .min()
        .unwrap_or_default();
    let completed_calls = std::fs::read_dir(directory)?
        .filter_map(|entry| entry.ok())
        .filter(|entry| {
            entry.file_name().to_str().is_some_and(|name| {
                name.strip_prefix("call-")
                    .and_then(|name| name.strip_suffix(".json"))
                    .and_then(|number| number.parse::<usize>().ok())
                    .is_some()
            })
        })
        .count();
    let summary = serde_json::json!({"status":"completed","preparation":"ready","wallSeconds":(chrono::Utc::now().timestamp_millis()-first_started) as f64/1000.0,"completedModelCalls":completed_calls,"programId":job.program_id,"lessonId":lesson.id,"jobId":job.id});
    std::fs::write(
        directory.join("summary.json"),
        serde_json::to_vec_pretty(&summary)?,
    )?;
    println!("LIVE PUBLICATION CONFIRMED: {summary}");
    reopened.close().await;
    Ok(())
}

#[tokio::test]
#[ignore = "resumes or retries the isolated lesson.sqlite after its prior test worker has stopped"]
async fn live_small_lesson_resumes_to_ready() -> Result<()> {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("lattice::features::learning=info")
        .try_init();
    let directory = std::path::PathBuf::from(std::env::var("LATTICE_LESSON_CALLS").unwrap());
    let mut job_id = std::fs::read_to_string(directory.join("job-id.txt"))?;
    let options = sqlx::sqlite::SqliteConnectOptions::new()
        .filename(directory.join("lesson.sqlite"))
        .foreign_keys(true)
        .create_if_missing(false);
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options.clone())
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;
    let repo = LearningCurriculumRepository::new(pool.clone());
    let job = repo.job(&job_id).await?;
    assert!(
        matches!(
            job.status,
            LearningGenerationJobStatus::Running
                | LearningGenerationJobStatus::Pending
                | LearningGenerationJobStatus::Failed
        ),
        "Only an interrupted, deferred or failed test worker may be continued here"
    );
    if job.status == LearningGenerationJobStatus::Failed {
        assert!(
            !repo
                .jobs(&job.program_id)
                .await?
                .iter()
                .any(|other| matches!(
                    other.status,
                    LearningGenerationJobStatus::Running | LearningGenerationJobStatus::Pending
                )),
            "Another test worker is active"
        );
        let program = LearningRepository::new(pool.clone())
            .get(&job.program_id)
            .await?;
        // Use the same retry operation as the UI. It copies durable work but
        // cannot approve content or bypass any publication checks.
        let retry = repo
            .retry_job(&LearningGenerationJobActionRequestDto {
                operation_id: uuid::Uuid::new_v4().to_string(),
                program_id: job.program_id,
                job_id: job.id,
                expected_revision: program.summary.revision,
            })
            .await?;
        job_id = retry.id;
        std::fs::write(directory.join("job-id.txt"), &job_id)?;
    }
    let settings: serde_json::Value = serde_json::from_slice(&std::fs::read(
        std::env::var("LATTICE_LLAMACPP_SETTINGS").unwrap(),
    )?)?;
    let config = serde_json::from_value(settings["settings"]["llm"].clone())?;
    let next_call = std::fs::read_dir(&directory)?
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            e.file_name()
                .to_str()
                .and_then(|name| name.strip_prefix("call-"))
                .and_then(|name| name.strip_suffix(".json"))
                .and_then(|name| name.parse::<usize>().ok())
        })
        .max()
        .map_or(0, |n| n + 1);
    let model = Arc::new(ObservedModel {
        inner: LlamaCppLlm::new(&config)?,
        directory: directory.clone(),
        call: std::sync::atomic::AtomicUsize::new(next_call),
    });
    // Exercise the same recovery the job runtime runs at native startup.
    crate::shared::runtime::jobs::JobStore::new(pool.clone())
        .recover(
            crate::features::learning::curriculum_repository::LESSON_PREPARATION,
            crate::shared::runtime::jobs::RecoveryPolicy::Requeue,
        )
        .await?;
    assert_eq!(
        repo.job(&job_id).await?.status,
        LearningGenerationJobStatus::Pending
    );
    drop(repo);
    run_and_confirm(pool, options, model, &job_id, &directory).await
}
