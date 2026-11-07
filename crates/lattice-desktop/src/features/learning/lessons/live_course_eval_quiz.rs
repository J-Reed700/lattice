//! The course eval's quiz hooks (docs/design/2026-10-23-quizzes-and-tests.md
//! §7; protocol in evals/courses/README.md):
//!
//! - the `quiz` stage: after a lesson is recorded Ready, write its bank with
//!   the app's own authoring path and record it, keys included, as
//!   `quiz-<sel>.json` (`course-eval-quiz-bank/1`) with the bank's numbers;
//! - the grading set: grade every answer of a committed set with the app's own
//!   graders and write `quiz-grading-grades/1`, the model's raw output kept
//!   beside the checked rationale, with three regrades of each contested
//!   answer.
//!
//! Both record outcomes and never assert on one; `scripts/course_eval.py` and
//! `scripts/course_eval_grading.py` score what they write.
use super::*;
use crate::features::learning::assessment_engine::{
    self as engine, KeyedOutcome, LearningAssessmentCandidate, LearningAssessmentKey,
    LearningAssessmentResponse, LearningItemFormat, LearningNumericKey, LearningRubricCriterion,
    LearningTextKey,
};
use crate::features::learning::assessment_generation::{
    self as generation, RawGrade, ShortAnswer, WrittenAnswer,
};
use crate::features::learning::assessment_repository::{
    LearningAssessmentRepository, StoredCandidate,
};
use crate::features::learning::grade_rationale::{self, Checked, Grader, GraderVerdict, Reference};
use crate::features::learning::{quiz_bank_jobs, quiz_blueprints};

const BANK_SCHEMA: &str = "course-eval-quiz-bank/1";
const GRADES_SCHEMA: &str = "quiz-grading-grades/1";
const GRADING_DIR: &str = "evals/courses/grading";
/// Section 7.2: each contested answer is regraded three times.
const REGRADES: usize = 3;

// --- the quiz stage ----------------------------------------------------------

/// Write and record a Ready lesson's bank. A recorded `quiz-<sel>.json` means
/// the stage is done; a failure is recorded in `metrics.json` as
/// `quiz_error` without a file, so the next run of the request retries it.
pub(super) async fn quiz_stage(
    run: &mut Run,
    pool: &sqlx::SqlitePool,
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
    let written =
        quiz_bank_jobs::prepare_bank_now(pool, &*run.model, program_id, &scope, false).await;
    let seconds = started.elapsed().as_secs_f64();
    let report = match written {
        Ok(report) => report,
        Err(error) => {
            run.metrics["lessons"][selector]["quiz_error"] = json!(error.to_string());
            run.save()?;
            return Ok(());
        }
    };
    let cost = super::stages::stage_cost(&run.dir, first_call..run.calls(), seconds)?;
    let lesson_text = recorded["lesson"]["blocks"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|block| block["body"].as_str())
        .collect::<Vec<_>>()
        .join("\n\n");
    let mut sources = BTreeMap::new();
    sources.insert(format!("lesson:{lesson_id}"), lesson_text);
    for source in LearningRepository::new(pool.clone())
        .verification_sources(program_id)
        .await?
    {
        sources.insert(source.id.clone(), source.excerpt.clone());
    }

    let mut connection = pool.acquire().await.map_err(db)?;
    let rows: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT id,origin,review_status FROM learning_assessment_candidates WHERE program_id=? AND lesson_id=? ORDER BY id",
    )
    .bind(program_id)
    .bind(&lesson_id)
    .fetch_all(&mut *connection)
    .await
    .map_err(db)?;
    let mut review = BTreeMap::<String, usize>::new();
    let mut items = Vec::new();
    for (id, origin, status) in &rows {
        *review.entry(status.clone()).or_default() += 1;
        if status == "agreed" {
            let stored =
                LearningAssessmentRepository::stored_candidate_in(&mut connection, program_id, id)
                    .await?;
            items.push(eval_item(&stored, origin, &lesson_id, &sources));
        }
    }
    drop(connection);
    let mut tx = pool.begin().await.map_err(db)?;
    let requirements =
        match quiz_blueprints::ensure_blueprint_in(&mut tx, program_id, &scope).await? {
            Some((blueprint, _)) => quiz_blueprints::requirements_in(&mut tx, &blueprint).await?,
            None => vec![],
        };
    tx.commit().await.map_err(db)?;
    let mut mix = BTreeMap::<String, usize>::new();
    for requirement in &requirements {
        *mix.entry(format_name(requirement.format)).or_default() += requirement.count;
    }
    let authored = report.written + report.dropped.len();
    let mut metrics = json!({
        "target": requirements.iter().map(|r| 3 * r.count).sum::<usize>(),
        "bank_size": items.len(), "blueprint_mix": mix,
        "written": report.written, "dropped": report.dropped,
        "key_review": review, "calls": cost["calls"], "seconds": seconds, "cost": cost,
    });
    // Written here and dropped by the checks or the blinded key review; the
    // lesson job's own bank stage reports its numbers in its job result.
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

