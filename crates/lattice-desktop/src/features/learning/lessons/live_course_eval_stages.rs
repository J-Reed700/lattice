//! The course eval's stages (§3.4) and the §1.1 metrics they record: the
//! outline as generated or continued, then each selected lesson's job.
use super::*;
use crate::features::learning::references::course_index::CourseIndexes;

/// The §1.1 outline keys the program and its saved draft decide.
pub(super) fn outline_metrics(
    program: &LearningProgramDto,
    candidate: Option<&Value>,
    quote_failures: usize,
) -> Value {
    use crate::features::learning::outline_linking::{cited_count, item_count};
    let review = program.outline_review.as_ref();
    let mut hosts: BTreeMap<String, usize> = BTreeMap::new();
    for url in program.sources.iter().filter_map(|s| s.url.as_deref()) {
        if let Some(site) = lattice_core::domain::provenance::Origin::of_url(url)
            .ok()
            .and_then(|origin| origin.site().map(str::to_owned))
        {
            *hosts.entry(site).or_default() += 1;
        }
    }
    let hosted: usize = hosts.values().sum();
    let max_share = hosts
        .values()
        .max()
        .map_or(0.0, |most| *most as f64 / hosted as f64);
    let lessons: Vec<&LearningLessonDto> =
        program.modules.iter().flat_map(|m| &m.lessons).collect();
    let fit = lessons
        .iter()
        .filter(|l| l.estimated_minutes <= program.minutes_per_session)
        .count();
    json!({
        "review": {
            "status": review.map_or(LearningOutlineReviewStatus::Unchecked, |r| r.status.clone()),
            "issues": review.map_or(0, |r| r.issues.len()),
            "repair_passes": review.map_or(0, |r| r.repair_passes),
        },
        "references": {"count": program.sources.len(), "domains": hosts.len(), "max_share": max_share},
        "links": {"items": candidate.map(item_count), "linked": candidate.map(cited_count)},
        "citations": {
            "verified": review.map_or(0, |r| r.citations_verified),
            "removed": review.map_or(0, |r| r.citations_removed),
        },
        "quotes": {"checked": candidate.map(quoted_fields), "unmatched": quote_failures},
        "shape": {"modules": program.modules.len(), "lessons": lessons.len(), "session_fit": fit},
    })
}

/// Outline items carrying an author quote, the fields `quote_checks` reads.
fn quoted_fields(candidate: &Value) -> usize {
    let quoted = |field: &Value| {
        field["quote"]
            .as_str()
            .is_some_and(|q| !q.trim().is_empty())
    };
    candidate["modules"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|module| {
            usize::from(quoted(&module["summary"]))
                + ["outcomes", "lessons"]
                    .iter()
                    .flat_map(|name| module[*name].as_array().into_iter().flatten())
                    .filter(|field| quoted(field))
                    .count()
        })
        .sum()
}

/// The §1.1 lesson keys the published lesson and its stored report decide.
pub(super) fn lesson_metrics(
    lesson: &LearningLessonDto,
    report: Option<&Value>,
    known_sources: &HashSet<String>,
) -> Value {
    let cited: BTreeSet<&str> = lesson
        .blocks
        .iter()
        .flat_map(|b| &b.source_ids)
        .map(String::as_str)
        .collect();
    let kinds: BTreeSet<String> = lesson
        .blocks
        .iter()
        .filter_map(|b| serde_json::to_value(&b.kind).ok())
        .filter_map(|kind| kind.as_str().map(str::to_owned))
        .collect();
    let items = |report: &Value, name: &str| report[name].as_array().cloned().unwrap_or_default();
    let claims = report.map(|report| {
        let findings = items(report, "findings");
        let verdict = |name: &str| findings.iter().filter(|f| f["verdict"] == name).count();
        // The provenance gate's test: the quote is a substring of a passage.
        let unmatched = findings
            .iter()
            .filter(|f| f["verdict"] == "supported")
            .filter(|f| {
                let quote = f["supporting_quote"].as_str().unwrap_or_default();
                !quote.is_empty()
                    && !items(f, "evidence")
                        .iter()
                        .any(|p| p["text"].as_str().is_some_and(|text| text.contains(quote)))
            })
            .count();
        json!({
            "total": findings.len(),
            "supported": verdict("supported"),
            "unsupported": verdict("unsupported"),
            "contradicted": verdict("contradicted"),
            "unverified": verdict("unverified"),
            "general_knowledge": findings.iter().filter(|f| f["verdict"] == "unsupported" && !f["resolution"].is_null()).count(),
            "quotes_unmatched": unmatched,
            "issues": items(report, "issues").len(),
        })
    });
    let examples = report.map(|report| {
        let statuses: Vec<Value> = items(report, "executions")
            .iter()
            .map(|e| e["result"]["status"].clone())
            .collect();
        let ran = statuses
            .iter()
            .filter(|s| *s != "runtime_unavailable" && *s != "cancelled")
            .count();
        let failed = statuses
            .iter()
            .filter(|s| *s == "failed" || *s == "timed_out")
            .count();
        json!({"run": ran, "failed": failed})
    });
    json!({
        "shape": {"blocks": lesson.blocks.len(), "kinds": kinds},
        "citations": {"source_ids": cited.len(), "unresolved": cited.iter().filter(|id| !known_sources.contains(**id)).count()},
        "claims": claims,
        "examples": examples,
    })
}

