//! A live, end-to-end course test in a new isolated database. A topic-only
//! request goes through production outline generation, reference gathering,
//! review and repair, acceptance and first-lesson preparation, using the
//! configured model and the real web service. Nothing is mocked or replayed.
use super::*;
use crate::features::learning::patient_model::PatientModel;
use crate::features::learning::{
    curriculum::LearningGenerationJobKind,
    dto::*,
    outline_draft::{
        DraftRunRegistry, LearningOutlineReviewStatus, RepairLearningOutlineRequestDto,
    },
    outline_progress::OutlineRunRegistry,
    plan_dto::StartLearningGenerationJobRequestDto,
};
use crate::features::web::services::web::WebService;

fn review_status(program: &LearningProgramDto) -> Option<LearningOutlineReviewStatus> {
    program
        .outline_review
        .as_ref()
        .map(|review| review.status.clone())
}

/// Every outline state the learner would have seen, for diagnosis after a run.
fn record(directory: &std::path::Path, step: &str, program: &LearningProgramDto) -> Result<()> {
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

#[tokio::test]
#[ignore = "requires LATTICE_LLAMACPP_SETTINGS, LATTICE_COURSE_REQUEST and a new LATTICE_COURSE_ARTIFACTS directory; makes real model and web requests"]
async fn live_topic_course_passes_review_and_prepares_its_first_lesson() -> Result<()> {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("lattice::features::learning=info")
        .try_init();
    let settings: serde_json::Value = serde_json::from_slice(&std::fs::read(
        std::env::var("LATTICE_LLAMACPP_SETTINGS").unwrap(),
    )?)?;
    let config = serde_json::from_value(settings["settings"]["llm"].clone())?;
    let request: GenerateLearningProgramRequestDto = serde_json::from_slice(&std::fs::read(
        std::env::var("LATTICE_COURSE_REQUEST").unwrap(),
    )?)?;
    assert!(
        request.document_ids.is_empty() && request.source_urls.is_empty(),
        "this test covers topic-only courses"
    );
    // Manual Repair presses allowed after generation. Zero proves that
    // generation alone produces an outline the learner can accept.
    let manual_repairs_allowed: usize = std::env::var("LATTICE_COURSE_MANUAL_REPAIRS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(0);
    let directory = std::path::PathBuf::from(std::env::var("LATTICE_COURSE_ARTIFACTS").unwrap());
    std::fs::create_dir_all(&directory)?;
    let database = directory.join("course.sqlite");
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
    let web_directory = directory.join("web");
    std::fs::create_dir_all(&web_directory)?;
    let web = WebService::new(&web_directory)?;
    let repo = LearningRepository::new(pool.clone());
    let drafts = DraftRunRegistry::new();
    let registry = OutlineRunRegistry::new();
    let started = std::time::Instant::now();
    let report =
        |update: crate::features::learning::outline_progress::LearningOutlineProgressDto| {
            println!(
                "Live outline: {:?} at {}s, {} response characters",
                update.stage, update.elapsed_seconds, update.response_characters
            );
        };

    let run = registry.register(None, report)?;
    let progress = &run.progress;
    let mut program = progress
        .run(async {
            progress.model(model.model_name());
            // As the app's command does: a model outage is waited out.
            let patient = PatientModel::new(model.as_ref(), progress.as_ref());
            service::generate_with_references_and_progress(
                &repo,
                &drafts,
                &patient,
                request.clone(),
                vec![],
                &[],
                progress,
                Some(&web),
            )
            .await
        })
        .await?;
    drop(run);
    record(&directory, "generated", &program)?;
    let generation_seconds = started.elapsed().as_secs();

    let mut manual_repairs = 0;
    while review_status(&program) != Some(LearningOutlineReviewStatus::Passed)
        && manual_repairs < manual_repairs_allowed
    {
        manual_repairs += 1;
        let run = registry.register(None, report)?;
        let progress = &run.progress;
        let repair = RepairLearningOutlineRequestDto {
            program_id: program.summary.id.clone(),
            expected_revision: program.summary.revision,
        };
        program = progress
            .run(async {
                let patient = PatientModel::new(model.as_ref(), progress.as_ref());
                crate::features::learning::outline_draft::repair_saved(
                    &repo,
                    &drafts,
                    &patient,
                    &repair,
                    progress,
                    Some(&web),
                )
                .await
            })
            .await?;
        record(&directory, &format!("repair-{manual_repairs}"), &program)?;
    }
    assert_eq!(
        review_status(&program),
        Some(LearningOutlineReviewStatus::Passed),
        "The outline did not pass review after {manual_repairs} manual repairs; see outline-*.json"
    );

    let accepted = service::accept(
        &repo,
        &drafts,
        AcceptLearningProgramRequestDto {
            program_id: program.summary.id.clone(),
            expected_revision: program.summary.revision,
            title: program.summary.title.chars().take(120).collect(),
        },
    )
    .await?;
    assert_eq!(accepted.summary.status, LearningProgramStatus::Active);
    let outline_seconds = started.elapsed().as_secs();
    let lesson_id = accepted.modules[0].lessons[0].id.clone();
    let jobs = LearningCurriculumRepository::new(pool.clone());
    let job = jobs
        .start_job(&StartLearningGenerationJobRequestDto {
            operation_id: uuid::Uuid::new_v4().to_string(),
            program_id: accepted.summary.id.clone(),
            expected_revision: accepted.summary.revision,
            kind: LearningGenerationJobKind::LessonPreparation,
            request_json: serde_json::json!({ "lessonIds": [lesson_id] }).to_string(),
            progress_total: 1,
        })
        .await?;
    std::fs::write(directory.join("job-id.txt"), &job.id)?;
    drop(jobs);
    drop(repo);
    small::run_and_confirm(pool, options, model, &job.id, &directory).await?;
    let summary = serde_json::json!({
        "goal": request.goal,
        "generationSeconds": generation_seconds,
        "outlineAcceptedSeconds": outline_seconds,
        "totalSeconds": started.elapsed().as_secs(),
        "manualRepairs": manual_repairs,
        "sources": accepted.sources.len(),
        "modules": accepted.summary.module_count,
        "lessons": accepted.summary.lesson_count,
    });
    std::fs::write(
        directory.join("course-summary.json"),
        serde_json::to_vec_pretty(&summary)?,
    )?;
    println!("LIVE COURSE CONFIRMED: {summary}");
    Ok(())
}