fn verdict_name(verdict: GraderVerdict) -> &'static str {
    match verdict {
        GraderVerdict::Met => "met",
        GraderVerdict::PartlyMet => "partly_met",
        GraderVerdict::Missed => "missed",
    }
}

/// The model's own answer before any check, in the rationale's shape, so the
/// scorer measures what the model did rather than what the checks repaired.
fn raw_json(raw: &RawGrade, rubric: &[LearningRubricCriterion], version: Option<&str>) -> Value {
    json!({
        "summary": raw.item.summary,
        "criteria": raw.item.criteria.iter().map(|c| {
            let criterion = rubric.iter().find(|r| r.id == c.criterion_id);
            json!({
                "criterion_id": c.criterion_id, "title": criterion.map(|r| r.title.as_str()),
                "verdict": verdict_name(c.verdict), "points": c.points,
                "max_points": criterion.map(|r| r.max_points),
                "what_was_missing": c.what_was_missing, "your_words": c.your_words,
                "full_credit_includes": c.full_credit_includes,
                "source_quote": c.source_quote.as_deref().filter(|q| !q.trim().is_empty()),
                "source_version_id": version,
            })
        }).collect::<Vec<_>>(),
    })
}

fn shown_json(rationale: &LearningGradeRationaleDto) -> Value {
    json!({
        "summary": rationale.summary,
        "criteria": rationale.criteria.iter().map(|c| json!({
            "criterion_id": c.criterion_id, "title": c.title, "verdict": c.verdict, "points": c.points,
            "max_points": c.max_points, "what_was_missing": c.what_was_missing, "your_words": c.your_words,
            "full_credit_includes": c.full_credit_includes, "source_quote": c.source_quote,
            "source_version_id": c.source_version_id,
        })).collect::<Vec<_>>(),
        "grader_model": rationale.grader_model, "rubric_version": rationale.rubric_version,
        "graded_at": rationale.graded_at,
    })
}

/// One model grade, exactly as `quiz_bank_jobs::grade_written_answer` and
/// `judge_short_answer` give it (the model's grade, then
/// `grade_rationale::check`), unrolled so the raw output is kept.
async fn model_grade(
    llm: &dyn LLMPort,
    candidate: &LearningAssessmentCandidate,
    item: &Value,
    answer: &str,
    note: Option<&str>,
) -> Result<Value> {
    let quote = item["source_quote"].as_str().unwrap_or_default();
    let version = item["source_version_id"].as_str();
    let (raw, rubric, reference, version) = if candidate.format == LearningItemFormat::Explanation {
        let written = WrittenAnswer {
            item_id: candidate.id.clone(),
            prompt: candidate.prompt.clone(),
            rubric: candidate.rubric.clone(),
            answer: answer.to_owned(),
            reference: quote.to_owned(),
            learner_note: note.map(str::to_owned),
        };
        let raw = generation::grade_written(llm, std::slice::from_ref(&written)).await?;
        (raw, candidate.rubric.clone(), quote.to_owned(), version)
    } else {
        let LearningAssessmentKey::Text(key) = &candidate.key else {
            return Err(refuse(format!("{} is not model-graded", candidate.id)));
        };
        let reference = if quote.trim().is_empty() {
            candidate.explanation.clone()
        } else {
            quote.to_owned()
        };
        let short = ShortAnswer {
            item_id: candidate.id.clone(),
            prompt: candidate.prompt.clone(),
            accepted: key.variants.clone(),
            answer: answer.to_owned(),
            reference: reference.clone(),
            learner_note: note.map(str::to_owned),
        };
        let raw = generation::judge_short_answers(llm, std::slice::from_ref(&short)).await?;
        (raw, grade_rationale::equivalence_rubric(), reference, None)
    };
    let raw = raw.into_iter().find(|r| r.item_id == candidate.id);
    let checked = match &raw {
        None => Checked::Unexplained(grade_rationale::UNEXPLAINED.into()),
        Some(raw) => grade_rationale::check(
            &rubric,
            answer,
            (!reference.trim().is_empty()).then_some(Reference {
                text: &reference,
                version_id: version,
            }),
            raw.item.clone(),
            &Grader {
                model: llm.model_name(),
                graded_at: now_ms() / 1000,
            },
        ),
    };
    let possible = possible_points(candidate, item);
    let mut entry = match checked {
        Checked::Graded {
            rationale,
            fraction,
            status,
        } => json!({
            "status": "graded", "error": null, "points_earned": fraction * possible,
            "points_possible": possible, "grade_status": status, "rationale": shown_json(&rationale),
        }),
        Checked::Unexplained(reason) => json!({
            "status": "couldnt_grade", "error": reason, "points_earned": null, "points_possible": possible,
            "grade_status": "pending", "rationale": {},
        }),
    };
    entry["path"] = json!("model");
    entry["raw"] = raw
        .as_ref()
        .map_or(Value::Null, |r| raw_json(r, &rubric, version));
    entry["response_to_note"] = json!(raw.and_then(|r| r.response_to_note));
    Ok(entry)
}