/// Deferrals, repair and deepening rounds, and seconds per phase, from one
/// selector's `phases.jsonl` lines.
pub(super) fn phase_metrics(lines: &[Value], selector: &str) -> Value {
    let rows: Vec<&Value> = lines.iter().filter(|l| l["selector"] == selector).collect();
    let deferrals = rows
        .iter()
        .position(|r| r["status"] == "running")
        .map_or(0, |first| {
            rows.iter()
                .skip(first + 1)
                .filter(|r| r["status"] == "pending")
                .count()
        });
    let (mut repairs, mut pages) = (0, BTreeSet::new());
    let mut seconds: BTreeMap<String, f64> = BTreeMap::new();
    let mut previous = &Value::Null;
    for (index, row) in rows.iter().enumerate() {
        if row["phase"] == "repair" && *previous != "repair" {
            repairs += 1;
        }
        previous = &row["phase"];
        if let Some(page) = row["deeper_page"].as_u64() {
            pages.insert(page);
        }
        if let (true, Some(phase), Some(next)) = (
            row["status"] == "running",
            row["phase"].as_str(),
            rows.get(index + 1),
        ) {
            let span = next["at"].as_i64().unwrap_or(0) - row["at"].as_i64().unwrap_or(0);
            *seconds.entry(phase.to_owned()).or_default() += span as f64 / 1000.0;
        }
    }
    json!({"deferrals": deferrals, "rounds": {"repair": repairs, "deepening": pages.len()}, "phase_seconds": seconds})
}

/// Calls in `calls` and their tokens, from the `call-N.json` files the
/// observed model wrote (a failed call writes none).
fn stage_cost(run: &Path, calls: std::ops::Range<usize>, seconds: f64) -> Result<Value> {
    let (mut count, mut input, mut output, mut peak) = (0u64, 0u64, 0u64, 0u64);
    for call in calls.clone() {
        let Some(record) = read_json(&run.join(format!("call-{call}.json")))? else {
            continue;
        };
        let tokens = record["inputTokens"].as_u64().unwrap_or(0);
        count += 1;
        input += tokens;
        output += record["outputTokens"].as_u64().unwrap_or(0);
        peak = peak.max(tokens);
    }
    // The inclusive `call-N.json` range the stage's calls were numbered in.
    let (first, last) = match calls.end.checked_sub(1) {
        Some(last) if calls.start <= last => (Some(calls.start), Some(last)),
        _ => (None, None),
    };
    Ok(
        json!({"seconds": seconds, "calls": count, "input_tokens": input, "output_tokens": output, "peak_input_tokens": peak, "first_call": first, "last_call": last}),
    )
}

