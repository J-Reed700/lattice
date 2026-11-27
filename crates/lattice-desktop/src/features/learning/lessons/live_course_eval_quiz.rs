//! The course eval's quiz hooks (docs/design/2026-10-23-quizzes-and-tests.md
//! §7; protocol in evals/courses/README.md):
//!
//! - the `quiz` stage: after a lesson is recorded Ready, run the quiz job its
//!   publication queued through the app's `QuizBankWorker` on the job runtime,
//!   top up what that left missing with the app's authoring path, and record
//!   the bank, keys included, as `quiz-<sel>.json` (`course-eval-quiz-bank/1`)
//!   with the numbers of both steps;
//! - the grading set: grade every answer of a committed set with the app's own
//!   graders and write `quiz-grading-grades/1`, the model's raw output kept
//!   beside the checked rationale, with three regrades of each contested
//!   answer.
//!
//! Both record outcomes and never assert on one; `scripts/course_eval.py` and
//! `scripts/course_eval_grading.py` score what they write.
use super::*;
use crate::features::learning::assessment_engine::{
    LearningAssessmentCandidate, LearningAssessmentKey, LearningAssessmentResponse,
    LearningItemFormat, LearningNumericKey, LearningRubricCriterion, LearningTextKey,
};
use crate::features::learning::assessment_repository::{
    LearningAssessmentRepository, StoredCandidate,
};
use crate::features::learning::grade_rationale;
use crate::features::learning::{quiz_bank_jobs, quiz_blueprints};
use lattice_core::shared::persistence::Database;
use lattice_core::shared::runtime::jobs::{JobRuntime, JobStore, RecoveryPolicy};

#[path = "live_course_eval_quiz_grading.rs"]
mod grading;
#[cfg(test)]
pub(super) use grading::grading_set;

const BANK_SCHEMA: &str = "course-eval-quiz-bank/1";

// --- the quiz stage ----------------------------------------------------------

/// Write and record a Ready lesson's bank. A recorded `quiz-<sel>.json` means
/// the stage is done; a failure is recorded in `metrics.json` as
/// `quiz_error` without a file, so the next run of the request retries it.
pub(super) async fn quiz_stage(
    run: &mut Run,
    database: &Database,
    program_id: &str,
    selector: &str,
) -> Result<()> {
    let file = run.dir.join(format!("quiz-{selector}.json"));
    if file.exists() {
        return Ok(());
    }
    let recorded = read_json(&run.dir.join(format!("lesson-{selector}.json")))?.unwrap_or_default();
    let Some(lesson_id) = recorded["lesson_id"].as_str().map(str::to_owned) else {
        return Ok(());
    };
    let scope = LearningQuizScopeDto {
        kind: LearningQuizScopeKind::Lesson,
        id: Some(lesson_id.clone()),
    };
    let first_call = run.calls();
    let started = std::time::Instant::now();
    let mut read = database.begin_read().await?;
    let job_id: Option<String> = sqlx::query_scalar(
        "SELECT id FROM jobs WHERE kind=? AND subject_id=? AND json_extract(requested_json,'$.lessonStage')=1 AND json_extract(requested_json,'$.scope.id')=? ORDER BY created_at DESC, id DESC LIMIT 1",
    )
    .bind(quiz_bank_jobs::QUIZ_BANK)
    .bind(program_id)
    .bind(&lesson_id)
    .fetch_optional(&mut *read)
    .await?;
    read.commit().await?;
    let mut staged = None;
    if let Some(job_id) = job_id {
        let id = run_lesson_quiz_job(run, database, &job_id).await?;
        let Some(report) = quiz_bank_jobs::lesson_stage_report(database, &id).await? else {
            let job = JobStore::new(database.clone()).get(&id).await?;
            let error = job
                .error_message
                .unwrap_or_else(|| job.status.as_str().to_owned());
            run.metrics["lessons"][selector]["quiz_error"] = json!(error);
            run.save()?;
            return Ok(());
        };
        staged = Some((id, report, run.calls(), started.elapsed().as_secs_f64()));
    }
    let top_up_started = std::time::Instant::now();
    let top_up_first = run.calls();
    let topped =
        quiz_bank_jobs::prepare_bank_now(database, &*run.model, program_id, &scope, false).await;
    let top_up_seconds = top_up_started.elapsed().as_secs_f64();
    let top_up = match topped {
        Ok(report) => report,
        Err(error) => {
            run.metrics["lessons"][selector]["quiz_error"] = json!(error.to_string());
            run.save()?;
            return Ok(());
        }
    };
    let seconds = started.elapsed().as_secs_f64();
    let cost = super::stages::stage_cost(&run.dir, first_call..run.calls(), seconds)?;
    let top_up_calls = run.calls() - top_up_first;
    let (lesson_stage, lesson_written, lesson_dropped) = match staged {
        Some((id, report, calls_after, lesson_seconds)) => (
            json!({
                "job_id": id, "status": "completed", "written": report.written,
                "dropped": report.dropped, "calls": calls_after - first_call,
                "seconds": lesson_seconds,
            }),
            report.written,
            report.dropped,
        ),
        None => (Value::Null, 0, Vec::new()),
    };
    let report = quiz_bank_jobs::BankReport {
        written: lesson_written + top_up.written,
        dropped: lesson_dropped
            .into_iter()
            .chain(top_up.dropped.clone())
            .collect(),
    };
    let lesson_text = recorded["lesson"]["blocks"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|block| block["body"].as_str())
        .collect::<Vec<_>>()
        .join("\n\n");
    let mut sources = BTreeMap::new();
    sources.insert(format!("lesson:{lesson_id}"), lesson_text);
    sources.extend(lesson_sources(database, program_id).await?);
    let (review, items) = read_bank(database, program_id, &lesson_id, &sources).await?;
    let (target, mix) = blueprint_mix(database, program_id, &scope).await?;
    let authored = report.written + report.dropped.len();
    let mut metrics = json!({
        "target": target,
        "bank_size": items.len(), "blueprint_mix": mix,
        "written": report.written, "dropped": report.dropped,
        "key_review": review, "calls": cost["calls"], "seconds": seconds, "cost": cost,
        "lesson_stage": lesson_stage,
        "top_up": {
            "written": top_up.written, "dropped": top_up.dropped, "calls": top_up_calls,
            "seconds": top_up_seconds,
        },
    });
    // Written and dropped by the checks or the blinded key review, both steps.
    if authored > 0 {
        metrics["validated"] = json!({"passed": report.written, "total": authored});
    }
    write_json(
        &file,
        &json!({
            "schema": BANK_SCHEMA, "selector": selector, "program_id": program_id,
            "lesson_id": lesson_id, "sources": sources, "metrics": metrics, "items": items,
        }),
    )
}