/// Every grade of one row: its first grade and, for a contested model-graded
/// answer, three regrades with the note given as data.
async fn grade_row(llm: &dyn LLMPort, row: &Value, item: &Value) -> Result<Vec<Value>> {
    let candidate = eval_candidate(item)?;
    let response = eval_response(&candidate.id, &row["response"]);
    let answer = response.text.clone().unwrap_or_default();
    let possible = possible_points(&candidate, item);
    let keyed = if candidate.format == LearningItemFormat::Explanation {
        // An empty written answer scores 0 in code, never at the model.
        (!response.is_answered()).then(|| (0.0, engine::NO_ANSWER.to_owned()))
    } else {
        match engine::grade_keyed(&candidate, &response)? {
            KeyedOutcome::Graded(grade) => Some((grade.fraction, grade.detail)),
            KeyedOutcome::NeedsJudge => None,
        }
    };
    let id = row["id"].clone();
    if let Some((fraction, detail)) = keyed {
        return Ok(vec![json!({
            "row_id": id, "pass": "first", "seed": 0, "path": "deterministic", "status": "graded",
            "error": null, "points_earned": fraction * possible, "points_possible": possible,
            "grade_status": "deterministic", "detail": detail, "rationale": {}, "raw": null,
            "response_to_note": null,
        })]);
    }
    let mut first = model_grade(llm, &candidate, item, &answer, None).await?;
    first["row_id"] = id.clone();
    first["pass"] = json!("first");
    first["seed"] = json!(0);
    let mut grades = vec![first];
    if let Some(note) = row["contest_note"]
        .as_str()
        .filter(|n| !n.trim().is_empty())
    {
        for seed in 0..REGRADES {
            let mut regrade = model_grade(llm, &candidate, item, &answer, Some(note)).await?;
            regrade["row_id"] = id.clone();
            regrade["pass"] = json!("regrade");
            regrade["seed"] = json!(seed);
            grades.push(regrade);
        }
    }
    Ok(grades)
}

/// The rows and bank of one registered grading set, with their file hashes.
pub(super) fn grading_set(name: &str) -> Result<(Vec<Value>, Value, String, String)> {
    let dir = repo_root().join(GRADING_DIR);
    let registry = read_json(&dir.join("sets.json"))?
        .ok_or_else(|| refuse("evals/courses/grading/sets.json is missing"))?;
    let entry = &registry["sets"][name];
    let (Some(rows), Some(bank)) = (entry["rows"].as_str(), entry["bank"].as_str()) else {
        return Err(refuse(format!("no grading set {name:?} in sets.json")));
    };
    let rows_path = dir.join(rows);
    let lines = std::fs::read_to_string(&rows_path)?
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(serde_json::from_str::<Value>)
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let bank_path = dir.join(bank);
    let bank = read_json(&bank_path)?.ok_or_else(|| refuse(format!("{bank} is missing")))?;
    Ok((
        lines,
        bank,
        file_sha256(&rows_path)?,
        file_sha256(&bank_path)?,
    ))
}