/// The outline stage, then each selected lesson; what done.json records.
pub(super) async fn run_stages(
    run: &mut Run,
    pool: &sqlx::SqlitePool,
    options: &sqlx::sqlite::SqliteConnectOptions,
    request: &GenerateLearningProgramRequestDto,
    selectors: &[String],
) -> Result<(Value, Value)> {
    let accepted = outline_stage(run, pool, request).await?;
    let mut lessons = serde_json::Map::new();
    for selector in selectors {
        let outcome = match &accepted {
            Some(program) => {
                lesson_stage(run, pool, options, &program.summary.id, selector).await?
            }
            // No lesson of an outline that did not pass can be prepared.
            None => "not_run".to_owned(),
        };
        lessons.insert(selector.clone(), json!(outcome));
    }
    let outline = run.metrics["outline"]["review"]["status"].clone();
    Ok((outline, Value::Object(lessons)))
}

/// Generate, continue or accept the outline as §3.4's table says. The
/// accepted course, or `None` when the outline did not pass.
async fn outline_stage(
    run: &mut Run,
    pool: &sqlx::SqlitePool,
    request: &GenerateLearningProgramRequestDto,
) -> Result<Option<LearningProgramDto>> {
    let repo = LearningRepository::new(pool.clone());
    let drafts = DraftRunRegistry::new();
    let ids: Vec<String> = sqlx::query_scalar("SELECT id FROM learning_programs")
        .fetch_all(pool)
        .await
        .map_err(db)?;
    if ids.len() > 1 {
        return Err(refuse("the run database holds more than one course"));
    }
    let saved = match ids.first() {
        Some(id) => Some(repo.get(id).await?),
        None => None,
    };
    let recorded = run.dir.join("outline.json").exists();
    let step = outline_step(
        saved.as_ref().map(|p| {
            (
                &p.summary.status,
                p.outline_review.as_ref().map(|r| &r.status),
            )
        }),
        recorded,
    );
    let first_call = run.calls();
    let started = std::time::Instant::now();
    let resumed = step == OutlineStep::Repair;
    let program = match (step, saved) {
        (OutlineStep::Generate | OutlineStep::Repair, saved) => {
            match draft_outline(run, &repo, &drafts, request, saved.as_ref()).await {
                Ok(program) => program,
                // The pipeline's failure is the stage's outcome, not the harness's.
                Err(error) => {
                    let error = error.to_string();
                    write_json(
                        &run.dir.join("outline.json"),
                        &json!({"status": "failed", "error": error}),
                    )?;
                    let cost = stage_cost(
                        &run.dir,
                        first_call..run.calls(),
                        started.elapsed().as_secs_f64(),
                    )?;
                    run.metrics["outline"] = json!({"review": {"status": "failed"}, "error": error, "cost": cost, "resumed": resumed});
                    run.save()?;
                    return Ok(None);
                }
            }
        }
        (_, Some(program)) => program,
        (_, None) => return Err(refuse("the outline step needs a saved course")),
    };
    if !recorded || matches!(step, OutlineStep::Generate | OutlineStep::Repair) {
        record_outline(run, &repo, &program, first_call, started, resumed).await?;
    }
    if step == OutlineStep::Accepted {
        return Ok(Some(program));
    }
    if course_e2e::review_status(&program) != Some(LearningOutlineReviewStatus::Passed) {
        return Ok(None);
    }
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
    Ok(Some(accepted))
}

/// Production outline generation, or the app's Repair continuation of a
/// saved draft, with no manual Repair presses.
async fn draft_outline(
    run: &Run,
    repo: &LearningRepository,
    drafts: &DraftRunRegistry,
    request: &GenerateLearningProgramRequestDto,
    saved: Option<&LearningProgramDto>,
) -> Result<LearningProgramDto> {
    let registry = OutlineRunRegistry::new();
    let report =
        |update: crate::features::learning::outline_progress::LearningOutlineProgressDto| {
            println!(
                "Course eval outline: {:?} at {}s, {} response characters",
                update.stage, update.elapsed_seconds, update.response_characters
            );
        };
    let outline_run = registry.register(None, report)?;
    let progress = &outline_run.progress;
    let model = run.model.as_ref();
    let web: &dyn WebServiceTrait = run.web.as_ref();
    progress
        .run(async {
            progress.model(model.model_name());
            // As the app's command does: a model outage is waited out.
            let patient = PatientModel::new(model, progress.as_ref());
            match saved {
                None => {
                    service::generate_with_references_and_progress(
                        repo,
                        drafts,
                        &patient,
                        request.clone(),
                        vec![],
                        &[],
                        progress,
                        Some(web),
                    )
                    .await
                }
                Some(program) => {
                    let repair = RepairLearningOutlineRequestDto {
                        program_id: program.summary.id.clone(),
                        expected_revision: program.summary.revision,
                    };
                    crate::features::learning::outline_draft::repair_saved(
                        repo,
                        drafts,
                        &patient,
                        &repair,
                        progress,
                        Some(web),
                    )
                    .await
                }
            }
        })
        .await
}

