//! A live, end-to-end course test in a new isolated database, on the app's own
//! runtime and, on Windows and Linux, a bare app's page browser, through the
//! app's own composition and commands: the container `headless::open` builds,
//! the app's job kinds, and the bodies of the commands the UI calls to draft
//! an outline, plan its topics, accept it and prepare its first module. The
//! Learning model, the keyring and the web come only from the container, and
//! every model and web call goes through a call log, recorded or replayed from
//! an earlier run.
use super::*;
use crate::features::learning::embedded_runtime::BuiltinRuntimes;
use crate::features::learning::{
    course_plan::PlanLearningCourseRequestDto,
    dto::*,
    outline_draft::{LearningOutlineReviewStatus, RepairLearningOutlineRequestDto},
    outline_progress::LearningOutlineProgressDto,
    plugin,
};
use crate::interfaces::di::Container;
use crate::tests::call_log::{self, CallLog};
use lattice_core::application::ports::llm_port::ModelRequirements;
use lattice_core::application::ports::SettingsRepositoryPort;
use lattice_core::shared::ipc::ApiError;
use lattice_core::shared::persistence::Page;
use lattice_core::shared::runtime::jobs::{JobRecord, JobStatus};

/// Whether no worker still owes the job work.
fn ended(job: &JobRecord) -> bool {
    !matches!(job.status, JobStatus::Pending | JobStatus::Running)
}

fn api(error: ApiError) -> AppError {
    AppError::InternalError(format!("{:?}: {}", error.code, error.message))
}

fn required(name: &str) -> Result<String> {
    std::env::var(name).map_err(|_| AppError::InvalidConfig(format!("Set {name}.")))
}

/// A job's kind, status and timeline.
fn job_json(job: &JobRecord) -> serde_json::Value {
    let seconds = |from: Option<i64>, to: Option<i64>| Some((to? - from?) as f64 / 1000.0);
    serde_json::json!({
        "id": job.id,
        "kind": job.kind,
        "subject": job.subject_id,
        "status": job.status.as_str(),
        "errorCode": job.error_code,
        "causeCode": job.cause_code,
        "errorMessage": job.error_message,
        "progress": [job.progress_current, job.progress_total],
        "progressMessage": job.progress_message,
        "retryCount": job.retry_count,
        "createdAt": job.created_at,
        "startedAt": job.started_at,
        "finishedAt": job.finished_at,
        "queuedSeconds": seconds(Some(job.created_at), job.started_at),
        "ranSeconds": seconds(job.started_at, job.finished_at),
    })
}

/// The latest attempt of every job of `kinds`.
async fn all_jobs(container: &Container, kinds: &[&str]) -> Result<Vec<JobRecord>> {
    container
        .jobs()
        .store()
        .list_latest(kinds, Page::new(None, None)?)
        .await
}

/// Each lesson of the module with its end state and its job's timeline.
async fn lesson_end_states(
    container: &Container,
    program_id: &str,
    module_id: &str,
) -> Result<serde_json::Value> {
    use crate::features::learning::curriculum_repository::LESSON_PREPARATION;
    let program = LearningRepository::new(container.database().clone())
        .get(program_id)
        .await?;
    let repo = LearningCurriculumRepository::new(container.database().clone());
    let mut jobs = Vec::new();
    for record in container
        .jobs()
        .store()
        .list_for_subject(LESSON_PREPARATION, program_id)
        .await?
    {
        jobs.push((repo.job(&record.id).await?, record));
    }
    let lessons = program
        .modules
        .iter()
        .find(|module| module.id == module_id)
        .map(|module| module.lessons.as_slice())
        .unwrap_or_default();
    Ok(serde_json::Value::Array(
        lessons
            .iter()
            .map(|lesson| {
                let job = jobs
                    .iter()
                    .find(|(job, _)| job.lesson_ids.contains(&lesson.id));
                let state = if lesson.preparation == LearningPreparation::Ready {
                    "published".to_owned()
                } else if job.is_some_and(|(job, _)| job.held_lesson_ids.contains(&lesson.id)) {
                    "held_for_review".to_owned()
                } else {
                    job.map_or("no_job".to_owned(), |(job, _)| {
                        format!("{:?}", job.status).to_lowercase()
                    })
                };
                serde_json::json!({
                    "lessonId": lesson.id,
                    "title": lesson.title,
                    "endState": state,
                    "heldForReview": job.is_some_and(|(job, _)| job.held_lesson_ids.contains(&lesson.id)),
                    "job": job.map(|(_, record)| job_json(record)),
                })
            })
            .collect(),
    ))
}