/// Run a lesson's quiz job as the app wires it, on its own runtime, until it
/// settles; returns the job that ran (a retry is a new job).
async fn run_lesson_quiz_job(run: &Run, database: &Database, job_id: &str) -> Result<String> {
    let store = JobStore::new(database.clone());
    let mut id = job_id.to_owned();
    match quiz_job_step(store.get(&id).await?.status) {
        QuizJobStep::Record => return Ok(id),
        QuizJobStep::Recover => {
            store
                .recover(quiz_bank_jobs::QUIZ_BANK, RecoveryPolicy::Requeue)
                .await?;
        }
        QuizJobStep::Retry => {
            id = store
                .retry(
                    &id,
                    &uuid::Uuid::new_v4().to_string(),
                    "Waiting to write quiz questions",
                )
                .await?
                .id;
        }
        QuizJobStep::Run => {}
    }
    let model = run.model.clone();
    let worker = quiz_bank_jobs::QuizBankWorker {
        database: database.clone(),
        load_llm: Arc::new(move || {
            let llm: Arc<dyn LLMPort> = model.clone();
            Box::pin(async move { Ok(pinned_for_test(llm)) })
        }),
    };
    let runtime = JobRuntime::new(database.clone());
    runtime
        .register(
            quiz_bank_jobs::QUIZ_BANK,
            Arc::new(worker),
            quiz_bank_jobs::quiz_job_config(),
        )
        .await?;
    runtime.dispatch(&id);
    while !store.get(&id).await?.status.is_finished() {
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
    }
    runtime.close();
    runtime.drain().await;
    Ok(id)
}

async fn lesson_sources(database: &Database, program_id: &str) -> Result<BTreeMap<String, String>> {
    let mut sources = BTreeMap::new();
    for source in LearningRepository::new(database.clone())
        .verification_sources(program_id)
        .await?
    {
        sources.insert(source.id.clone(), source.excerpt.clone());
    }
    Ok(sources)
}

async fn read_bank(
    database: &Database,
    program_id: &str,
    lesson_id: &str,
    sources: &BTreeMap<String, String>,
) -> Result<(BTreeMap<String, usize>, Vec<Value>)> {
    let mut tx = database.begin_read().await?;
    let rows: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT id,origin,review_status FROM learning_assessment_candidates WHERE program_id=? AND lesson_id=? ORDER BY id",
    )
    .bind(program_id)
    .bind(lesson_id)
    .fetch_all(&mut *tx)
    .await?;
    let mut review = BTreeMap::<String, usize>::new();
    let mut items = Vec::new();
    for (id, origin, status) in &rows {
        *review.entry(status.clone()).or_default() += 1;
        if status == "agreed" {
            let stored =
                LearningAssessmentRepository::stored_candidate_in(&mut tx, program_id, id).await?;
            items.push(eval_item(&stored, origin, lesson_id, sources));
        }
    }
    tx.commit().await?;
    Ok((review, items))
}