/// `outline-generated.json`, `outline.json` and the outline's metrics,
/// read while the draft is still saved.
async fn record_outline(
    run: &mut Run,
    repo: &LearningRepository,
    program: &LearningProgramDto,
    first_call: usize,
    started: std::time::Instant,
    resumed: bool,
) -> Result<()> {
    use crate::features::learning::outline_linking::{cited_count, item_count};
    let draft = repo.outline_draft(&program.summary.id).await?;
    let saved = repo.verification_sources(&program.summary.id).await?;
    let (candidate, failures, covering) = match &draft {
        Some(draft) => {
            let sources: Vec<LearningSourceDto> = draft
                .source_ids
                .iter()
                .filter_map(|id| saved.iter().find(|s| &s.id == id).cloned())
                .collect();
            let failures = crate::features::learning::outline_evidence::quote_checks(
                &draft.candidate,
                &sources,
            );
            let covering = draft
                .citation_receipts
                .values()
                .filter(|r| r.covers)
                .count();
            (Some(&draft.candidate), failures, Some(covering))
        }
        None => (None, Vec::new(), None),
    };
    course_e2e::record(&run.dir, "generated", program)?;
    let generated = run.dir.join("outline-generated.json");
    if let Some(mut file) = read_json(&generated)? {
        // The run's LinkLog is folded into the review counters and not kept;
        // the saved candidate and receipts say the same of the final outline.
        file["linkLog"] = json!({"items": candidate.map(item_count), "linked": candidate.map(cited_count), "receiptsCovering": covering});
        file["quoteChecks"] = json!(failures);
        write_json(&generated, &file)?;
    }
    let review = program.outline_review.as_ref();
    write_json(
        &run.dir.join("outline.json"),
        &json!({
            "status": review.map_or(LearningOutlineReviewStatus::Unchecked, |r| r.status.clone()),
            "issues": review.map(|r| &r.issues),
            "repair_passes": review.map_or(0, |r| r.repair_passes),
            "sources": program.sources.iter().map(|s| json!({"id": s.id, "title": s.title, "url": s.url})).collect::<Vec<_>>(),
            "modules": program.modules.iter().map(|m| json!({
                "id": m.id, "title": m.title, "summary": m.summary, "outcomes": m.outcomes, "project": m.project,
                "lessons": m.lessons.iter().map(|l| json!({"id": l.id, "title": l.title, "objective": l.objective, "estimated_minutes": l.estimated_minutes})).collect::<Vec<_>>(),
            })).collect::<Vec<_>>(),
        }),
    )?;
    let mut metrics = outline_metrics(program, candidate, failures.len());
    metrics["cost"] = stage_cost(
        &run.dir,
        first_call..run.calls(),
        started.elapsed().as_secs_f64(),
    )?;
    metrics["resumed"] = json!(resumed);
    run.metrics["outline"] = metrics;
    run.save()
}