fn report(update: LearningOutlineProgressDto) {
    println!(
        "Live outline: {:?} at {}s, {} response characters",
        update.stage, update.elapsed_seconds, update.response_characters
    );
}

/// What the UI does after each outline operation: plan the course's topics.
async fn plan_topics(container: &Container, program_id: &str) {
    let request = PlanLearningCourseRequestDto {
        program_id: program_id.to_owned(),
    };
    if let Err(error) = plugin::plan_course(container, request).await {
        println!("Live outline: planning topics failed: {}", error.message);
    }
}

pub(super) fn review_status(program: &LearningProgramDto) -> Option<LearningOutlineReviewStatus> {
    program
        .outline_review
        .as_ref()
        .map(|review| review.status.clone())
}

/// Every outline state the learner would have seen, for diagnosis after a run.
pub(super) fn record(
    directory: &std::path::Path,
    step: &str,
    program: &LearningProgramDto,
) -> Result<()> {
    std::fs::write(
        directory.join(format!("outline-{step}.json")),
        serde_json::to_vec_pretty(&serde_json::json!({
            "review": program.outline_review,
            "sources": program.sources.iter().map(|s| serde_json::json!({"title": s.title, "url": s.url})).collect::<Vec<_>>(),
            "modules": program.modules.iter().map(|m| serde_json::json!({
                "title": m.title, "summary": m.summary, "outcomes": m.outcomes,
                "lessons": m.lessons.iter().map(|l| serde_json::json!({"title": l.title, "objective": l.objective})).collect::<Vec<_>>(),
            })).collect::<Vec<_>>(),
        }))?,
    )?;
    let review = program.outline_review.as_ref();
    println!(
        "LIVE COURSE {step}: {:?}, {} findings, {} sources, note: {}",
        review.map(|r| &r.status),
        review.map_or(0, |r| r.issues.len()),
        program.sources.len(),
        review.map(|r| r.note.as_str()).unwrap_or_default()
    );
    Ok(())
}

/// What one course of the run reached, kept even when it fails part way.
struct CourseRun {
    name: String,
    goal: String,
    program_id: Option<String>,
    module_id: Option<String>,
    review: Option<String>,
    manual_repairs: usize,
    generation_seconds: u64,
    outline_seconds: Option<u64>,
    counts: Option<(usize, i64, i64)>,
    error: Option<String>,
    /// Jobs a retry press queued again on a continued run.
    retried_jobs: Vec<String>,
    /// Seconds earlier runs of this directory already spent on the course.
    earlier_seconds: u64,
}

/// How a run begins.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Start {
    /// New courses in a new artifacts directory.
    New,
    /// The courses an earlier run of the same directory left unfinished, as the app continues them after a restart.
    Continue,
}

impl CourseRun {
    /// A course of the run being continued, from the files that run wrote.
    fn earlier(directory: &std::path::Path, name: String, goal: String) -> Self {
        let read = |file: &str| -> Option<serde_json::Value> {
            serde_json::from_slice(&std::fs::read(directory.join(file)).ok()?).ok()
        };
        let ids = read("module.json").and_then(|module| {
            let id = |key: &str| module.get(key)?.as_str().map(str::to_owned);
            Some((id("programId")?, id("moduleId")?))
        });
        let summary = read("course-summary.json").unwrap_or(serde_json::Value::Null);
        let number = |key: &str| summary.get(key).and_then(|value| value.as_u64());
        let counts = match (
            number("sources"),
            summary.get("modules").and_then(|value| value.as_i64()),
            summary.get("lessons").and_then(|value| value.as_i64()),
        ) {
            (Some(sources), Some(modules), Some(lessons)) => {
                usize::try_from(sources).ok().map(|s| (s, modules, lessons))
            }
            _ => None,
        };
        let error = ids
            .is_none()
            .then(|| "the earlier run prepared no module for this course".to_owned());
        let (program_id, module_id) = match ids {
            Some((program_id, module_id)) => (Some(program_id), Some(module_id)),
            None => (None, None),
        };
        Self {
            name,
            goal,
            program_id,
            module_id,
            review: summary
                .get("reviewStatus")
                .and_then(|value| value.as_str())
                .map(str::to_owned),
            manual_repairs: number("manualRepairs")
                .and_then(|repairs| usize::try_from(repairs).ok())
                .unwrap_or(0),
            generation_seconds: number("generationSeconds").unwrap_or(0),
            outline_seconds: number("outlineAcceptedSeconds"),
            counts,
            error,
            retried_jobs: Vec::new(),
            earlier_seconds: number("totalSeconds").unwrap_or(0),
        }
    }
}