async fn blueprint_mix(
    database: &Database,
    program_id: &str,
    scope: &LearningQuizScopeDto,
) -> Result<(usize, BTreeMap<String, usize>)> {
    let mut tx = database.begin_write().await?;
    let requirements =
        match quiz_blueprints::ensure_blueprint_in(&mut tx, program_id, scope).await? {
            Some((blueprint, _)) => quiz_blueprints::requirements_in(&mut tx, &blueprint).await?,
            None => vec![],
        };
    tx.commit().await?;
    let mut mix = BTreeMap::<String, usize>::new();
    for requirement in &requirements {
        *mix.entry(format_name(requirement.format)).or_default() += requirement.count;
    }
    Ok((requirements.iter().map(|r| r.bank_target()).sum(), mix))
}

fn format_name(format: LearningItemFormat) -> String {
    serde_json::to_value(format)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default()
}

/// Where an item's quote is verbatim: the lesson's own text, else the first
/// cited source that holds it, else the first cited source (which the scorer
/// then reports as not verbatim).
fn quote_home(
    candidate: &LearningAssessmentCandidate,
    quote: &str,
    lesson_id: &str,
    sources: &BTreeMap<String, String>,
) -> Option<String> {
    if quote.trim().is_empty() {
        return candidate.source_version_ids.first().cloned();
    }
    let lesson = format!("lesson:{lesson_id}");
    if sources
        .get(&lesson)
        .is_some_and(|text| grade_rationale::verbatim(text, quote))
    {
        return Some(lesson);
    }
    candidate
        .source_version_ids
        .iter()
        .find(|id| {
            sources
                .get(*id)
                .is_some_and(|text| grade_rationale::verbatim(text, quote))
        })
        .or(candidate.source_version_ids.first())
        .cloned()
}

fn numeric_json(numeric: &Option<LearningNumericKey>) -> Value {
    numeric.as_ref().map_or(
        Value::Null,
        |n| json!({"value": n.value, "tolerance": n.tolerance, "unit_variants": n.unit_variants}),
    )
}

/// A bank item in the eval's item shape (the header of
/// `scripts/course_eval_quiz.py`), with its key.
pub(super) fn eval_item(
    stored: &StoredCandidate,
    origin: &str,
    lesson_id: &str,
    sources: &BTreeMap<String, String>,
) -> Value {
    let c = &stored.candidate;
    let mut item = json!({
        "id": c.id, "format": format_name(c.format), "prompt": c.prompt, "points": 1.0,
        "explanation": c.explanation, "origin": origin, "difficulty": c.difficulty,
        "outcome_ids": c.outcome_ids, "source_quote": stored.source_quote,
        "source_version_id": quote_home(c, &stored.source_quote, lesson_id, sources),
    });
    match &c.key {
        LearningAssessmentKey::Choice(key) => {
            item["options"] = json!(c.options);
            item["key"] = json!([key]);
            item["distractor_rationales"] = json!(c.distractor_rationales);
        }
        LearningAssessmentKey::Choices(keys) => {
            item["options"] = json!(c.options);
            item["key"] = json!(keys.iter().collect::<Vec<_>>());
            item["distractor_rationales"] = json!(c.distractor_rationales);
        }
        LearningAssessmentKey::Text(key) => {
            item["variants"] = json!(key.variants);
            item["numeric"] = numeric_json(&key.numeric);
        }
        LearningAssessmentKey::Cloze(blanks) => {
            item["blanks"] = json!(blanks.iter().map(|b| &b.variants).collect::<Vec<_>>());
            item["blank_numeric"] = json!(blanks
                .iter()
                .map(|b| numeric_json(&b.numeric))
                .collect::<Vec<_>>());
        }
        LearningAssessmentKey::Matching(pairs) => {
            item["left"] = json!(c.left);
            item["right"] = json!(c.options);
            item["key"] = json!(pairs);
        }
        LearningAssessmentKey::Ordering(order) => {
            item["values"] = json!(order);
            item["shown"] = json!(c.options);
        }
        LearningAssessmentKey::Rubric => {
            item["rubric"] = json!(c
                .rubric
                .iter()
                .map(|r| json!({"id": r.id, "title": r.title, "description": r.description, "max_points": r.max_points}))
                .collect::<Vec<_>>());
            item["points"] = json!(c.rubric.iter().map(|r| r.max_points).sum::<u32>());
        }
    }
    item
}

// --- the grading set -----------------------------------------------------------

fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|v| v.as_str().map(str::to_owned))
        .collect()
}

fn indices(value: &Value) -> Vec<usize> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|v| v.as_u64().map(|n| n as usize))
        .collect()
}