/// One selected lesson, started, continued or retried as §3.4's table says,
/// and recorded once its job settles. Returns its outcome.
async fn lesson_stage(
    run: &mut Run,
    pool: &sqlx::SqlitePool,
    options: &sqlx::sqlite::SqliteConnectOptions,
    program_id: &str,
    selector: &str,
) -> Result<String> {
    use crate::features::learning::curriculum_repository::LESSON_PREPARATION;
    use lattice_core::shared::runtime::jobs::{JobStore, RecoveryPolicy};
    use std::io::Write;
    let lesson_file = run.dir.join(format!("lesson-{selector}.json"));
    let job_dir = run.dir.join("jobs").join(selector);
    std::fs::create_dir_all(&job_dir)?;
    let job_id_file = job_dir.join("job-id.txt");
    let jobs = LearningCurriculumRepository::new(pool.clone());
    let saved = match std::fs::read_to_string(&job_id_file) {
        Ok(id) => Some(jobs.job(id.trim()).await?),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(error.into()),
    };
    let step = lesson_step(
        lesson_file.exists(),
        saved.as_ref().map(|job| (job.status, job.error.as_deref())),
    );
    if step == LessonStep::Recorded {
        let recorded = read_json(&lesson_file)?.unwrap_or_default();
        return Ok(recorded["outcome"].as_str().unwrap_or("unknown").to_owned());
    }
    let (module, position) = parse_selector(selector)?;
    let program = LearningRepository::new(pool.clone())
        .get(program_id)
        .await?;
    let Some(lesson_id) = program
        .modules
        .get(module)
        .and_then(|m| m.lessons.get(position))
        .map(|l| l.id.clone())
    else {
        let error = format!("the outline has no lesson {selector}");
        run.metrics["lessons"][selector] = json!({"outcome": "failed", "error": error});
        run.save()?;
        write_json(
            &lesson_file,
            &json!({"selector": selector, "lesson_id": null, "outcome": "failed", "error": error}),
        )?;
        return Ok("failed".into());
    };
    let restart = match (step, &saved) {
        (LessonStep::Start, _) => {
            let request = StartLearningGenerationJobRequestDto {
                operation_id: uuid::Uuid::new_v4().to_string(),
                program_id: program_id.to_owned(),
                expected_revision: program.summary.revision,
                kind: LearningGenerationJobKind::LessonPreparation,
                request_json: json!({ "lessonIds": [lesson_id] }).to_string(),
                progress_total: 1,
            };
            Some(jobs.start_job(&request).await?.id)
        }
        (LessonStep::Dispatch, Some(job)) => {
            // As native startup does: a job left running is requeued.
            JobStore::new(pool.clone())
                .recover(LESSON_PREPARATION, RecoveryPolicy::Requeue)
                .await?;
            Some(job.id.clone())
        }
        (LessonStep::Retry, Some(job)) => {
            let request = LearningGenerationJobActionRequestDto {
                operation_id: uuid::Uuid::new_v4().to_string(),
                program_id: program_id.to_owned(),
                job_id: job.id.clone(),
                expected_revision: program.summary.revision,
            };
            Some(jobs.retry_job(&request).await?.id)
        }
        _ => None,
    };
    let first_call = run.calls();
    let started = std::time::Instant::now();
    let job = match restart {
        Some(id) => {
            std::fs::write(&job_id_file, &id)?;
            let phases = run.dir.join("phases.jsonl");
            let last = std::sync::Mutex::new(None::<Value>);
            let model = run.model.clone();
            let observe = |job: &LearningGenerationJob| -> Result<()> {
                let activity = job.activity.as_ref();
                let state = json!({
                    "status": job.status, "phase": activity.map(|a| a.phase), "message": job.progress_message,
                    "deeper_page": activity.and_then(|a| a.deeper_evidence.as_ref()).map(|d| d.page),
                });
                let mut last = last
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                if last.as_ref() == Some(&state) {
                    return Ok(());
                }
                let mut line = state.clone();
                line["at"] = json!(now_ms());
                line["selector"] = json!(selector);
                line["calls"] = json!(model.call.load(Ordering::SeqCst));
                let mut file = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&phases)?;
                writeln!(file, "{line}")?;
                *last = Some(state);
                Ok(())
            };
            // As the app wires it: one runtime runs the lesson and the course
            // index builds, and the worker searches the course's index.
            let runtime = lattice_core::shared::runtime::jobs::JobRuntime::new(pool.clone());
            let courses = CourseIndexes::new(
                run.dir.join("course-index"),
                pool.clone(),
                Arc::new(super::super::page_recall::GivenModel(Some(
                    run.embedder.clone(),
                ))),
                Arc::new(crate::features::search::trait_def::HybridSearchUseCaseFactory),
                Arc::clone(&runtime),
                Arc::new(lattice_core::shared::runtime::background::BackgroundTasks::default()),
                Arc::default(),
            );
            courses.register().await?;
            let worker = small::worker(pool, run.model.clone(), run.web.clone(), Some(courses));
            small::run_job(runtime, worker, &id, &job_dir, &observe).await?
        }
        None => {
            let job =
                saved.ok_or_else(|| refuse(format!("lesson {selector} has no job to record")))?;
            write_json(&job_dir.join("result.json"), &serde_json::to_value(&job)?)?;
            job
        }
    };
    let seconds = started.elapsed().as_secs_f64();

    // Read what was published from a new connection, as the app would.
    let reopened = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options.clone())
        .await
        .map_err(db)?;
    let library = LearningRepository::new(reopened.clone());
    let lesson = library
        .get(program_id)
        .await?
        .modules
        .get(module)
        .and_then(|m| m.lessons.get(position))
        .cloned()
        .ok_or_else(|| refuse(format!("lesson {selector} is no longer in the course")))?;
    let report: Option<String> = sqlx::query_scalar(
        "SELECT report_json FROM learning_lesson_verifications WHERE program_id=? AND lesson_id=?",
    )
    .bind(program_id)
    .bind(&lesson.id)
    .fetch_optional(&reopened)
    .await
    .map_err(db)?;
    let report: Option<Value> = report.as_deref().map(serde_json::from_str).transpose()?;
    // Answer keys are kept apart from learner DTOs.
    let keys: Vec<(String, i64, String)> = sqlx::query_as(
        "SELECT k.question_id,k.correct_index,k.explanation FROM learning_answer_keys k JOIN learning_questions q ON q.id=k.question_id WHERE q.lesson_id=?",
    )
    .bind(&lesson.id)
    .fetch_all(&reopened)
    .await
    .map_err(db)?;
    let sources = library.verification_sources(program_id).await?;
    reopened.close().await;

    use LearningGenerationJobStatus as Status;
    let (outcome, error) = match job.status {
        Status::Completed if lesson.preparation == LearningPreparation::Ready => ("ready", None),
        Status::Completed => (
            "failed",
            Some("the job completed but the lesson is not ready".to_owned()),
        ),
        Status::Interrupted | Status::Pending | Status::Running => {
            ("interrupted", job.error.clone())
        }
        Status::Failed | Status::Cancelled => ("failed", job.error.clone()),
    };
    let known: HashSet<String> = sources.iter().map(|s| s.id.clone()).collect();
    let lines: Vec<Value> = match std::fs::read_to_string(run.dir.join("phases.jsonl")) {
        Ok(text) => text
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(serde_json::from_str::<Value>)
            .collect::<std::result::Result<_, _>>()?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(error) => return Err(error.into()),
    };
    let phases = phase_metrics(&lines, selector);
    let mut metrics = lesson_metrics(&lesson, report.as_ref(), &known);
    metrics["outcome"] = json!(outcome);
    metrics["error"] = json!(error);
    metrics["deferrals"] = phases["deferrals"].clone();
    metrics["rounds"] = phases["rounds"].clone();
    metrics["cost"] = stage_cost(&run.dir, first_call..run.calls(), seconds)?;
    metrics["cost"]["phase_seconds"] = phases["phase_seconds"].clone();
    metrics["resumed"] = json!(step != LessonStep::Start);
    run.metrics["lessons"][selector] = metrics;
    run.save()?;

    let mut published = serde_json::to_value(&lesson)?;
    for question in published["questions"].as_array_mut().into_iter().flatten() {
        let id = question["id"].as_str().unwrap_or_default().to_owned();
        let key = keys.iter().find(|(question_id, ..)| *question_id == id);
        question["correctIndex"] = json!(key.map(|(_, index, _)| index));
        question["explanation"] = json!(key.map(|(.., explanation)| explanation));
    }
    // Written last: its presence is what marks the lesson recorded.
    write_json(
        &lesson_file,
        &json!({
            "selector": selector, "lesson_id": lesson.id, "outcome": outcome, "error": error,
            "lesson": published, "report": report,
            "sources": sources.iter().map(|s| json!({"id": s.id, "title": s.title, "url": s.url, "content_sha256": sha256_hex(s.excerpt.as_bytes())})).collect::<Vec<_>>(),
        }),
    )?;
    Ok(outcome.to_owned())
}