/// Presses "Retry lesson preparation" once on each lesson whose latest attempt failed.
async fn retry_failed_lessons(container: &Container, run: &mut CourseRun) {
    use crate::features::learning::curriculum::{
        LearningGenerationJobKind, LearningGenerationJobStatus,
    };
    let Some(program_id) = run.program_id.clone() else {
        return;
    };
    let result: Result<Vec<String>> = async {
        let jobs = LearningCurriculumRepository::new(container.database().clone())
            .jobs(&program_id)
            .await?;
        let mut retried = Vec::new();
        for job in jobs.iter().filter(|job| {
            job.kind == LearningGenerationJobKind::LessonPreparation
                && job.status == LearningGenerationJobStatus::Failed
                && !jobs
                    .iter()
                    .any(|other| other.retry_of_job_id.as_deref() == Some(job.id.as_str()))
        }) {
            let revision = LearningRepository::new(container.database().clone())
                .get(&program_id)
                .await?
                .summary
                .revision;
            plugin::retry_generation_job(
                container,
                crate::features::learning::plan_dto::LearningGenerationJobActionRequestDto {
                    operation_id: uuid::Uuid::new_v4().to_string(),
                    program_id: program_id.clone(),
                    job_id: job.id.clone(),
                    expected_revision: revision,
                },
            )
            .await
            .map_err(api)?;
            retried.push(job.id.clone());
        }
        Ok(retried)
    }
    .await;
    match result {
        Ok(retried) => run.retried_jobs.extend(retried),
        Err(error) => run.error = Some(format!("retrying failed lessons: {error}")),
    }
    println!(
        "LIVE COURSE {} RETRIED: {} lessons",
        run.name,
        run.retried_jobs.len()
    );
}

/// The requests of `LATTICE_COURSE_REQUESTS`: every `*.json`, named by its stem, in file-name order.
fn requests() -> Result<Vec<(String, GenerateLearningProgramRequestDto)>> {
    let mut paths: Vec<_> = std::fs::read_dir(required("LATTICE_COURSE_REQUESTS")?)?
        .collect::<std::io::Result<Vec<_>>>()?
        .into_iter()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .collect();
    paths.sort();
    if paths.is_empty() {
        return Err(AppError::InvalidConfig(
            "LATTICE_COURSE_REQUESTS holds no request JSON files".into(),
        ));
    }
    let mut found = Vec::new();
    for path in paths {
        let request: GenerateLearningProgramRequestDto =
            serde_json::from_slice(&std::fs::read(&path)?)?;
        assert!(
            request.document_ids.is_empty() && request.source_urls.is_empty(),
            "this test covers topic-only courses"
        );
        let name = path
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_default();
        found.push((name, request));
    }
    Ok(found)
}

/// One course as a learner runs it: outline, repairs, accept, prepare module 1. Its error stays its own.
async fn course(
    container: Container,
    directory: std::path::PathBuf,
    name: String,
    request: GenerateLearningProgramRequestDto,
    manual_repairs_allowed: usize,
    started: std::time::Instant,
) -> CourseRun {
    let mut run = CourseRun {
        name,
        goal: request.goal.clone(),
        program_id: None,
        module_id: None,
        review: None,
        manual_repairs: 0,
        generation_seconds: 0,
        outline_seconds: None,
        counts: None,
        error: None,
        retried_jobs: Vec::new(),
        earlier_seconds: 0,
    };
    if let Err(error) = course_steps(
        &container,
        &directory,
        request,
        manual_repairs_allowed,
        started,
        &mut run,
    )
    .await
    {
        println!("LIVE COURSE {} FAILED: {error}", run.name);
        run.error = Some(error.to_string());
    }
    run
}