fn text_key(variants: &Value, numeric: &Value) -> LearningTextKey {
    LearningTextKey {
        variants: strings(variants),
        numeric: numeric.is_object().then(|| LearningNumericKey {
            value: numeric["value"].as_f64().unwrap_or_default(),
            tolerance: numeric["tolerance"].as_f64().unwrap_or_default(),
            unit_variants: strings(&numeric["unit_variants"]),
        }),
    }
}

/// A grading-set bank item as the engine's candidate. A rubric criterion's
/// levels become its description, strongest first.
pub(super) fn eval_candidate(item: &Value) -> Result<LearningAssessmentCandidate> {
    let format: LearningItemFormat = serde_json::from_value(item["format"].clone())?;
    let first = |v: &Value| {
        indices(v)
            .first()
            .copied()
            .ok_or_else(|| refuse(format!("{} has no key", item["id"])))
    };
    let (options, left, key, rubric) = match format {
        LearningItemFormat::SingleChoice | LearningItemFormat::TrueFalse => (
            strings(&item["options"]),
            vec![],
            LearningAssessmentKey::Choice(first(&item["key"])?),
            vec![],
        ),
        LearningItemFormat::MultiSelect => (
            strings(&item["options"]),
            vec![],
            LearningAssessmentKey::Choices(indices(&item["key"]).into_iter().collect()),
            vec![],
        ),
        LearningItemFormat::ShortAnswer => (
            vec![],
            vec![],
            LearningAssessmentKey::Text(text_key(&item["variants"], &item["numeric"])),
            vec![],
        ),
        LearningItemFormat::Cloze => (
            vec![],
            vec![],
            LearningAssessmentKey::Cloze(
                item["blanks"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .enumerate()
                    .map(|(n, blank)| text_key(blank, &item["blank_numeric"][n]))
                    .collect(),
            ),
            vec![],
        ),
        LearningItemFormat::Matching => (
            strings(&item["right"]),
            strings(&item["left"]),
            LearningAssessmentKey::Matching(indices(&item["key"])),
            vec![],
        ),
        LearningItemFormat::Ordering => {
            let values = strings(&item["values"]);
            (
                values.clone(),
                vec![],
                LearningAssessmentKey::Ordering(values),
                vec![],
            )
        }
        LearningItemFormat::Explanation => {
            let rubric: Vec<LearningRubricCriterion> = item["rubric"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|c| {
                    let levels = &c["levels"];
                    let description = c["description"].as_str().map_or_else(
                        || {
                            ["strong", "adequate", "incomplete"]
                                .iter()
                                .map(|l| format!("{l}: {}", levels[*l].as_str().unwrap_or("")))
                                .collect::<Vec<_>>()
                                .join("; ")
                        },
                        str::to_owned,
                    );
                    LearningRubricCriterion {
                        id: c["id"].as_str().unwrap_or_default().to_owned(),
                        title: c["title"].as_str().unwrap_or_default().to_owned(),
                        description,
                        max_points: c["max_points"].as_u64().unwrap_or(1) as u32,
                    }
                })
                .collect();
            (vec![], vec![], LearningAssessmentKey::Rubric, rubric)
        }
        LearningItemFormat::Artifact => {
            return Err(refuse("the grading set holds no artifact items"))
        }
    };
    Ok(LearningAssessmentCandidate {
        id: item["id"].as_str().unwrap_or_default().to_owned(),
        outcome_ids: vec![],
        format,
        difficulty: 3,
        prompt: item["prompt"].as_str().unwrap_or_default().to_owned(),
        options,
        left,
        source_version_ids: vec![],
        rubric,
        key,
        explanation: item["explanation"].as_str().unwrap_or_default().to_owned(),
        distractor_rationales: vec![],
    })
}

/// A grading-set response, already in authored terms.
pub(super) fn eval_response(item_id: &str, response: &Value) -> LearningAssessmentResponse {
    let mut out = LearningAssessmentResponse::empty(item_id);
    out.selected_indices = indices(&response["selected_indices"]);
    out.text = response["text"].as_str().map(str::to_owned);
    out.blank_responses = strings(&response["blank_responses"]);
    out.matches = response["matches"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|v| v.as_u64().map(|n| n as usize))
        .collect();
    out.ordered_values = strings(&response["ordered_values"]);
    out
}

/// The points an item is worth: its rubric's, or one for a keyed item.
pub(super) fn possible_points(candidate: &LearningAssessmentCandidate, item: &Value) -> f64 {
    if candidate.format == LearningItemFormat::Explanation {
        candidate
            .rubric
            .iter()
            .map(|r| f64::from(r.max_points))
            .sum()
    } else {
        item["points"].as_f64().unwrap_or(1.0)
    }
}