#[tokio::test]
#[ignore = "quiz grading set: requires LATTICE_COURSE_RECIPE and LATTICE_QUIZ_GRADING_RUN; makes real model requests"]
async fn live_quiz_grading_eval() -> Result<()> {
    let env = |name: &str| std::env::var(name).ok();
    let required = |name: &str| env(name).ok_or_else(|| refuse(format!("{name} is not set")));
    let recipe_text = std::fs::read_to_string(required("LATTICE_COURSE_RECIPE")?)?;
    let recipe = CourseRecipe::parse(&recipe_text)?;
    let set = env("LATTICE_QUIZ_GRADING_SET").unwrap_or_else(|| "quiz-grading-v1".into());
    let dir = PathBuf::from(required("LATTICE_QUIZ_GRADING_RUN")?);
    std::fs::create_dir_all(&dir)?;
    let (rows, bank, rows_sha, bank_sha) = grading_set(&set)?;
    let items: BTreeMap<String, Value> = bank["items"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|item| item["id"].as_str().map(|id| (id.to_owned(), item.clone())))
        .collect();
    let (url, auth) = recipe.connection(&env)?;
    let settings = recipe.llm_settings(&url, auth.as_ref())?;
    let identity = probe(&settings, recipe.server.expected_slots).await?;
    let model = ObservedModel::new(&settings, dir.clone(), next_call(&dir)?).await?;
    let recipe_sha = sha256_hex(recipe_text.as_bytes());
    let previous = read_json(&dir.join("run.json"))?;
    if let Some(previous) = &previous {
        if previous["recipe_sha256"] != recipe_sha.as_str()
            || previous["rows_sha256"] != rows_sha.as_str()
        {
            return Err(refuse(
                "this run directory graded another recipe or set version",
            ));
        }
    }
    let mut manifest = json!({
        "schema": "quiz-grading-run/1", "set": set, "rows_sha256": rows_sha, "bank_sha256": bank_sha,
        "recipe_sha256": recipe_sha, "recipe": serde_json::from_str::<Value>(&recipe_text)?,
        "binary_sha256": file_sha256(&std::env::current_exe()?)?, "commit": git(&["rev-parse", "HEAD"]),
        "server": {"identity": identity, "url_sha256": sha256_hex(url.as_bytes()), "thinking": recipe.server.thinking},
        "settings": recorded_settings(&settings)?,
        "started_at": previous.as_ref().map_or(json!(chrono::Utc::now().to_rfc3339()), |p| p["started_at"].clone()),
        "finished_at": null,
    });
    write_json(&dir.join("run.json"), &manifest)?;

    // Resumable: a row's grades are appended together once all are in.
    let partial = dir.join("grades.partial.jsonl");
    let mut grades: Vec<Value> = match std::fs::read_to_string(&partial) {
        Ok(text) => text
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(serde_json::from_str::<Value>)
            .collect::<std::result::Result<_, _>>()?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(error) => return Err(error.into()),
    };
    let done: HashSet<String> = grades
        .iter()
        .filter_map(|g| g["row_id"].as_str().map(str::to_owned))
        .collect();
    for row in &rows {
        let id = row["id"].as_str().unwrap_or_default();
        if done.contains(id) {
            continue;
        }
        let item = items
            .get(row["item"].as_str().unwrap_or_default())
            .ok_or_else(|| refuse(format!("{id}: its item is not in the bank")))?;
        let entries = grade_row(&model, row, item).await?;
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&partial)?;
        for entry in &entries {
            use std::io::Write;
            writeln!(file, "{entry}")?;
        }
        grades.extend(entries);
    }
    write_json(
        &dir.join("grades.json"),
        &json!({
            "schema": GRADES_SCHEMA, "set": set, "rows_sha256": rows_sha, "bank_sha256": bank_sha,
            "grader": {"model": recipe.server.model,
                       "model_revision": recipe.server.gguf_sha256.clone().or(recipe.server.model_revision.clone()),
                       "rubric_version": "per grade (grade_rationale::rubric_version)",
                       "recipe_sha256": manifest["recipe_sha256"], "commit": manifest["commit"]},
            "grades": grades,
        }),
    )?;
    manifest["finished_at"] = json!(chrono::Utc::now().to_rfc3339());
    write_json(&dir.join("run.json"), &manifest)?;
    println!(
        "QUIZ GRADING DONE: {} rows graded into {}",
        rows.len(),
        dir.join("grades.json").display()
    );
    Ok(())
}