async fn course_steps(
    container: &Container,
    directory: &std::path::Path,
    request: GenerateLearningProgramRequestDto,
    manual_repairs_allowed: usize,
    started: std::time::Instant,
    run: &mut CourseRun,
) -> Result<()> {
    std::fs::create_dir_all(directory)?;
    let mut program = plugin::generate_outline(container, request, None, report)
        .await
        .map_err(api)?;
    run.program_id = Some(program.summary.id.clone());
    run.review = review_status(&program).map(|status| format!("{status:?}"));
    record(directory, "generated", &program)?;
    plan_topics(container, &program.summary.id).await;
    run.generation_seconds = started.elapsed().as_secs();

    while review_status(&program) != Some(LearningOutlineReviewStatus::Passed)
        && run.manual_repairs < manual_repairs_allowed
    {
        run.manual_repairs += 1;
        program = plugin::repair_outline(
            container,
            RepairLearningOutlineRequestDto {
                program_id: program.summary.id.clone(),
                expected_revision: program.summary.revision,
            },
            None,
            report,
        )
        .await
        .map_err(api)?;
        run.review = review_status(&program).map(|status| format!("{status:?}"));
        record(
            directory,
            &format!("repair-{}", run.manual_repairs),
            &program,
        )?;
        plan_topics(container, &program.summary.id).await;
    }
    if review_status(&program) != Some(LearningOutlineReviewStatus::Passed) {
        return Err(AppError::InternalError(format!(
            "The outline did not pass review after {} manual repairs; see outline-*.json",
            run.manual_repairs
        )));
    }

    // "Start this course": accept, then prepare module 1, or every module with
    // LATTICE_COURSE_ALL_MODULES=1.
    let accepted = plugin::accept_program(
        container,
        AcceptLearningProgramRequestDto {
            program_id: program.summary.id.clone(),
            expected_revision: program.summary.revision,
            title: program.summary.title.chars().take(120).collect(),
        },
    )
    .await
    .map_err(api)?;
    if accepted.summary.status != LearningProgramStatus::Active {
        return Err(AppError::InternalError(format!(
            "accepting left the course {:?}, not active",
            accepted.summary.status
        )));
    }
    run.outline_seconds = Some(started.elapsed().as_secs());
    run.counts = Some((
        accepted.sources.len(),
        accepted.summary.module_count,
        accepted.summary.lesson_count,
    ));
    let first = accepted
        .modules
        .first()
        .ok_or_else(|| AppError::InternalError("the accepted course has no module".into()))?;
    let module_id = first.id.clone();
    std::fs::write(
        directory.join("module.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "programId": accepted.summary.id,
            "moduleId": module_id,
            "lessonIds": first.lessons.iter().map(|lesson| lesson.id.clone()).collect::<Vec<_>>(),
        }))?,
    )?;
    let modules: Vec<String> =
        if std::env::var("LATTICE_COURSE_ALL_MODULES").is_ok_and(|value| value == "1") {
            accepted
                .modules
                .iter()
                .map(|module| module.id.clone())
                .collect()
        } else {
            vec![module_id.clone()]
        };
    let mut prepared = accepted;
    for module in modules {
        prepared = plugin::prepare_module(
            container,
            PrepareLearningModuleRequestDto {
                program_id: prepared.summary.id.clone(),
                module_id: module,
                expected_revision: prepared.summary.revision,
            },
        )
        .await
        .map_err(api)?;
    }
    run.module_id = Some(module_id);
    Ok(())
}

#[test]
#[ignore = "requires LATTICE_COURSE_SETTINGS, LATTICE_LEARNING_PYTHON, LATTICE_COURSE_REQUESTS and a new LATTICE_COURSE_ARTIFACTS directory; makes real model and web requests"]
fn live_topic_courses_pass_review_and_prepare_their_first_modules() -> Result<()> {
    let runtime = crate::bootstrap::async_runtime::install()?;
    runtime.block_on(courses_in_one_process(Start::New))
}

#[test]
#[ignore = "requires the LATTICE_COURSE_* variables of the run it continues, with LATTICE_COURSE_ARTIFACTS that run's own directory; makes real model and web requests"]
fn live_courses_an_earlier_run_left_unfinished_finish_preparing() -> Result<()> {
    let runtime = crate::bootstrap::async_runtime::install()?;
    runtime.block_on(courses_in_one_process(Start::Continue))
}

#[test]
fn a_continued_course_takes_its_ids_and_outline_figures_from_the_earlier_runs_files() -> Result<()>
{
    let dir = tempfile::tempdir()?;
    std::fs::write(
        dir.path().join("module.json"),
        r#"{"programId":"program-1","moduleId":"module-1","lessonIds":[]}"#,
    )?;
    std::fs::write(
        dir.path().join("course-summary.json"),
        serde_json::to_vec(&serde_json::json!({
            "generationSeconds": 40,
            "outlineAcceptedSeconds": 55,
            "manualRepairs": 1,
            "sources": 3,
            "modules": 2,
            "lessons": 9,
            "reviewStatus": "Passed",
            "totalSeconds": 600,
        }))?,
    )?;
    let run = CourseRun::earlier(dir.path(), "course".to_owned(), "a goal".to_owned());
    assert_eq!(run.program_id.as_deref(), Some("program-1"));
    assert_eq!(run.module_id.as_deref(), Some("module-1"));
    assert_eq!(run.goal, "a goal");
    assert_eq!(run.generation_seconds, 40);
    assert_eq!(run.outline_seconds, Some(55));
    assert_eq!(run.manual_repairs, 1);
    assert_eq!(run.counts, Some((3, 2, 9)));
    assert_eq!(run.review.as_deref(), Some("Passed"));
    assert_eq!(run.earlier_seconds, 600);
    assert!(run.error.is_none());
    Ok(())
}

#[test]
fn a_continued_course_with_no_module_file_says_so() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let run = CourseRun::earlier(dir.path(), "course".to_owned(), "a goal".to_owned());
    assert!(run.program_id.is_none());
    assert!(run.module_id.is_none());
    assert_eq!(
        run.error.as_deref(),
        Some("the earlier run prepared no module for this course")
    );
    Ok(())
}

#[test]
#[ignore = "requires LATTICE_BROWSER_PROBE_URL, a page that refuses the HTTP client; makes a real web request"]
#[cfg(any(windows, target_os = "linux"))]
fn the_headless_browser_reads_a_page_the_http_client_is_refused() -> Result<()> {
    use lattice_core::application::contracts::web::ReadVia;
    use lattice_core::application::ports::WebServiceTrait;
    let runtime = crate::bootstrap::async_runtime::install()?;
    runtime.block_on(async {
        let url = required("LATTICE_BROWSER_PROBE_URL")?;
        let dir = tempfile::tempdir()?;
        let container = crate::interfaces::di::headless::open(
            dir.path(),
            &lattice_core::application::contracts::settings::SettingsDto::default(),
            None,
        )
        .await?;
        container.attach_browser(&crate::interfaces::di::headless::browser_app()?);
        let page = container.web_service().fetch_url_content(&url).await?;
        println!(
            "browser probe: {:?}, read via {:?}, {}",
            page.title,
            page.read_via,
            page.content.chars().take(200).collect::<String>()
        );
        if page.content.trim().is_empty() {
            return Err(AppError::InternalError("the page read with no text".into()));
        }
        if page.read_via != ReadVia::Browser {
            return Err(AppError::InternalError(format!(
                "the page was read {:?}, not through the browser",
                page.read_via
            )));
        }
        Ok(())
    })
}

async fn courses_in_one_process(start: Start) -> Result<()> {
    let directory = std::path::PathBuf::from(required("LATTICE_COURSE_ARTIFACTS")?);
    std::fs::create_dir_all(&directory)?;
    // A stop request belongs to the segment it ended.
    let stop_file = directory.join("stop");
    if stop_file.exists() {
        std::fs::remove_file(&stop_file)?;
    }
    let _log = lattice_core::shared::testing::logs::write_process_log(&directory.join("app.log"))?;
    let started_at = chrono::Utc::now();
    let requests = requests()?;
    // Manual Repair presses allowed per course after generation. Zero proves
    // that generation alone produces an outline the learner can accept.
    let manual_repairs_allowed: usize = std::env::var("LATTICE_COURSE_MANUAL_REPAIRS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(0);
    let retry_failed = start == Start::Continue
        && std::env::var("LATTICE_COURSE_RETRY_FAILED").is_ok_and(|value| value == "1");
    let replay_from = std::env::var("LATTICE_COURSE_REPLAY_FROM");
    if start == Start::Continue && replay_from.is_ok() {
        return Err(AppError::InvalidConfig(
            "a continued run records its calls: unset LATTICE_COURSE_REPLAY_FROM".into(),
        ));
    }
    let log = match replay_from {
        Ok(earlier) => CallLog::Replay {
            from: std::path::PathBuf::from(earlier).join("calls"),
            into: directory.join("calls"),
            misses: if std::env::var_os("LATTICE_COURSE_REPLAY_STRICT").is_some() {
                call_log::Misses::Fail
            } else {
                call_log::Misses::GoLive
            },
        },
        Err(_) => CallLog::Record(directory.join("calls")),
    };
    let calls = log.open()?;

    // The app's own settings reader, on a copy of its settings file.
    let settings = crate::features::settings::repository::SettingsRepository::new(
        std::path::PathBuf::from(required("LATTICE_COURSE_SETTINGS")?),
    )
    .await?
    .get_all()
    .await?;
    if settings.vault.enabled {
        return Err(AppError::InvalidConfig(
            "a run keeps out of the real vault: leave vault.enabled off".into(),
        ));
    }
    let data_dir = directory.join("app-data");
    let has_database = data_dir.join("lattice.db").exists();
    if start == Start::New && has_database {
        return Err(AppError::InvalidConfig(format!(
            "{} already holds a database; use a new artifacts directory",
            data_dir.display()
        )));
    }
    if start == Start::Continue && !has_database {
        return Err(AppError::InvalidConfig(format!(
            "{} holds no database to continue",
            data_dir.display()
        )));
    }
    std::fs::create_dir_all(&data_dir)?;
    let container = crate::interfaces::di::headless::open(&data_dir, &settings, None)
        .await?
        .with_calls(calls.clone());
    #[cfg(any(windows, target_os = "linux"))]
    container.attach_browser(&crate::interfaces::di::headless::browser_app()?);
    drop(settings);

    // What the app's startup does for lesson preparation: the web plugin's
    // finders and snapshot jobs, and the learning plugin's job kinds on its
    // bundled Python runtime, which must verify.
    crate::features::web::plugin::start(&container).await;
    let python = plugin::verified_python(std::path::PathBuf::from(required(
        "LATTICE_LEARNING_PYTHON",
    )?))
    .ok_or_else(|| {
        AppError::InvalidConfig(
            "the Python bundle in LATTICE_LEARNING_PYTHON does not verify".into(),
        )
    })?;
    let runtimes = BuiltinRuntimes::new(Some(python));
    let practice_kinds = plugin::kinds_of(&runtimes)?;
    let registrations = plugin::job_kinds(&container, runtimes, practice_kinds);
    let mut kind_names: Vec<String> = registrations.iter().map(|r| r.kind.clone()).collect();
    kind_names.push(container.web_page_snapshots().job_kind().kind);
    let failures = container.jobs().register_all(registrations).await;
    if !failures.is_empty() {
        return Err(AppError::InternalError(format!(
            "job kinds failed to register: {failures:?}"
        )));
    }
    let kinds: Vec<&str> = kind_names.iter().map(String::as_str).collect();

    let model_name = container
        .learning_llm(ModelRequirements::default())
        .await?
        .llm
        .model_name()
        .to_owned();
    let started = std::time::Instant::now();

    let mut runs = Vec::new();
    if start == Start::New {
        // Every course starts at once, as a learner starting each.
        let handles: Vec<_> = requests
            .into_iter()
            .map(|(name, request)| {
                tauri::async_runtime::spawn(course(
                    container.clone(),
                    directory.join(&name),
                    name,
                    request,
                    manual_repairs_allowed,
                    started,
                ))
            })
            .collect();
        for handle in handles {
            runs.push(handle.await.map_err(|error| {
                AppError::InternalError(format!("a course task ended without its result: {error}"))
            })?);
        }
    } else {
        // Registering the job kinds resumed the suspended work.
        for (name, request) in requests {
            let mut run = CourseRun::earlier(&directory.join(&name), name, request.goal);
            println!(
                "LIVE COURSE {} CONTINUED: module {:?}",
                run.name, run.module_id
            );
            if retry_failed {
                retry_failed_lessons(&container, &mut run).await;
            }
            runs.push(run);
        }
    }

    // Every job of every kind has ended, or a `stop` file asks to end the run
    // in order. A job another one queues exists before the one that queued it ends.
    let mut stopped = false;
    loop {
        let jobs = all_jobs(&container, &kinds).await?;
        if jobs.iter().all(ended) {
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            if all_jobs(&container, &kinds).await?.len() == jobs.len() {
                break;
            }
        }
        if stop_file.exists() {
            stopped = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
    }
    container.jobs().close();
    container.jobs().drain().await;

    let mut courses = Vec::new();
    for run in &runs {
        let course_dir = directory.join(&run.name);
        let lessons = match (&run.program_id, &run.module_id) {
            (Some(program_id), Some(module_id)) => {
                lesson_end_states(&container, program_id, module_id).await?
            }
            _ => serde_json::json!([]),
        };
        std::fs::write(
            course_dir.join("lessons.json"),
            serde_json::to_vec_pretty(&lessons)?,
        )?;
        std::fs::write(
            course_dir.join("course-summary.json"),
            serde_json::to_vec_pretty(&serde_json::json!({
                "goal": run.goal,
                "generationSeconds": run.generation_seconds,
                "outlineAcceptedSeconds": run.outline_seconds,
                "totalSeconds": run.earlier_seconds + started.elapsed().as_secs(),
                "reviewStatus": run.review,
                "manualRepairs": run.manual_repairs,
                "sources": run.counts.map(|counts| counts.0),
                "modules": run.counts.map(|counts| counts.1),
                "lessons": run.counts.map(|counts| counts.2),
                "moduleLessons": lessons,
                "error": run.error,
                "retriedJobs": run.retried_jobs,
            }))?,
        )?;
        courses.push(serde_json::json!({
            "name": run.name,
            "programId": run.program_id,
            "moduleId": run.module_id,
            "reviewStatus": run.review,
            "manualRepairs": run.manual_repairs,
            "error": run.error,
        }));
    }
    let jobs: Vec<_> = all_jobs(&container, &kinds)
        .await?
        .iter()
        .map(job_json)
        .collect();
    std::fs::write(
        directory.join("jobs.json"),
        serde_json::to_vec_pretty(&jobs)?,
    )?;
    let run_file = directory.join("run.json");
    if run_file.exists() {
        let earlier = std::fs::read_dir(&directory)?
            .collect::<std::io::Result<Vec<_>>>()?
            .iter()
            .filter(|entry| {
                let file = entry.file_name().to_string_lossy().into_owned();
                file.starts_with("run-") && file.ends_with(".json")
            })
            .count();
        std::fs::rename(
            &run_file,
            directory.join(format!("run-{}.json", earlier + 1)),
        )?;
    }
    std::fs::write(
        run_file,
        serde_json::to_vec_pretty(&serde_json::json!({
            "continued": start == Start::Continue,
            "model": model_name,
            "startedAt": started_at.to_rfc3339(),
            "endedAt": chrono::Utc::now().to_rfc3339(),
            "commit": std::env::var("LATTICE_COURSE_COMMIT").ok(),
            "stopped": stopped,
            "replayFrom": std::env::var("LATTICE_COURSE_REPLAY_FROM").ok(),
            "courses": courses,
            "calls": calls.finish()?,
        }))?,
    )?;

    let failed: Vec<String> = runs
        .iter()
        .filter(|run| run.error.is_some() || run.module_id.is_none())
        .map(|run| {
            format!(
                "{} ({})",
                run.name,
                run.error.as_deref().unwrap_or("module not prepared")
            )
        })
        .collect();
    if stopped {
        return Err(AppError::InternalError(
            "stopped at request before every job ended; continue with LATTICE_COURSE_CONTINUE=1"
                .into(),
        ));
    }
    if !failed.is_empty() {
        return Err(AppError::InternalError(format!(
            "courses that failed: {}; see run.json, jobs.json and each course's lessons.json",
            failed.join(", ")
        )));
    }
    println!("LIVE COURSES CONFIRMED: {}", runs.len());
    Ok(())
}
